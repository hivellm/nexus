# syntax=docker/dockerfile:1.6
# Multi-stage Dockerfile for Nexus Graph Database
#
# HOW TO BUILD:
#   docker build -t nexus-graph-db:latest .
#   docker build -t nexus-graph-db:v0.11.0 -t nexus-graph-db:latest .
#
# The `# syntax=docker/dockerfile:1.6` header opts into the
# `RUN --mount=type=cache` frontend so the cargo registry + target
# directory are cached across rebuilds (see the build stage below).
# Works out of the box with BuildKit — the default Docker CLI since
# 23.0 and always on with `docker buildx build`. For older clients,
# export `DOCKER_BUILDKIT=1` before `docker build`.
#
# HOW TO RUN:
#   # Using docker run (basic):
#   docker run -d \
#     --name nexus \
#     -p 15474:15474 \
#     -v nexus-data:/app/data \
#     -e NEXUS_ROOT_USERNAME=admin \
#     -e NEXUS_ROOT_PASSWORD=secure_password \
#     -e NEXUS_AUTH_ENABLED=true \
#     nexus-graph-db:latest
#
#   # Using docker run with Docker secrets (recommended for production):
#   echo "secure_password" > secrets/root_password.txt
#   chmod 600 secrets/root_password.txt
#   docker run -d \
#     --name nexus \
#     -p 15474:15474 \
#     -v nexus-data:/app/data \
#     -v $(pwd)/secrets/root_password.txt:/run/secrets/nexus_root_password:ro \
#     -e NEXUS_ROOT_USERNAME=admin \
#     -e NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password \
#     -e NEXUS_AUTH_ENABLED=true \
#     -e NEXUS_DISABLE_ROOT_AFTER_SETUP=true \
#     nexus-graph-db:latest
#
#   # Using docker-compose (recommended):
#   docker-compose up -d
#
# HOW TO VERIFY:
#   curl http://localhost:15474/health
#   docker logs nexus
#
# For more details, see docs/guides/DEPLOYMENT_GUIDE.md

# Build stage
FROM rustlang/rust:nightly AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./

# Copy source for every workspace member declared in the root Cargo.toml.
# `cargo build --workspace` fails with "failed to load manifest for
# workspace member" if any member directory is missing — notably nexus-cli,
# which was absent from this Dockerfile previously.
COPY nexus-core ./nexus-core
COPY nexus-server ./nexus-server
COPY nexus-protocol ./nexus-protocol
COPY nexus-cli ./nexus-cli

# Build in release mode.
#
# Two BuildKit cache mounts cut rebuild time from ~4 min (observed
# during the memtest debugging session) to a fraction of that on warm
# caches:
#   - `/usr/local/cargo/registry` keeps the downloaded index + source
#     of every crate (`tantivy`, `hnsw_rs`, `heed`, ...) across builds
#     so `cargo fetch` doesn't re-download them every time.
#   - `/app/target` keeps the compiled artifacts — when only one
#     source file changed, rustc + cargo only recompile the
#     touched crates + their dependents, not the full 300-crate
#     workspace.
# The cache is build-local (not in the final image), so the runtime
# stage is still the same size and shape as before.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo +nightly build --release --workspace \
 && mkdir -p /out/release \
 && cp target/release/nexus-server /out/release/nexus-server

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 nexus && \
    mkdir -p /app/data /app/config /run/secrets && \
    chown -R nexus:nexus /app /run/secrets

# Copy binary from builder. The build stage staged the binary under
# `/out/release/` precisely because `/app/target/` is a cache mount
# that does not persist into the image — only paths *outside* the
# mount survive into subsequent stages.
COPY --from=builder /out/release/nexus-server /usr/local/bin/nexus-server
RUN chmod +x /usr/local/bin/nexus-server

# Set working directory
WORKDIR /app

# Switch to non-root user
USER nexus

# Expose default ports.
#   15474 — HTTP API (`/cypher`, `/knn_traverse`, `/health`, …).
#   15475 — Native binary RPC transport (`nexus://host:15475`), the
#           default for every first-party SDK since
#           `phase2_sdk-rpc-transport-default`. Operators who want
#           HTTP-only can leave 15475 unpublished on the host side
#           or set `[rpc].enabled = false` in `config.yml`.
EXPOSE 15474 15475

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:15474/health || exit 1

# Default environment variables
ENV NEXUS_ADDR=0.0.0.0:15474
ENV NEXUS_DATA_DIR=/app/data
ENV RUST_LOG=info

# Run server
CMD ["nexus-server"]

