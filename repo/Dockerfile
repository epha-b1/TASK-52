# === Builder stage ===
FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

RUN wget -qO- https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xzf - -C /usr/local/bin && chmod +x /usr/local/bin/trunk

COPY rust-toolchain.toml /app/rust-toolchain.toml
WORKDIR /app
RUN rustup target add wasm32-unknown-unknown

# Copy everything and build directly — no dep caching tricks
COPY . .
RUN cd src/frontend && trunk build
# Run Rust unit tests for backend + shared as part of the build.
# Backend tests cover: common::CivilDateTime, crypto AES round-trip, error
# envelope flattening, zip writer, traceability code checksum, supply parser,
# evidence compression policy, log sanitizer. Shared tests cover the draft
# autosave serialization + key constants used by the frontend.
RUN cargo test --release -p fieldtrace-backend -p fieldtrace-shared
RUN cargo build --release -p fieldtrace-backend

# === Runtime stage ===
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    libsqlite3-0 wget curl ca-certificates bash unzip jq \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/fieldtrace-backend /usr/local/bin/fieldtrace-backend
COPY --from=builder /app/src/frontend/dist /app/static
COPY --from=builder /app/migrations /app/migrations
COPY unit_tests/ /app/unit_tests/
COPY API_tests/ /app/API_tests/
RUN mkdir -p /app/storage

ENV PORT=8080
ENV DATABASE_URL="sqlite:///app/storage/app.db"
ENV STATIC_DIR=/app/static
ENV STORAGE_DIR=/app/storage
ENV RUST_LOG=info
EXPOSE 8080
CMD ["fieldtrace-backend"]
