# syntax=docker/dockerfile:1.6
# Multi-stage Dockerfile for Nexus Graph Database — zero-CVE edition.
#
# The runtime image is `FROM scratch` carrying ONLY a fully static
# (x86_64-unknown-linux-musl) nexus-server binary + user database +
# CA bundle. Zero OS packages → zero CVEs by construction (the previous
# DHI/debian runtime carried 14 disputed/won't-fix lows across glibc,
# systemd, coreutils and openssl that no `apt upgrade` can remove).
# Trade-off: no shell in the image — `docker exec ... sh` does not work;
# debug via `docker logs` and the HTTP API. The container HEALTHCHECK
# uses the binary itself (`nexus-server --healthcheck`).
#
# HOW TO BUILD:
#   docker build -t hivehub/nexus:2.5.0 -t hivehub/nexus:latest .
#
# HOW TO PUBLISH (Docker Hub — hivehub/nexus):
#   docker login
#   docker push hivehub/nexus:2.5.0
#   docker push hivehub/nexus:latest
#
# HOW TO RUN:
#   docker run -d \
#     --name nexus \
#     -p 15474:15474 \
#     -p 15475:15475 \
#     -v nexus-data:/app/data \
#     -e NEXUS_ROOT_USERNAME=admin \
#     -e NEXUS_ROOT_PASSWORD=secure_password \
#     -e NEXUS_AUTH_ENABLED=true \
#     hivehub/nexus:2.5.0
#
#   # Using docker-compose (recommended):
#   docker-compose up -d
#
# HOW TO VERIFY:
#   curl http://localhost:15474/health
#   docker logs nexus
#
# For more details, see docs/guides/DEPLOYMENT_GUIDE.md

# Build stage — static musl binary.
#
# `rustlang/rust:nightly` is Debian-based; `musl-tools` provides
# musl-gcc, which `cc`-built C deps (LMDB via heed, zstd via tantivy,
# jemalloc via tikv-jemalloc-sys) compile against. Rust targets
# x86_64-unknown-linux-musl with crt-static by default, producing a
# fully static PIE with no interpreter — runnable in `scratch`.
FROM rustlang/rust:nightly AS builder

RUN apt-get update && apt-get install -y \
    musl-tools \
    file \
    && rm -rf /var/lib/apt/lists/* \
 && rustup target add x86_64-unknown-linux-musl

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./

# Copy source for every workspace member declared in the root Cargo.toml.
# `cargo build` fails with "failed to load manifest for workspace member"
# if any member directory is missing, even when building a single package.
COPY crates/nexus-core ./crates/nexus-core
COPY crates/nexus-server ./crates/nexus-server
COPY crates/nexus-protocol ./crates/nexus-protocol
COPY crates/nexus-cli ./crates/nexus-cli
COPY crates/nexus-bench ./crates/nexus-bench
COPY crates/nexus-knn-bench ./crates/nexus-knn-bench

# Build ONLY nexus-server (the runtime ships a single binary) in release
# mode for the musl target. BuildKit cache mounts keep the registry and
# target dir warm across rebuilds. The binary is staged outside the
# cache mount (only paths outside the mount survive into later stages),
# and the build fails fast if the result is not statically linked —
# a dynamic binary would be unrunnable in `scratch`.
# NOTE: no `+nightly` here — the image's default toolchain IS nightly,
# but as a *dated* toolchain (e.g. nightly-2026-07-11). `cargo +nightly`
# would resolve to the undated channel, triggering rustup to download a
# fresh toolchain WITHOUT the musl target added above (build fails with
# "can't find crate for `core`").
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --package nexus-server \
      --target x86_64-unknown-linux-musl \
 && mkdir -p /out/release \
 && cp target/x86_64-unknown-linux-musl/release/nexus-server /out/release/nexus-server \
 && file /out/release/nexus-server | grep -Eq 'static-pie linked|statically linked' \
 && ! ldd /out/release/nexus-server 2>/dev/null

# User-prep stage
#
# Run `useradd` in a throwaway `trixie-dev` stage so the `passwd`
# package + apt + dpkg never land in the final image. We then copy
# only the resulting `/etc/passwd`, `/etc/group`, and the prepared
# directory skeleton (with ownership) into the scratch runtime —
# `USER nexus` and writable /app/data need them.
FROM dhi.io/debian-base:trixie-dev AS user-prep
RUN apt-get update && apt-get install -y --no-install-recommends passwd \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -m -u 1000 nexus \
 && mkdir -p /app/data /app/config /run/secrets /tmp-skel \
 && chown -R nexus:nexus /app /run/secrets /tmp-skel \
 && chmod 1777 /tmp-skel

# Runtime stage — scratch: zero OS packages, zero CVEs.
FROM scratch

# OCI image metadata. `org.opencontainers.image.version` is the
# canonical place container registries read the version from and must
# match the pushed tag.
LABEL org.opencontainers.image.title="Nexus" \
      org.opencontainers.image.description="High-performance property graph database with native vector search (KNN/HNSW)" \
      org.opencontainers.image.version="2.5.0" \
      org.opencontainers.image.vendor="HiveLLM" \
      org.opencontainers.image.source="https://github.com/hivellm/nexus" \
      org.opencontainers.image.documentation="https://github.com/hivellm/nexus/blob/main/README.md" \
      org.opencontainers.image.licenses="Apache-2.0"

# User database (so `USER nexus` resolves) + directory skeleton with
# ownership. scratch has no mkdir/chown — everything arrives via COPY.
COPY --from=user-prep /etc/passwd /etc/passwd
COPY --from=user-prep /etc/group /etc/group
COPY --from=user-prep --chown=1000:1000 /home/nexus /home/nexus
COPY --from=user-prep --chown=1000:1000 /app /app
COPY --from=user-prep --chown=1000:1000 /run/secrets /run/secrets
COPY --from=user-prep --chown=1000:1000 /tmp-skel /tmp

# CA bundle for outbound TLS (rustls reads the system bundle via
# rustls-native-certs). Sourced from the builder's Debian ca-certificates.
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# The static binary — the only executable in the image.
COPY --from=builder --chmod=0755 /out/release/nexus-server /usr/local/bin/nexus-server

WORKDIR /app

USER nexus

# Expose default ports.
#   15474 — HTTP API (`/cypher`, `/knn_traverse`, `/health`, …).
#   15475 — Native binary RPC transport (`nexus://host:15475`), the
#           default for every first-party SDK. Operators who want
#           HTTP-only can leave 15475 unpublished on the host side
#           or set `[rpc].enabled = false` in `config.yml`.
EXPOSE 15474 15475

# Health check via the binary itself (`--healthcheck` performs an
# HTTP/1.0 GET to 127.0.0.1:<port from NEXUS_ADDR>/health and exits
# 0/1). No shell exists in this image, so exec-form with the absolute
# path is mandatory.
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD ["/usr/local/bin/nexus-server", "--healthcheck"]

# Default environment variables
ENV NEXUS_ADDR=0.0.0.0:15474
ENV NEXUS_DATA_DIR=/app/data
ENV RUST_LOG=info
ENV TZ=UTC

# Run server
CMD ["/usr/local/bin/nexus-server"]
