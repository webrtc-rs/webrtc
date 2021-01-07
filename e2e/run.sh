#!/bin/bash
set -e

TAG=rs-webrtc-test-harness:latest

# set these according to the tag documentation of https://hub.docker.com/_/rust
RUST_SEMVER=1.49.0
DISTRO=alpine3.11

cd $(git rev-parse --show-toplevel) 
docker build \
    --tag=$TAG \
    --build-arg="RUST_SEMVER=$RUST_SEMVER" \
    --build-arg="DISTRO=$DISTRO" \
    --file=./e2e/Dockerfile \
    .

docker run -ti --rm $TAG
