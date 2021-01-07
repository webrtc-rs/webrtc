ARG RUST_SEMVER
ARG DISTRO

FROM rust:${RUST_SEMVER}-${DISTRO}

RUN apk add --no-cache \
  openssl

WORKDIR /workspace
COPY . .

ENTRYPOINT [ "/usr/local/cargo/bin/cargo", "test" ]
