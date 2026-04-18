## 1. Infrastructure — simd module scaffolding
- [x] 1.1 Create `nexus-core/src/simd/mod.rs` with `pub mod dispatch; pub mod scalar; pub mod distance; pub mod bitmap;` (also added: reduce, compare, crc32c, json, rle, x86, aarch64 over subsequent phases)
- [x] 1.2 Architecture-specific gates via `#[cfg(target_arch = "x86_64")]` and `#[cfg(target_arch = "aarch64")]` on the x86 / aarch64 modules (simpler than `cfg_if!`; same outcome)
- [x] 1.3 Register `pub mod simd;` from `nexus-core/src/lib.rs`
- [ ] 1.4 ~~Cargo features `simd`, `simd-avx512`, `simd-portable`~~ — superseded by ADR-003 decision: SIMD is always compiled (no feature flag); `NEXUS_SIMD_DISABLE` env var provides runtime rollback instead
- [x] 1.5 Dependency audit: `bytemuck` already present via workspace; `aligned-vec` not needed (proptest confirmed `_loadu_*` paths are correct for unaligned slices, which is what production uses)
- [x] 1.6 Dev-dependencies: `proptest = "1.4"`, `approx = "0.5"`

## 2. Runtime feature detection
- [x] 2.1 Define `CpuFeatures { avx512f, avx512_vpopcntdq, avx2, sse42, neon, sve2, disabled }` in `simd/dispatch.rs`
- [x] 2.2 `CpuFeatures::detect()` via `std::is_x86_feature_detected!` / `std::arch::is_aarch64_feature_detected!`
- [x] 2.3 `pub fn cpu() -> &'static CpuFeatures` via `OnceLock`
- [x] 2.4 `tracing::info!` on first call with all flag values + `preferred_tier()`
- [x] 2.5 Unit tests: `detect_returns_consistent_features_within_a_process`, `preferred_tier_matches_highest_flag`, `neon_is_available_on_aarch64_builds` (gated)

## 3. Scalar reference kernels (ground truth)
- [x] 3.1 `scalar::dot_f32` with `iter().zip().map().sum()` + `debug_assert_eq!` on lengths
- [x] 3.2 `scalar::l2_sq_f32` (squared L2)
- [x] 3.3 `scalar::cosine_f32` via dot + norms (returns 0.0 for zero-norm, matches KnnIndex helper)
- [x] 3.4 `scalar::normalize_f32` in-place, returns pre-normalisation norm
- [x] 3.5 All marked `#[inline(always)]`

## 4. x86 SSE4.2 kernels (4 lanes f32)
- [x] 4.1 `#[target_feature(enable = "sse4.2")]` in `simd/x86.rs` (single-file layout chosen over the `simd/x86/sse.rs` tree)
- [x] 4.2 `dot_f32_sse42` via `_mm_mul_ps` + `_mm_add_ps` + `_mm_hadd_ps` reduction
- [x] 4.3 `l2_sq_f32_sse42` via `_mm_sub_ps` + `_mm_mul_ps` + reduce
- [x] 4.4 `normalize_f32_sse42` via sum-of-squares + scalar `sqrt`
- [x] 4.5 Scalar tail for last `len % 4` elements
- [x] 4.6 proptest SIMD-vs-scalar parity within 5e-4 tolerance (`tests/simd_distance_parity.rs::x86_parity::sse42_*`)

## 5. x86 AVX2 kernels (8 lanes f32)
- [x] 5.1 `#[target_feature(enable = "avx2,fma")]` in `simd/x86.rs`
- [x] 5.2 `dot_f32_avx2` with 4-way ILP accumulators + `_mm256_fmadd_ps` + horizontal reduce
- [x] 5.3 `l2_sq_f32_avx2` via FMA(d, d, acc) with sub
- [x] 5.4 `normalize_f32_avx2` via dot-product sum-of-squares + scalar 1/sqrt
- [x] 5.5 Scalar tail (less complex than `_mm_maskmov_ps` and equivalent perf for small tails)
- [x] 5.6 proptest: `avx2_dot_matches_scalar`, `avx2_l2_sq_matches_scalar`, `avx2_cosine_matches_scalar`, `avx2_normalize_matches_scalar`

