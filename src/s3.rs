use std::io::Error;
use std::{pin::Pin, task::Poll};

use async_trait::async_trait;
use futures::Stream;
use s3::Bucket;
use s3::error::S3Error;
use tokio::io::{AsyncRead, ReadBuf};

struct AsyncReadStream<S>
where
    S: Stream<Item = Result<Vec<u8>, Error>> + Unpin,
{
    stream: Option<S>,
}

impl<S> AsyncReadStream<S>
where
    S: Stream<Item = Result<Vec<u8>, Error>> + Unpin,
{
    fn new(stream: S) -> Self {
        Self {
            stream: Some(stream),
        }
    }
}

impl<S> AsyncRead for AsyncReadStream<S>
where
    S: Stream<Item = Result<Vec<u8>, Error>> + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let self_mut = self.get_mut();
        let mut stream = self_mut.stream.take().unwrap();

        let result = match Pin::new(&mut stream).poll_next(cx) {
            Poll::Ready(Some(Ok(data))) => {
                buf.put_slice(&data);
                Poll::Ready(Ok(()))
            }

            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(e)),

            // EOF
            Poll::Ready(None) => Poll::Ready(Ok(())),

            Poll::Pending => Poll::Pending,
        };

        self_mut.stream = Some(stream);

        result
    }
}

#[async_trait]
trait BucketPutStream {
    async fn try_put_async_stream(&self, stream: impl Stream<Item = Result<Vec<u8>, Error>> + Unpin + Send + 'async_trait, s3_path: &str) -> Result<u16, S3Error>;
}

#[async_trait]
impl BucketPutStream for Bucket {

    async fn try_put_async_stream(&self, stream: impl Stream<Item = Result<Vec<u8>, Error>> + Unpin + Send + 'async_trait, s3_path: &str) -> Result<u16, S3Error> {
        let (_, code) = self.head_object(s3_path).await?;
        // TODO inspect the head in some way?
        if code == 404 {
            let mut wrapped_stream = AsyncReadStream::new(stream);
            Ok(self.put_object_stream(&mut wrapped_stream, s3_path).await?)
        } else {
            Ok(code)
        }
    }

}
