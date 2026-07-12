## 1. Builder cross-compilation
- [ ] 1.1 Build stage → `FROM --platform=$BUILDPLATFORM rustlang/rust:nightly`; `ARG TARGETARCH` mapped to `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl`; both rustup targets added
- [ ] 1.2 aarch64-musl cross toolchain (vectorizer-style `tonistiigi/xx` or explicit cross-gcc) wired: `CC_aarch64_unknown_linux_musl`, `AR`, and cargo target linker env so LMDB/zstd/jemalloc C code cross-compiles
- [ ] 1.3 Per-arch static-linkage gate (`file | grep -E 'static'`) in the build stage

## 2. Multi-arch manifest
- [ ] 2.1 Dockerfile header: replace single-arch build/push instructions with `docker buildx build --platform linux/amd64,linux/arm64 ... --push`
- [ ] 2.2 Verify user-prep stage resolves under buildx for both target platforms (it only produces files; pin to BUILDPLATFORM if needed)

## 3. Validation gates
- [ ] 3.1 buildx builds BOTH platforms; both binaries statically linked
- [ ] 3.2 amd64 smoke battery green (health, param CREATE round-trip, restart persistence, --healthcheck exit 0)
- [ ] 3.3 arm64 smoke via `docker run --platform linux/arm64` (qemu): same battery; LMDB/mmap works under aarch64-musl; no SIMD crash (NEON or scalar fallback)
- [ ] 3.4 Docker Scout on both platform digests: 0C 0H 0M 0L, 0 packages

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation (DEPLOYMENT_GUIDE multi-arch pull note, README, CHANGELOG)
- [ ] 4.2 Write tests covering the new behavior (smoke scripts per arch recorded in the task; static-gate is the build-time test)
- [ ] 4.3 Run tests and confirm they pass
