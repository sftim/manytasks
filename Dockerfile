# Build Stage
FROM rust:1.56.0 AS builder
WORKDIR /usr/src/
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /usr/src/manytasks
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo install --target x86_64-unknown-linux-musl --path .

# Bundle Stage
FROM scratch
COPY --from=builder /usr/local/cargo/bin/manytasks /bin/manytasks
USER 1000
ENTRYPOINT ["/bin/manytasks"]
