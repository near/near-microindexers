FROM rust:1.64 AS builder
WORKDIR /tmp/

# this build step will cache your dependencies
COPY Cargo.lock ./
RUN echo '[workspace]\nmembers = ["indexer-balances"]' > Cargo.toml
COPY indexer-balances/Cargo.toml indexer-balances/Cargo.toml
RUN mkdir indexer-balances/src && touch indexer-balances/src/lib.rs && cargo build --release && rm -r indexer-balances/src

# copy your source tree
COPY ./indexer-balances/src ./indexer-balances/src

# build for release
RUN cargo build --release

FROM ubuntu:20.04
RUN apt update && apt install -yy openssl ca-certificates
COPY --from=builder /tmp/target/release/indexer-balances .
ENTRYPOINT ["./indexer-balances"]
