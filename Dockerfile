# ------------------------------------------------------------------------------
# Build Stage: Compile static binary using rust:alpine with musl target
# ------------------------------------------------------------------------------
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev gcc make binutils

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY server server
COPY core-client core-client
COPY cli cli
COPY bench bench
COPY clients/python clients/python

# Build statically linked stripped release binary
RUN cargo build --release --target x86_64-unknown-linux-musl -p aerokv-server && \
    strip /build/target/x86_64-unknown-linux-musl/release/aerokv-server

# ------------------------------------------------------------------------------
# Runtime Stage: Empty 0-byte scratch image (< 8 MB total footprint)
# ------------------------------------------------------------------------------
FROM scratch

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/aerokv-server /aerokv-server

EXPOSE 6379/tcp

ENTRYPOINT ["/aerokv-server"]

