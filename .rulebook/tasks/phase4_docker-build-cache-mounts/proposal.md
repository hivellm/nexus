# Proposal: phase4_docker-build-cache-mounts

## Why

`Dockerfile` and `Dockerfile.memtest` each run `cargo +nightly build
--release --workspace` from scratch every build. No BuildKit cache mount,
no `cargo-chef` split between dependency compile and source compile. On
this repo (300+ deps, full HNSW / Tantivy / LMDB stack) a clean rebuild
takes ~4 minutes even when only `nexus-server/src/main.rs` changed —
observed during the memtest debugging session.

This is a developer and CI time tax that compounds every time someone
has to retest a fix in container.

## What Changes

- Enable `# syntax=docker/dockerfile:1.6` headers so `RUN --mount=type=cache`
  is available.
- Add:
  ```
  RUN --mount=type=cache,target=/usr/local/cargo/registry \
      --mount=type=cache,target=/app/target \
      cargo +nightly build --release ...
  ```
- Optionally introduce a `cargo-chef` stage to cache dependency builds
  independently of source changes.
- Apply both patterns to `Dockerfile` and `Dockerfile.memtest`.

## Impact

- Affected specs: none
- Affected code: `Dockerfile`, `scripts/memtest/Dockerfile.memtest`,
  `docker-compose*.yml` only if BuildKit needs explicit opt-in there
- Breaking change: NO (build output identical)
- User benefit: rebuild time drops from ~4 min to under 1 min on warm
  caches; CI pipeline becomes noticeably cheaper