## 6. x86 AVX-512 kernels (16 lanes f32)
- [x] 6.1 `#[target_feature(enable = "avx512f")]` in `simd/x86.rs` — always compiled (no `simd-avx512` feature flag per ADR-003)
- [x] 6.2 `dot_f32_avx512` via `_mm512_fmadd_ps` + `_mm512_reduce_add_ps`
- [x] 6.3 `l2_sq_f32_avx512`, `normalize_f32_avx512`
- [x] 6.4 Masked tail via `_mm512_maskz_loadu_ps` / `_mm512_mask_storeu_ps`
- [x] 6.5 proptest parity on all four ops

## 7. AArch64 NEON kernels (4 lanes f32)
- [x] 7.1 `#[target_feature(enable = "neon")]` in `simd/aarch64.rs`
- [x] 7.2 `dot_f32_neon` via `vfmaq_f32` + `vaddvq_f32`
- [x] 7.3 `l2_sq_f32_neon` via `vsubq_f32` + `vfmaq_f32`
- [x] 7.4 `normalize_f32_neon` via sum-of-squares + scalar sqrt + `vmulq_f32`
- [x] 7.5 Scalar tail
- [ ] 7.6 proptest parity on aarch64 host — code is in place gated `#[cfg(target_arch = "aarch64")]`; validation blocked locally by missing aarch64 C cross-toolchain (cc-rs). Runs on first ARM CI job.

## 8. Portable SIMD path (nightly)
- [ ] 8.1 Dropped per ADR-003 decision: hand-written intrinsic kernels cover every target architecture we ship (x86_64 SSE4.2/AVX2/AVX-512, aarch64 NEON) and win decisively on measured hardware. `std::simd` adds nightly dependency without improving on the hand-written paths.

## 9. Dispatch layer — distance kernel pointers
- [x] 9.1 Kernel fn types `DotF32Fn` / `L2SqF32Fn` / `CosineF32Fn` / `NormalizeF32Fn`
- [x] 9.2 `pick_*_f32()` cascade AVX-512 → AVX2 → SSE4.2 → NEON → Scalar
- [x] 9.3 Safe wrappers `pub fn dot_f32(a, b)` with `assert_eq!` on lengths
- [x] 9.4 `l2_sq_f32`, `cosine_f32`, `normalize_f32` safe wrappers
- [x] 9.5 `OnceLock<Fn>` caching + `dispatch_handles_various_sizes` test covers corner cases (len 1/3/4/7/8/15/16/31/32/63/64/127/128/768/1024)

## 10. Alignment and memory helpers
- [ ] 10.1 Not needed: proptest parity confirmed correctness with `_loadu_*` (unaligned loads). HNSW storage uses contiguous `Vec<f32>` which is 4-byte aligned; 32-byte alignment would be a pure perf optimisation not a correctness requirement. Reopens if a bench shows load-alignment is a bottleneck.

## 11. Bitmap popcount kernels (node-cosine input)
- [x] 11.1 `scalar::popcount_u64` via `count_ones`
- [x] 11.2 AVX-512 VPOPCNTQ variant (`x86::popcount_u64_avx512` + masked tail)
- [x] 11.3 AVX2 Mula nibble-LUT via `_mm256_shuffle_epi8`
- [x] 11.4 NEON `vcntq_u8` + `vaddlvq_u8`
- [x] 11.5 `and_popcount_u64` (pointwise AND + popcount) — all four tiers
- [x] 11.6 Bit-exact proptest (`tests/simd_bitmap_parity.rs`), 5 cases × 256 inputs each

## 12. Replace call sites — KNN index
- [x] 12.1 `nexus-core/src/index/mod.rs::normalize_vector` delegates to `simd::distance::normalize_f32`
- [x] 12.2 `DistSimdCosine` + `DistSimdL2` structs implement `hnsw_rs::dist::Distance<f32>`
- [x] 12.3 `KnnIndex.hnsw` field type changed to `Hnsw<'static, f32, DistSimdCosine>`; all HNSW distance calls flow through SIMD
- [x] 12.4 101 index tests green after swap (tolerance observed: identical under `1e-5`)

## 13. Replace call sites — graph traversal
- [x] 13.1 `graph/algorithms/traversal.rs::cosine_similarity` refactored to `pack_neighbor_bitmaps` + `simd::bitmap::and_popcount_u64` + `simd::bitmap::popcount_u64`
- [x] 13.2 `jaccard_similarity` refactored via the same helper
- [x] 13.3 `test_cosine_similarity` + `test_jaccard_similarity` + `test_node_similarity_calculation` green; broader 2566-test suite green

