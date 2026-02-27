# ---- Build stage ----
FROM rust:1-bookworm AS builder

WORKDIR /build

# Cache dependencies: copy manifests first, build a dummy to populate cache
COPY Cargo.toml Cargo.lock ./
COPY crates/camgrab-core/Cargo.toml crates/camgrab-core/Cargo.toml
COPY crates/camgrab-cli/Cargo.toml  crates/camgrab-cli/Cargo.toml
COPY crates/camgrab-daemon/Cargo.toml crates/camgrab-daemon/Cargo.toml

# Create dummy source files so cargo can resolve the workspace
RUN mkdir -p crates/camgrab-core/src && echo "pub fn _dummy() {}" > crates/camgrab-core/src/lib.rs \
 && mkdir -p crates/camgrab-cli/src  && echo "fn main() {}" > crates/camgrab-cli/src/main.rs \
 && mkdir -p crates/camgrab-daemon/src && echo "pub fn _dummy() {}" > crates/camgrab-daemon/src/lib.rs

RUN cargo build --release -p camgrab-cli 2>/dev/null || true

# Copy actual source and rebuild
COPY . .
# Touch source files so cargo detects changes over the dummies
RUN touch crates/camgrab-core/src/lib.rs crates/camgrab-cli/src/main.rs crates/camgrab-daemon/src/lib.rs

RUN cargo build --release -p camgrab-cli && \
    strip target/release/camgrab

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Run as non-root
RUN groupadd -r camgrab && useradd -r -g camgrab -m camgrab

COPY --from=builder /build/target/release/camgrab /usr/local/bin/camgrab

# Default storage and config directories
RUN mkdir -p /var/lib/camgrab /etc/camgrab && \
    chown -R camgrab:camgrab /var/lib/camgrab /etc/camgrab

USER camgrab

ENV CAMGRAB_CONFIG=/etc/camgrab/config.toml

VOLUME ["/var/lib/camgrab", "/etc/camgrab"]

ENTRYPOINT ["camgrab"]
CMD ["--help"]
