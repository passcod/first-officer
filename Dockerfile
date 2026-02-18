FROM rust:bookworm AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y musl-tools && \
  rustup target add aarch64-unknown-linux-musl

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && \
  cargo build --release --target aarch64-unknown-linux-musl && \
  rm -rf src

COPY src ./src
RUN touch src/main.rs && \
  cargo build --release --target aarch64-unknown-linux-musl

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=builder /build/target/aarch64-unknown-linux-musl/release/first-officer /first-officer

EXPOSE 4141
ENTRYPOINT ["/first-officer"]
