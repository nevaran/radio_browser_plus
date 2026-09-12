# Frontend builder: full Bookworm image (has the C toolchain `cargo install`
# needs) plus glibc for the Trunk/wasm-bindgen/wasm-opt downloads.
FROM rust:1.98.1-bookworm AS frontend-builder

WORKDIR /app

RUN rustup target add wasm32-unknown-unknown \
    && cargo install trunk --version ^0.21 --locked

COPY Cargo.toml Cargo.lock ./
COPY frontend ./frontend

WORKDIR /app/frontend
RUN trunk build --release

# Backend builder: static musl binary, as before.
FROM rust:1.98.1-alpine AS backend-builder

WORKDIR /app

# Install build dependencies (build-base provides the C toolchain needed to
# compile reqwest's TLS stack - aws-lc - from source for musl)
RUN apk add --no-cache build-base

COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Whole frontend member: cargo must load the full workspace (manifest + targets)
# even when only building the backend package.
COPY frontend ./frontend

RUN cargo build --release --locked

FROM alpine:latest AS runtime

RUN apk add --no-cache ca-certificates
RUN addgroup -S app && adduser -S -G app app

WORKDIR /app

COPY --from=backend-builder /app/target/release/radio_browser_plus ./radio_browser_plus
COPY --from=frontend-builder /app/dist ./dist
RUN mkdir -p /app/data && chown -R app:app /app

USER app

EXPOSE 8000
CMD ["./radio_browser_plus"]
