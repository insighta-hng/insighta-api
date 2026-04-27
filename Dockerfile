FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    ca-certificates \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

RUN cargo build --release || true
RUN rm -rf target/release/.fingerprint/insighta_api-* \
    target/release/deps/insighta_api* \
    target/release/deps/libinsighta_api*

COPY src ./src
COPY seed_profiles.json ./

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/insighta_api /app/insighta_api
COPY --from=builder /app/seed_profiles.json /app/seed_profiles.json

ENV RUST_LOG=info

EXPOSE 8000

ENTRYPOINT ["/app/insighta_api"]
