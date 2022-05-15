#!/usr/bin/env bash

set -e

docker_id=
proxy_pid=

finally() {
    if [ -n "$docker_id" ]; then
        docker stop "$docker_id"
    fi
    if [ -n "$proxy_pid" ]; then
        kill "$proxy_pid"
    fi
}

trap finally EXIT

docker_id=$(docker run -d -it --rm \
    -p 4566:4566 \
    -e LOCALSTACK_SERVICES=s3 \
    localstack/localstack)

while ! aws --endpoint-url=http://localhost:4566 s3 ls; do
    sleep 1
done

export AWS_ACCESS_KEY=test
export AWS_SECRET_KEY=test
export AWS_REGION=ap-southeast-2
export SOUNDS_PROXY_BASE_URL=http://localhost:3000
export SOUNDS_PROXY_S3_BUCKET=sounds-proxy-test
export RUST_LOG=sounds_proxy=debug

aws --endpoint-url=http://localhost:4566 s3 mb "s3://$SOUNDS_PROXY_S3_BUCKET" || true

cargo run &
proxy_pid=$!

sleep 5

curl -v http://localhost:8080/episode/p0bzn8f1.aac

aws --endpoint-url=http://localhost:4566 s3 ls "s3://$SOUNDS_PROXY_S3_BUCKET"
