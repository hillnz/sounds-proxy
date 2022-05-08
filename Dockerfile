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

COPY --from=builder /usr/local/cargo/bin/sounds-proxy /usr/local/bin/sounds-proxy
ENTRYPOINT [ "sounds-proxy" ]
