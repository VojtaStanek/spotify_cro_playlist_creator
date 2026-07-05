# ---- Chef base ----
# cargo-chef lets us cache dependency compilation as its own layer, so editing
# first-party source doesn't recompile every crate from scratch.
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

# ---- Planner ----
# Produce a dependency "recipe" describing exactly which crates to build.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
RUN cargo chef prepare --recipe-path recipe.json

# ---- Builder ----
FROM chef AS builder
# Build (and cache) only the dependencies first.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
# Then build the application itself. Templates are needed at compile time
# (askama embeds them into the binary).
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
