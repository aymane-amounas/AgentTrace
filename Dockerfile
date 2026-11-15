# syntax=docker/dockerfile:1

# ---- Stage 1: build the viewer ----
FROM node:22-bookworm-slim AS viewer
WORKDIR /viewer
COPY viewer/package.json viewer/package-lock.json* ./
RUN npm install --no-audit --no-fund
COPY viewer/ ./
RUN npm run build

# ---- Stage 2: build the collector ----
FROM rust:1.83-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/
# Build only the collector (and its dependency, the SDK).
RUN cargo build --release -p agenttrace-collector

# ---- Stage 3: runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/agenttrace-collector /usr/local/bin/agenttrace-collector
COPY --from=viewer /viewer/dist /app/viewer

# Inside the container we bind to all interfaces; docker-compose publishes the
# port to loopback only, so the collector is never public by default.
ENV AGENTTRACE_HOST=0.0.0.0 \
    AGENTTRACE_PORT=4357 \
    AGENTTRACE_DB=/data/agenttrace.db \
    AGENTTRACE_VIEWER_DIR=/app/viewer \
    AGENTTRACE_RETENTION_DAYS=30

VOLUME ["/data"]
EXPOSE 4357

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -fsS http://127.0.0.1:4357/healthz || exit 1

ENTRYPOINT ["agenttrace-collector"]
