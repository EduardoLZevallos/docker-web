FROM rust:1.90-bookworm AS build
WORKDIR /src
COPY backend/Cargo.toml ./Cargo.toml
COPY backend/Cargo.lock ./Cargo.lock
COPY backend/src ./src
RUN cargo build --locked --release --bin docker-web

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/docker-web /usr/local/bin/docker-web
ENTRYPOINT ["/usr/local/bin/docker-web"]
