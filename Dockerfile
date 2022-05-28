FROM rust:1.60.0 AS builder

RUN apt-get update && apt-get install -y \
    libavdevice-dev \
    libavfilter-dev \
    libavformat-dev \
    libavutil-dev \
    libclang-dev \
    libssl-dev

WORKDIR /usr/src/app
COPY . .
RUN cargo install --path .

FROM debian:stable-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    ffmpeg

# Adds support for running in Lambda
ENV READINESS_CHECK_PATH=/ok
COPY --from=public.ecr.aws/awsguru/aws-lambda-adapter:0.3.2 /lambda-adapter /opt/extensions/lambda-adapter

COPY --from=builder /usr/local/cargo/bin/sounds-proxy /usr/local/bin/sounds-proxy
ENTRYPOINT [ "sounds-proxy" ]
