FROM rust:1.95 AS builder
WORKDIR /reader
COPY . .
RUN rustup target add x86_64-unknown-linux-musl
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:3.21
COPY --from=builder /reader/target/x86_64-unknown-linux-musl/release/reader /usr/bin/reader
CMD ["reader"]
