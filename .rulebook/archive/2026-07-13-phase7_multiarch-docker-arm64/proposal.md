# Proposal: phase7_multiarch-docker-arm64

## Why

`hivehub/nexus` on Docker Hub ships **linux/amd64 only**. Every Apple-Silicon
Mac, AWS Graviton, Ampere, and Raspberry-class deployment either fails to pull
or silently runs under qemu emulation (several times slower — fatal for a
performance-positioned database). The sibling project **Vectorizer already
publishes multi-arch** (`docker buildx build --platform linux/amd64,linux/arm64`
with the `tonistiigi/xx` cross-compilation helper and
`FROM --platform=$BUILDPLATFORM` build stages — see
`e:\HiveLLM\Vectorizer\Dockerfile` lines 10-13, 131-150); Nexus should match.

Nexus is well-positioned for arm64: the SIMD module already ships NEON
kernels with runtime dispatch, and the 2.5.0 zero-CVE image is a fully
static musl binary in `FROM scratch` — `scratch` is inherently multi-arch,
so only the build stage needs cross-compilation.

## What Changes

Extend the Dockerfile to build both architectures and publish a multi-arch
manifest:

- Build stage becomes `FROM --platform=$BUILDPLATFORM rustlang/rust:nightly`
  (always compiles on the native builder — no qemu for rustc) with
  `ARG TARGETARCH` mapping: `amd64 → x86_64-unknown-linux-musl`,
  `arm64 → aarch64-unknown-linux-musl`.
- aarch64-musl cross toolchain in the builder (vectorizer-style
  `tonistiigi/xx`, or explicit `gcc-aarch64-linux-gnu` + musl cross) so the
  C dependencies (LMDB via heed, zstd via tantivy, jemalloc via
  tikv-jemalloc-sys) cross-compile; `CC`/`AR`/cargo `[target.*]` linker env
  wired per target.
- Static-linkage build gate (`file | grep`) runs per-arch.
- Runtime stays `FROM scratch` (0 packages, 0 CVEs on BOTH arches);
  user-prep stage (`dhi.io/debian-base:trixie-dev`) must resolve for the
  build platform only (it produces plain files).
- Publish: `docker buildx build --platform linux/amd64,linux/arm64 -t
  hivehub/nexus:<ver> -t hivehub/nexus:latest --push .` documented in the
  Dockerfile header (replacing the single-arch `docker build`/`docker push`
  instructions).

## Impact

- Affected specs: specs/docker-multiarch/spec.md (this task)
- Affected code: `Dockerfile`; docs (DEPLOYMENT_GUIDE, README badges/pull
  instructions)
- Breaking change: NO (amd64 digest continues to exist under the manifest)
- User benefit: native pulls on Apple Silicon / Graviton / Ampere with NEON
  SIMD instead of emulation-or-nothing; parity with the Vectorizer release
  process.

## Verification gates

- `docker buildx build --platform linux/amd64,linux/arm64` completes; both
  binaries pass the static gate.
- amd64 image: existing smoke battery green.
- arm64 image: `docker run --platform linux/arm64` (qemu) smoke: /health up,
  parameterized CREATE round-trip, restart persistence (LMDB/mmap under
  aarch64-musl), `--healthcheck` exit 0, SIMD tier logs NEON (or scalar
  fallback under qemu) without crashing.
- Docker Scout on both platform digests: 0C 0H 0M 0L, 0 packages.
