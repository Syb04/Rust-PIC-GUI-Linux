# ============================================================
# Rust-PIC Web — マルチステージビルド
# ============================================================

# --- stage 1: Rust 計算コア + Axum サーバーをビルド ---
FROM rust:1-bookworm AS rust-build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p rust-pic -p server

# --- stage 2: フロント (Vite) をビルド ---
FROM node:20-bookworm AS web-build
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN npm ci
COPY web/ ./
RUN npm run build

# --- stage 3: ランタイム ---
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# 計算バイナリ・サーバー・フロント・断面積データを配置
COPY --from=rust-build /build/target/release/server /app/server
COPY --from=rust-build /build/target/release/rust-pic /app/rust-pic
COPY --from=web-build /web/dist /app/web/dist
COPY xsec /app/xsec

ENV BIND_ADDR=0.0.0.0:8090 \
    RUST_PIC_BIN=/app/rust-pic \
    WEB_DIST=/app/web/dist \
    WORKSPACES_DIR=/app/workspaces \
    MAX_CONCURRENT=4 \
    MAX_JOBS=50

EXPOSE 8090
CMD ["/app/server"]
