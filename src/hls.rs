use std::panic;
use std::{
    os::unix::prelude::AsRawFd,
    pin::Pin,
    task::{Context, Poll},
    thread,
};

use ffmpeg_next::codec::Id;
use ffmpeg_next::{codec, encoder, format, media};
use futures::{Future, FutureExt, Stream};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio_pipe::PipeRead;

#[derive(Error, Debug)]
pub enum HlsError {
    #[error("No audio stream found")]
    NoAudio,

    #[error("Unsupported codec (only AAC is supported)")]
    UnsupportedCodec,

    #[error("Ffmpeg Error: {0}")]
    FfmpegError(#[from] ffmpeg_next::error::Error),

    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
}

type Result<T, E = HlsError> = std::result::Result<T, E>;

type PollResult = Result<(Option<Vec<u8>>, PipeRead)>;

pub struct HlsStream {
    ff_thread: Option<thread::JoinHandle<Result<(), HlsError>>>,
    poll: Pin<Box<dyn Future<Output = PollResult>>>,
}

async fn poll_next_async(mut rx: PipeRead) -> PollResult {
    let mut buf = vec![0; 1024];
    let n = rx.read(&mut buf).await?;
    if n == 0 {
        return Ok((None, rx));
    }
    buf.truncate(n);
    Ok((Some(buf), rx))
}

impl HlsStream {
    pub fn new(url: String) -> Result<Self> {
        let (rx, tx) = tokio_pipe::pipe()?;

        let ff_thread = thread::spawn(move || {
            let out_pipe = format!("pipe:{}", tx.as_raw_fd());

            ffmpeg_next::init()?;

            let mut input = format::input(&url)?;
            let mut output = format::output_as(&out_pipe, "adts")?;

            let (audio_stream_index, audio_stream) = input
                .streams()
                .into_iter()
                .enumerate()
                .find(|(_, s)| s.parameters().medium() == media::Type::Audio)
                .ok_or(HlsError::NoAudio)?;

            if audio_stream.parameters().id() != Id::AAC {
                return Err(HlsError::UnsupportedCodec);
            }

            let time_base = audio_stream.time_base();

            {
                let mut output_stream = output.add_stream(encoder::find(codec::Id::None))?;
                output_stream.set_parameters(audio_stream.parameters());
                unsafe {
                    (*output_stream.parameters().as_mut_ptr()).codec_tag = 0;
                }
            }

            output.set_metadata(input.metadata().to_owned());
            output.write_header()?;

            for (stream, mut packet) in input.packets() {
                if stream.index() != audio_stream_index {
                    continue;
                }

                let output_stream = output.stream(0).unwrap();
                packet.rescale_ts(time_base, output_stream.time_base());
                packet.set_position(-1);
                packet.set_stream(0);
                packet.write_interleaved(&mut output)?;
            }

            output.write_trailer()?;

            Ok(())
        });

        let poll = Box::pin(poll_next_async(rx));

        Ok(HlsStream {
            ff_thread: Some(ff_thread),
            poll,
        })
    }
}

impl Stream for HlsStream {
    type Item = Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.poll.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Ok((Some(buf), rx))) => {
                self.poll = Box::pin(poll_next_async(rx));
                Poll::Ready(Some(Ok(buf)))
            }

            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),

            Poll::Ready(Ok((None, _))) => match self.ff_thread.take().unwrap().join() {
                Ok(result) => match result {
                    Ok(_) => Poll::Ready(None),
                    Err(e) => Poll::Ready(Some(Err(e))),
                },
                Err(e) => panic::resume_unwind(e),
            },
        }
    }
}
