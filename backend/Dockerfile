# syntax=docker/dockerfile:1

FROM rust:1.90-bookworm AS builder

WORKDIR /src
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
RUN cargo build --locked --release --bin docker-web

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY deploy/config/config.yaml /app/config/config.yaml
COPY --from=builder /src/target/release/docker-web /usr/local/bin/docker-web

USER 10001:10001
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/docker-web"]