## 14. Documentation — simd-dispatch spec
- [x] 14.1 `docs/specs/simd-dispatch.md` created with full CpuFeatures table, cascade rules, kernel tiers map, "add a new kernel" checklist
- [x] 14.2 Tail-handling convention documented (AVX-512 native masked load, else scalar tail)
- [x] 14.3 Correctness tolerances documented (5e-4 scaled for f32, 1e-9 scaled for f64, bit-exact for integer/bitmap)
- [ ] 14.4 `docs/PERFORMANCE.md` cross-link not written; numbers live in `docs/specs/simd-dispatch.md` directly — a cross-link is a follow-up once `PERFORMANCE.md` exists.

## 15. Benchmarks — Criterion
- [x] 15.1 `nexus-core/benches/simd_distance.rs` with groups dot/l2/cos/normalize
- [x] 15.2 Dim sweep 32, 128, 256, 512, 768, 1024, 1536
- [ ] 15.3 Batch sweep (1/10/100/1000) not added — single-vector dim sweep covers the production KNN call pattern; multi-vector batch would only matter for a future batched-distance API.
- [x] 15.4 Reports in `target/criterion/` (Criterion default)
- [x] 15.5 Acceptance target exceeded: measured 12.7× AVX-512 vs scalar at dim=768 (>=4× target)
- [x] 15.6 `nexus-core/benches/simd_popcount.rs` for bitmap kernels
- [ ] 15.7 Makefile target not added — Criterion is runnable directly via `cargo +nightly bench --bench simd_distance`; adding a Makefile is a separate ergonomics task.

## 16. Integration benchmarks (end-to-end KNN)
- [ ] 16.1 End-to-end KNN query via HTTP not measured in this phase — kernel benches already quantify the SIMD win in isolation. Follow-up once RPC transport lands and HTTP latency budget becomes the binding constraint.
- [ ] 16.2 — covered by 16.1
- [ ] 16.3 — covered by 16.1
- [x] 16.4 2566/2566 `nexus-core` tests green across all commits (Neo4j compat suite runs as part of the full workspace test pass the hooks enforce on every commit)

## 17. Cargo + lint + coverage
- [x] 17.1 `cargo +nightly fmt --all` clean on every commit (pre-commit hook enforces)
- [x] 17.2 `cargo +nightly clippy -p nexus-core --tests --benches -- -D warnings` clean (workspace `--all-features` not measured — no features to enable on SIMD anyway)
- [x] 17.3 Every `unsafe fn` / `unsafe {}` carries a `// SAFETY:` comment tying the call to the `cpu()` flag
- [ ] 17.4 `cargo llvm-cov` coverage report not collected; test counts speak to coverage breadth (50+ proptest cases per kernel family).
- [ ] 17.5 `cargo check --target aarch64-unknown-linux-gnu` blocked locally by missing aarch64 C cross-toolchain (tantivy / zstd build scripts need a cc). Used intrinsics (`vld1q_f32`/`vfmaq_f32`/`vaddvq_f32`/`vdupq_n_f32`/`vmulq_f32`/`vst1q_f32`/`vsubq_f32`/`vcntq_u8`/`vaddlvq_u8`/`vandq_u8`) are all stable in `std::arch::aarch64` and will be validated on first ARM CI.

## 18. Rollout safety net
- [x] 18.1 `NEXUS_SIMD_DISABLE=1` env var forces scalar across every dispatched op (distance + bitmap + reduce + compare + rle)
- [x] 18.2 Single `tracing::info!` line on first `cpu()` call reports the selected tier + all flag values
- [ ] 18.3 `/stats` endpoint extension not yet added; kernel tiers already exposed via `distance::kernel_tiers()`, `bitmap::kernel_tiers()`, `reduce::kernel_tiers()`, `compare::kernel_tiers()` — wiring to HTTP is a one-liner when `/stats` is next touched.

## 19. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 19.1 Update or create documentation covering the implementation (`docs/specs/simd-dispatch.md` with full spec + reference benchmark numbers; CHANGELOG entries per commit)
- [x] 19.2 Write tests covering the new behavior: proptest parity (`tests/simd_scalar_properties.rs` 6 cases, `tests/simd_distance_parity.rs` 16 cases, `tests/simd_bitmap_parity.rs` 5 cases — 256 proptest inputs each; plus 32 unit tests across the module)
- [x] 19.3 Run tests and confirm they pass: 2566 nexus-core tests green (0 failures) across all phase-1 SIMD commits
