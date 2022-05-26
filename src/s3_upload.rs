use aws_sdk_s3::{
    error::{HeadObjectError, HeadObjectErrorKind},
    types::{ByteStream, SdkError},
    Client,
};
use aws_smithy_http::body::SdkBody;
use bytes::Buf;
use futures::Stream;
use futures::StreamExt;

#[derive(Debug, thiserror::Error)]
pub enum S3Error {
    #[error("upload error")]
    UploadError,

    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    #[error("unknown error")]
    UnknownError,
}

impl<E> From<SdkError<E>> for S3Error
where
    E: std::error::Error,
{
    fn from(err: SdkError<E>) -> Self {
        log::error!("AWS SDK Error: {:?}", err);
        S3Error::UploadError
    }
}

impl From<hyper::Error> for S3Error {
    fn from(err: hyper::Error) -> Self {
        log::error!("Hyper Error: {:?}", err);
        S3Error::UploadError
    }
}

pub async fn try_put_async_stream<S, B>(
    client: &Client,
    bucket_name: &str,
    stream: S,
    s3_path: &str,
    content_type: Option<&str>,
) -> Result<(), S3Error>
where
    S: Stream<Item = Result<B, std::io::Error>> + Unpin,
    B: Buf,
{
    let head_result = client
        .head_object()
        .bucket(bucket_name)
        .key(s3_path)
        .send()
        .await;

    let found = match head_result {
        Ok(_) => Ok(true),
        Err(SdkError::ServiceError {
            err:
                HeadObjectError {
                    kind: HeadObjectErrorKind::NotFound(_),
                    ..
                },
            ..
        }) => Ok(false),
        Err(err) => Err(err),
    }?;

    if !found {
        let (mut tx, channel_body) = hyper::Body::channel();
        let byte_stream = ByteStream::new(SdkBody::from(channel_body));

        let copy_op = async move { // move is important to ensure tx will be dropped
            let mut stream = stream.fuse();
            while let Some(data) = stream.next().await {
                let mut data = data?;
                let data = data.copy_to_bytes(data.remaining());
                tx.send_data(data).await?;
            }
            Ok::<(), S3Error>(())
        };

        let put_op = client
            .put_object()
            .bucket(bucket_name)
            .key(s3_path)
            .body(byte_stream)
            .cache_control("public, max-age=604800") // 7 days
            .content_type(content_type.unwrap_or("application/octet-stream"))
            .send();

        let (put_result, copy_result) = futures::join!(put_op, copy_op);
        put_result?;
        copy_result?;
    }

    Ok(())
}
