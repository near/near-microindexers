FROM rust:1.64 AS builder
WORKDIR /tmp/

# this build step will cache your dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && touch src/lib.rs && cargo build --release && rm -r src

# copy your source tree
COPY ./src ./src

# build for release
RUN cargo build --release

FROM ubuntu:20.04
RUN apt update && apt install -yy openssl ca-certificates
COPY --from=builder /tmp/target/release/indexer-balances .
ENTRYPOINT ["./indexer-balances"]
