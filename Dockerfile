# syntax=docker/dockerfile:1.6
# Multi-stage Dockerfile for Nexus Graph Database
#
# HOW TO BUILD:
#   docker build -t hivehub/nexus:2.1.0 -t hivehub/nexus:latest .
#
# HOW TO PUBLISH (Docker Hub — hivehub/nexus):
#   docker login
#   docker push hivehub/nexus:2.1.0
#   docker push hivehub/nexus:latest
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
#   #   Publish 15474 (HTTP API) and 15475 (native RPC transport, default
#   #   for first-party SDKs). Drop `-p 15475:15475` for HTTP-only
#   #   deployments and also set `[rpc].enabled = false` in config.yml.
#   docker run -d \
#     --name nexus \
#     -p 15474:15474 \
#     -p 15475:15475 \
#     -v nexus-data:/app/data \
#     -e NEXUS_ROOT_USERNAME=admin \
#     -e NEXUS_ROOT_PASSWORD=secure_password \
#     -e NEXUS_AUTH_ENABLED=true \
#     hivehub/nexus:2.1.0
#
#   # Using docker run with Docker secrets (recommended for production):
#   echo "secure_password" > secrets/root_password.txt
#   chmod 600 secrets/root_password.txt
#   docker run -d \
#     --name nexus \
#     -p 15474:15474 \
#     -p 15475:15475 \
#     -v nexus-data:/app/data \
#     -v $(pwd)/secrets/root_password.txt:/run/secrets/nexus_root_password:ro \
#     -e NEXUS_ROOT_USERNAME=admin \
#     -e NEXUS_ROOT_PASSWORD_FILE=/run/secrets/nexus_root_password \
#     -e NEXUS_AUTH_ENABLED=true \
#     -e NEXUS_DISABLE_ROOT_AFTER_SETUP=true \
#     hivehub/nexus:2.1.0
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
# workspace member" if any member directory is missing.
# All crates live under `crates/` per the workspace manifest.
COPY crates/nexus-core ./crates/nexus-core
COPY crates/nexus-server ./crates/nexus-server
COPY crates/nexus-protocol ./crates/nexus-protocol
COPY crates/nexus-cli ./crates/nexus-cli
COPY crates/nexus-bench ./crates/nexus-bench
COPY crates/nexus-knn-bench ./crates/nexus-knn-bench

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

# User-prep stage
#
# Run `useradd` in a throwaway `trixie-dev` stage so the `passwd`
# package + apt + dpkg never land in the final image. We then copy
# only the resulting `/etc/passwd`, `/etc/group`, `/etc/shadow`, and
# `/home/nexus` lines into the distroless runtime.
FROM dhi.io/debian-base:trixie-dev AS user-prep
RUN apt-get update && apt-get install -y --no-install-recommends passwd \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -m -u 1000 nexus \
 && mkdir -p /app/data /app/config /run/secrets \
 && chown -R nexus:nexus /app /run/secrets

# Runtime stage
#
# `dhi.io/debian-base:trixie` is the distroless DHI variant: same
# glibc 2.41 as the `rustlang/rust:nightly` builder, ships
# libssl3 / libcrypto3 / libz / libzstd / libgcc_s / ca-certificates
# / bash — the full runtime closure for `nexus-server` — but no
# apt, no dpkg-query, no curl, no shell utils, no compilers. Drops
# the package count from ~150 to ~25 and the Docker Scout grade
# from C to A on a freshly published image. DHI is the
# org-approved base; the Docker Hub `debian:trixie-slim` was not
# on the approved list.
FROM dhi.io/debian-base:trixie

# OCI image metadata. `org.opencontainers.image.version` is the
# canonical place container registries (Docker Hub, ghcr) read the
# version from; `docker inspect hivehub/nexus:2.1.0 --format
# '{{ index .Config.Labels "org.opencontainers.image.version" }}'`
# must match the tag.
LABEL org.opencontainers.image.title="Nexus" \
      org.opencontainers.image.description="High-performance property graph database with native vector search (KNN/HNSW)" \
      org.opencontainers.image.version="2.1.0" \
      org.opencontainers.image.vendor="HiveLLM" \
      org.opencontainers.image.source="https://github.com/hivellm/nexus" \
      org.opencontainers.image.documentation="https://github.com/hivellm/nexus/blob/main/README.md" \
      org.opencontainers.image.licenses="Apache-2.0"

# Provision the `nexus` user (uid 1000) by lifting only the
# user-database lines + home directory from the prep stage. No apt,
# no `passwd` package in the final image.
COPY --from=user-prep /etc/passwd /etc/passwd
COPY --from=user-prep /etc/group /etc/group
COPY --from=user-prep /etc/shadow /etc/shadow
COPY --from=user-prep --chown=1000:1000 /home/nexus /home/nexus
COPY --from=user-prep --chown=1000:1000 /app /app
COPY --from=user-prep --chown=1000:1000 /run/secrets /run/secrets

# Copy binary from builder. The build stage staged the binary under
# `/out/release/` precisely because `/app/target/` is a cache mount
# that does not persist into the image — only paths *outside* the
# mount survive into subsequent stages.
COPY --from=builder --chmod=0755 /out/release/nexus-server /usr/local/bin/nexus-server

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

# Health check.
#
# Distroless trixie ships bash but no curl / wget / grep. Probe via
# bash built-ins only: `/dev/tcp` for the socket, `read` for a single
# response line, `[[ ... == *200 OK* ]]` for the status assertion.
# Exits 0 only when the server replies `HTTP/1.x 200 OK` to /health.
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/15474 && printf 'GET /health HTTP/1.0\\r\\nHost: localhost\\r\\n\\r\\n' >&3 && read -r line <&3 && [[ \"$line\" == *'200 OK'* ]]"]

# Default environment variables
ENV NEXUS_ADDR=0.0.0.0:15474
ENV NEXUS_DATA_DIR=/app/data
ENV RUST_LOG=info

# Run server
CMD ["nexus-server"]

