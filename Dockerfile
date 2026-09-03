FROM rust:1.94-alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache musl-dev

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM alpine:latest AS runtime

RUN apk add --no-cache ca-certificates
RUN addgroup -S app && adduser -S -G app app

WORKDIR /app

COPY --from=builder /app/target/release/radio_browser_plus ./radio_browser_plus
COPY public ./public
COPY resources/flag-icons/flags/1x1 ./public/flags
RUN mkdir -p /app/data && chown -R app:app /app

USER app

EXPOSE 8000
CMD ["./radio_browser_plus"]
