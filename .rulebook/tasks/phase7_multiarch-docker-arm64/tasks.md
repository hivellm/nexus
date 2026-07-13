## 1. Builder cross-compilation
- [x] 1.1 Implemented with the SYNAP pattern instead of cross-compilation: builder stays `rustlang/rust:nightly` (multi-arch image) and each platform builds NATIVELY (arm64 under qemu/binfmt); `ARG TARGETARCH` case-maps to `x86_64-unknown-linux-musl` / `aarch64-unknown-linux-musl` in both the rustup-target layer and the build layer
- [x] 1.2 No cross toolchain needed — `musl-tools` provides the HOST-arch musl-gcc on both platforms, so LMDB/zstd/jemalloc C code compiles natively per leg (simpler than the vectorizer xx approach, which targets glibc; jemalloc's musl `disable_initial_exec_tls` feature applies to aarch64-musl automatically via `cfg(target_env = "musl")`)
- [x] 1.3 Per-arch static-linkage gate (`file | grep -E 'static-pie linked|statically linked'`) runs in the build stage on both legs — both passed

## 2. Multi-arch manifest
- [x] 2.1 Dockerfile header documents `docker buildx build --platform linux/amd64,linux/arm64 ... --push` (single-arch instructions kept for local builds)
- [x] 2.2 user-prep stage pinned to `--platform=$BUILDPLATFORM` (its output is arch-neutral text files + empty dirs; no qemu run, no dependency on a DHI arm64 variant)

## 3. Validation gates
- [x] 3.1 Both platforms built: amd64 25.7MB, arm64 21.6MB; both binaries statically linked (build gate)
- [x] 3.2 amd64 smoke battery green (health 2s, param MERGE-rel round-trip w=7 in correct column order, restart persistence, --healthcheck exit 0) — validated before the earlier amd64 push
- [ ] 3.3 arm64 runtime smoke BLOCKED on this machine: qemu-user does not implement the `get_robust_list` syscall (verified via QEMU_STRACE: `get_robust_list(...) = -1 errno=38`), which LMDB's robust-mutex env-open requires — the binary exits with "Database error: Function not implemented (os error 38)" under emulation. This is a qemu-user limitation, NOT an arm64 defect: the syscall exists in every real arm64 kernel (Graviton, Apple Silicon VM). RELEASE GATE: run the smoke battery on real arm64 hardware (or Docker Desktop on Apple Silicon) before tagging the final 2.5.0
- [x] 3.4 Docker Scout on both platform digests: amd64 0C 0H 0M 0L / 0 packages; arm64 0C 0H 0M 0L / 0 packages. Multi-arch manifest `hivehub/nexus:2.5.0-dev` (list digest sha256:8f30300c...) pushed serving linux/amd64 (sha256:630ca740...) + linux/arm64 (sha256:a941d7a6...)

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering the implementation — Dockerfile header (buildx multi-arch instructions + native-per-platform rationale), DEPLOYMENT_GUIDE multi-arch note, CHANGELOG entry
- [x] 4.2 Write tests covering the new behavior — per-arch static-linkage build gate (fails the build on a dynamic binary); amd64 smoke battery; arm64 smoke procedure documented (blocked by qemu-user, see 3.3)
- [ ] 4.3 Run tests and confirm they pass — amd64 complete; arm64 runtime pending real-arm64 hardware (release gate, see 3.3)
