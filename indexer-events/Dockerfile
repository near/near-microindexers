FROM rust:1.64 AS builder
WORKDIR /tmp/

# this build step will cache your dependencies
COPY Cargo.lock ./
RUN echo '[workspace]\nmembers = ["indexer-events", "indexer-opts"]' > Cargo.toml
COPY indexer-opts ./indexer-opts
COPY indexer-events/Cargo.toml indexer-events/Cargo.toml
RUN mkdir indexer-events/src && touch indexer-events/src/lib.rs && cargo build --release && rm -r indexer-events/src

# copy your source tree
COPY ./indexer-events/src ./indexer-events/src

# build for release
RUN cargo build --release

FROM ubuntu:20.04
RUN apt update && apt install -yy openssl ca-certificates
COPY --from=builder /tmp/target/release/indexer-events .
ENTRYPOINT ["./indexer-events"]
