# sounds-proxy

A proxy server for BBC Sounds podcasts. Opens access to episodes which are normally exclusive to BBC Sounds so that they can be accessed via other podcast applications.

## Build

You'll need the [build dependencies for `ffmpeg-next`](https://github.com/zmwangx/rust-ffmpeg/wiki/Notes-on-building), then `cargo build`.

Or you can use the Dockerfile.

## Usage

Configuration is via environment variables.

| Variable | Description | Default |
| --- | --- | --- |
| SOUNDS_PROXY_LISTEN_PORT | Listen port | 8080 |
| SOUNDS_PROXY_BASE_URL | Base URL (so it can be returned in the podcast feed) | None |
| SOUNDS_PROXY_S3_BUCKET | If specified, episodes will be saved to, and served from, this bucket | None |
| SOUNDS_PROXY_S3_BASE_URL | Base URL for the S3 bucket (or a proxy etc) | https://<bucket-name>.s3.<region>.amazonaws.com/ |

Then run `sounds-proxy`.

To request a podcast feed, you'll need the show's ID. This ID will be the last element of the show's URL on BBC Sounds.
Request http://localhost:8080/shows/<show-id> to get the feed (adjusting for your base URL as appropriate).

There's a demo version hosted at https://sounds.errsuccess.com/.

## Deploy

Run the `sounds-proxy` binary or the Docker image.

You could also [use AWS Lambda](terraform-sounds-proxy-lambda/README.md).

## Caveats

BBC Sounds audio is AAC ADTS audio in an MPEG-TS container served via HLS. For improved compatibility this is remuxed on the fly to a raw ADTS AAC audio file, but this still may not be supported by some podcast players. No format conversion is performed as this would be computationally expensive.
