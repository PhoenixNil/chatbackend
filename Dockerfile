# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system app \
    && useradd --system --gid app --home-dir /app --create-home app

WORKDIR /app
COPY --from=builder /app/target/release/chatbackend /usr/local/bin/chatbackend

ENV APP_HOST=0.0.0.0 \
    APP_PORT=8080 \
    RUST_LOG=info,sqlx=warn,tower_http=info

EXPOSE 8080
USER app

ENTRYPOINT ["chatbackend"]
