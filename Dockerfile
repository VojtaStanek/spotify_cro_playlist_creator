# ---- Builder ----
FROM rust:1-bookworm AS builder

WORKDIR /app

# Copy the full source. Templates are needed at compile time (askama embeds them).
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates

RUN cargo build --release

# ---- Runtime ----
FROM debian:bookworm-slim AS runtime

# rustls is used for TLS, so only CA certificates are needed (no libssl).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/spotify_cro_playlist_creator /app/spotify_cro_playlist_creator

ENV PORT=8080
EXPOSE 8080

CMD ["/app/spotify_cro_playlist_creator"]
