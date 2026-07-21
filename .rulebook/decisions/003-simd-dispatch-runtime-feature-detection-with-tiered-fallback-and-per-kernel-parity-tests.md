# 3. SIMD dispatch: runtime feature detection with tiered fallback and per-kernel parity tests

**Status**: undefined
**Date**: 2026-04-18
**Related Tasks**: phase1_simd-knn-distance-kernels, phase2_simd-executor-filter-aggregate, phase3_simd-storage-checksum-parse

## Context

Nexus v1.0.0 rolls out SIMD across KNN distance, executor filter/aggregate, WAL CRC, record codec, and Cypher tokenizer. Without a single dispatch convention, every kernel would re-invent CPU feature detection, alignment, tail handling, and correctness testing, and the release binary would either (a) crash on older CPUs (compile-time target-feature) or (b) run scalar everywhere (no kernels selected). We need one architecture for all ~15 SIMD call sites. Project direction: SIMD is integrated into nexus-core with runtime validation, not an optional feature — whenever the CPU supports it, SIMD is the default execution path. No compile-time opt-out.

## Decision

One `nexus-core::simd` module owns all dispatch and is **always compiled** into nexus-core. There is no Cargo feature gate for SIMD — the use is standard whenever the runtime CPU supports it.

- Runtime detection via `std::is_x86_feature_detected!` / `is_aarch64_feature_detected!` executed once, cached in `OnceLock<CpuFeatures>`. No per-call detection overhead.
- Kernel selection cascade: AVX-512F -> AVX2+FMA -> SSE4.2 -> NEON -> Scalar. First match wins. Runs every time the binary starts; no rebuild required to pick up a faster CPU.
- **No Cargo features** for SIMD. All architecture-specific kernels are compiled unconditionally behind `#[cfg(target_arch = "...")]` gates so one build artifact runs optimally on any supported CPU. AVX-512 intrinsics (`_mm512_*`) are stable in `std::arch::x86_64` and are compiled for every x86_64 build; the `#[target_feature(enable = "avx512f")]` attribute is the safety gate, `is_x86_feature_detected!("avx512f")` is the runtime gate.
- Baseline guaranteed architectures: x86_64 = SSE2 (every x86_64 CPU), AArch64 = NEON (every ARMv8 CPU). Everything above is detected at runtime.
- Every SIMD kernel has a scalar reference kernel alongside it, marked `#[inline(always)]`. The scalar kernel is the ground truth.
- Correctness: proptest asserts SIMD-vs-scalar parity. Tolerance: `1e-5` absolute for f32, `1e-9 * max(abs(x), 1)` for f64 (FMA reorderings), bit-exact for integer and bitmap ops.
- Every `unsafe fn` and `unsafe {}` block carries a `// SAFETY:` comment referencing the `target_feature` gate.
- Tail handling convention: AVX-512 uses native masked load (`_mm512_maskz_loadu_*`); AVX2/SSE/NEON fall back to scalar for the last `n % LANES` elements.
- Escape hatch: `NEXUS_SIMD_DISABLE=1` env var forces scalar dispatch globally for emergency runtime rollback (no rebuild required).
- Observability: `GET /stats` exposes `simd.<op>_kernel` showing which implementation was selected at startup; logged once at INFO level on first use.
- Benches (Criterion) run per-kernel and per-dim; CI fails if SIMD regresses >5% vs previous baseline on the reference machine.
- Portable SIMD (`std::simd`) is **not used** in v1.0.0 — hand-written intrinsic kernels cover every target architecture already. Revisit if a future maintenance cost justifies it.

This convention applies to phase1 (distance), phase2 (compare/reduce), phase3 (crc/codec/rle/parse) kernels uniformly.

## Consequences

_No consequences documented._
