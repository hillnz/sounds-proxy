use aws_sdk_s3::{
    error::{HeadObjectError, HeadObjectErrorKind},
    model::{CompletedMultipartUpload, CompletedPart, ObjectCannedAcl},
    types::{ByteStream, SdkError},
    Client,
};
use bytes::{Buf, BytesMut, BufMut, Bytes};
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

// 5 MB is the minimum aws allows
const BUFFER_SIZE: usize = 0x500000;

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
        log::debug!("S3 object {} not found, uploading", s3_path);

        let upload = client
            .create_multipart_upload()
            .bucket(bucket_name)
            .key(s3_path)
            .acl(ObjectCannedAcl::PublicRead)
            .cache_control("public, max-age=604800") // 7 days
            .set_content_type(content_type.map(|s| s.to_string()))
            .send()
            .await?;

        let upload_id = upload.upload_id().unwrap();

        
        let upload_part = |buff: Bytes, part_number| async move {
            let len = buff.len();
            let _md5 = md5::compute(&buff);
            let body = ByteStream::from(buff);
            let part = client
                .upload_part()
                .bucket(bucket_name)
                .key(s3_path)
                .body(body)
                .content_length(len as i64)
                // .content_md5(md5.to_string())
                .upload_id(upload_id.to_string())
                .part_number(part_number)
                .send()
                .await?;

            Ok::<_, S3Error>((part_number, part.e_tag().unwrap().to_string()))
        };

        let mut stream = stream.fuse();

        let mut parts = Vec::new();
        let mut part_number = 1;
        let mut buff = BytesMut::with_capacity(BUFFER_SIZE);
        while let Some(data) = stream.next().await {
            let mut data = data?;

            while data.has_remaining() {

                if buff.len() < BUFFER_SIZE {
                    // buffer not full
                    let mut piece = data.take(BUFFER_SIZE - buff.len());
                    buff.put(&mut piece);
                    data = piece.into_inner();
                }

                if buff.len() >= BUFFER_SIZE {
                    // buffer full
                    parts.push(upload_part(buff.freeze(), part_number).await?);
                    part_number += 1;
                    buff = BytesMut::with_capacity(BUFFER_SIZE);
                }
            }

        }
        // final part
        if !buff.is_empty() {
            parts.push(upload_part(buff.freeze(), part_number).await?);
        }

        let multipart_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(
                parts
                    .into_iter()
                    .map(|(part_number, e_tag)| {
                        CompletedPart::builder()
                            .part_number(part_number)
                            .e_tag(e_tag)
                            .build()
                    })
                    .collect(),
            ))
            .build();

        log::debug!("{:?}", multipart_upload);

        client
            .complete_multipart_upload()
            .bucket(bucket_name)
            .key(s3_path)
            .upload_id(upload_id.to_string())
            .multipart_upload(multipart_upload)
            .send()
            .await?;
    }

    Ok(())
}
