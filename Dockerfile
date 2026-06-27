# Stage 1: Frontend build
FROM oven/bun:1-slim AS frontend
WORKDIR /app/client
COPY client/package.json ./
RUN bun install
COPY client/ .
RUN bun run build

# Stage 2: Rust build
FROM rust:1-slim AS backend
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# 依存を先にビルド（Cargo.toml/lock 変更時のみ再実行）
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY src/ src/
RUN touch src/main.rs && cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=backend /app/target/release/viewer-of-5ch /usr/local/bin/
COPY --from=frontend /app/client/build /app/client/build
WORKDIR /app
ENV PORT=3000
ENV DATABASE_PATH=/data/5ch-viewer.db
EXPOSE 3000
CMD ["viewer-of-5ch"]
