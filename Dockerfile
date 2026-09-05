# syntax=docker/dockerfile:1

FROM oven/bun:1.3.10-slim AS frontend
WORKDIR /app/client
COPY client/package.json client/bun.lock ./
RUN bun install --frozen-lockfile
COPY client/ .
RUN bun run build

# Keep the planner and dependency/application builds on the same toolchain.
FROM rust:1.96-bookworm AS chef
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN cargo install cargo-chef --version 0.1.78 --locked

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
# Include src/bin so the recipe discovers every target.
COPY src/ src/
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS backend
# The recipe masks the local package version, preserving dependencies across tags.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --locked --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --locked --release --bin viewer-of-5ch --bin migrate-image-cache --bin resize-image-cache

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=backend /app/target/release/viewer-of-5ch /usr/local/bin/
COPY --from=backend /app/target/release/migrate-image-cache /usr/local/bin/
COPY --from=backend /app/target/release/resize-image-cache /usr/local/bin/
COPY --from=frontend /app/client/build /app/client/build
WORKDIR /app
ENV PORT=3000
ENV DATABASE_PATH=/data/5ch-viewer.db
ENV IMAGE_CACHE_DIR=/data/images
EXPOSE 3000
CMD ["viewer-of-5ch"]
