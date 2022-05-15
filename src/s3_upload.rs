use bytes::Buf;
use futures::Stream;
use s3::error::S3Error;
use s3::Bucket;
use std::io::Error;
use tokio_util::io::StreamReader;

pub async fn try_put_async_stream<S, B>(
    bucket: &Bucket,
    stream: S,
    s3_path: &str,
) -> Result<u16, S3Error>
where
    S: Stream<Item = Result<B, Error>> + Unpin,
    B: Buf,
{
    let (_, code) = bucket.head_object(s3_path).await?;
    // TODO inspect the head in some way?
    if code == 404 {
        let mut reader = StreamReader::new(stream);
        let result = bucket.put_object_stream(&mut reader, s3_path).await?;
        Ok(result)
    } else {
        Ok(code)
    }
}
