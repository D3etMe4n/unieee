FROM docker.io/library/rust:1-bookworm AS builder
ARG BIN_NAME=enrollment-engine
WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake g++ pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY services services

RUN cargo build --release --locked --bin "${BIN_NAME}" \
    && cp "/workspace/target/release/${BIN_NAME}" /workspace/service

FROM docker.io/library/debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system unieee \
    && useradd --system --gid unieee --no-create-home unieee \
    && mkdir -p /var/lib/unieee/snapshots \
    && chown -R unieee:unieee /var/lib/unieee /app

COPY --from=builder --chown=unieee:unieee /workspace/service /app/service

USER unieee
EXPOSE 8081

ENTRYPOINT ["/app/service"]
