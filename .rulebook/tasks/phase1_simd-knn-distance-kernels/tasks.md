## 1. Infrastructure — simd module scaffolding
- [ ] 1.1 Create `nexus-core/src/simd/mod.rs` with `pub mod dispatch; pub mod scalar; pub mod distance; pub mod bitmap;`
- [ ] 1.2 Add `cfg_if!` gates for `x86.rs` (cfg x86_64), `aarch64.rs` (cfg aarch64), `portable.rs` (cfg feature simd-portable)
- [ ] 1.3 Register `pub mod simd;` from `nexus-core/src/lib.rs`
- [ ] 1.4 Add Cargo features: `simd` (default on), `simd-avx512`, `simd-portable`
- [ ] 1.5 Add dependencies: `bytemuck = "1"`, `aligned-vec = "0.6"` to `nexus-core/Cargo.toml`
- [ ] 1.6 Add dev-dependency: `proptest = "1"`, `approx = "0.5"`

## 2. Runtime feature detection
- [ ] 2.1 Define `CpuFeatures { avx512f, avx512vpopcntdq, avx2, sse42, neon, sve2 }` in `simd/dispatch.rs`
- [ ] 2.2 Implement `CpuFeatures::detect()` using `is_x86_feature_detected!` / `is_aarch64_feature_detected!`
- [ ] 2.3 Expose `pub fn cpu() -> &'static CpuFeatures` via `OnceLock`
- [ ] 2.4 Log detected features at tracing::info! on first call (one-shot)
- [ ] 2.5 Unit tests: on x86_64 host, `cpu().avx2` matches `cpuid` probe; on unsupported arches, all flags are false

## 3. Scalar reference kernels (ground truth)
- [ ] 3.1 Implement `scalar::dot(a, b)` with `iter().zip().map().sum()` and debug-assert equal-length slices
- [ ] 3.2 Implement `scalar::l2_sq(a, b)` (squared L2, no sqrt)
- [ ] 3.3 Implement `scalar::cosine(a, b)` on top of `dot` + `l2_norm` reuse
- [ ] 3.4 Implement `scalar::normalize(v)` in-place (returns norm for caller bookkeeping)
- [ ] 3.5 Make scalar kernels `#[inline(always)]` with `debug_assert_eq!(a.len(), b.len())` only

## 4. x86 SSE4.2 kernels (4 lanes f32)
- [ ] 4.1 Create `simd/x86/sse.rs` with `#[target_feature(enable = "sse4.2")]`
- [ ] 4.2 `dot_sse42(a, b)` using `_mm_mul_ps` + `_mm_add_ps` + horizontal reduce via `_mm_hadd_ps`
- [ ] 4.3 `l2_sq_sse42(a, b)` using `_mm_sub_ps` + `_mm_mul_ps` + reduce
- [ ] 4.4 `normalize_sse42(v)` using sum-of-squares + scalar 1/sqrt at the end
- [ ] 4.5 Tail: last `len % 4` elements processed scalar
- [ ] 4.6 proptest: SSE vs scalar identical within `1e-5`

## 5. x86 AVX2 kernels (8 lanes f32)
- [ ] 5.1 Create `simd/x86/avx2.rs` with `#[target_feature(enable = "avx2,fma")]`
- [ ] 5.2 `dot_avx2` using `_mm256_fmadd_ps` in 4 accumulators (ILP), then horizontal reduce
- [ ] 5.3 `l2_sq_avx2` using FMA(sub, sub, acc)
- [ ] 5.4 `normalize_avx2` with `_mm256_rsqrt_ps` + one Newton-Raphson iteration for 1-ULP accuracy
- [ ] 5.5 Tail via `_mm_maskmov_ps` or scalar fallback
- [ ] 5.6 proptest: AVX2 vs scalar identical within `1e-5`

## 6. x86 AVX-512 kernels (16 lanes f32)
- [ ] 6.1 Create `simd/x86/avx512.rs` behind `#[cfg(feature = "simd-avx512")]` and `#[target_feature(enable = "avx512f,avx512vl")]`
- [ ] 6.2 `dot_avx512` with `_mm512_fmadd_ps` + `_mm512_reduce_add_ps`
- [ ] 6.3 `l2_sq_avx512`, `normalize_avx512`
- [ ] 6.4 Tail via `_mm512_maskz_loadu_ps` (native masked load)
- [ ] 6.5 proptest: AVX-512 vs scalar identical within `1e-5`

## 7. AArch64 NEON kernels (4 lanes f32)
- [ ] 7.1 Create `simd/aarch64/neon.rs` with `#[target_feature(enable = "neon")]`
- [ ] 7.2 `dot_neon` using `vfmaq_f32` + `vaddvq_f32` (horizontal add)
- [ ] 7.3 `l2_sq_neon` using `vsubq_f32` + `vfmaq_f32`
- [ ] 7.4 `normalize_neon` with `vrsqrteq_f32` + `vrsqrtsq_f32` Newton step
- [ ] 7.5 Tail scalar
- [ ] 7.6 proptest: NEON vs scalar identical within `1e-5` (test gated `#[cfg(target_arch = "aarch64")]`)

## 8. Portable SIMD path (nightly)
- [ ] 8.1 Create `simd/portable.rs` behind `#[cfg(feature = "simd-portable")]`
- [ ] 8.2 Implement `dot`, `l2_sq`, `normalize` using `std::simd::f32x16`
- [ ] 8.3 Benchmark vs hand-written AVX2 — when within 10%, keep portable as primary; otherwise keep hand-written as primary

## 9. Dispatch layer — distance kernel pointers
- [ ] 9.1 Define kernel fn types: `type DotFn = unsafe fn(&[f32], &[f32]) -> f32;` (same for l2_sq, cosine, normalize)
- [ ] 9.2 `fn pick_dot_kernel() -> DotFn` selects kernel in order: AVX-512 → AVX2 → SSE4.2 → NEON → Scalar
- [ ] 9.3 Expose safe wrappers: `pub fn dot(a: &[f32], b: &[f32]) -> f32` that asserts len and calls the dispatched kernel
- [ ] 9.4 Same for `l2_sq`, `cosine`, `normalize`
- [ ] 9.5 `OnceLock<DotFn>` caches the selection — unit test: first call selects, second call hits cache

## 10. Alignment and memory helpers
- [ ] 10.1 `simd::aligned::AlignedVec<f32>` wrapper around `aligned_vec::AVec<f32, 32>`
- [ ] 10.2 Convert HNSW embedding storage (`hnsw_rs` wrapper) to use `AlignedVec` — confirms 32-byte alignment for AVX2/AVX-512 loads
- [ ] 10.3 Add `simd::aligned::as_aligned(&[f32]) -> Option<&[f32]>` helper that returns None when slice isn't aligned (rare fallback)

## 11. Bitmap popcount kernels (node-cosine input)
- [ ] 11.1 `simd::bitmap::popcount(words: &[u64]) -> u64` — scalar uses `count_ones`
- [ ] 11.2 x86 AVX-512 VPOPCNTQ variant
- [ ] 11.3 x86 AVX2 variant using `_mm256_shuffle_epi8` nibble-LUT
- [ ] 11.4 NEON variant using `vcntq_u8` + `vaddvq_u8`
- [ ] 11.5 `and_popcount(a: &[u64], b: &[u64])` — used for `|A ∩ B|` in Jaccard/cosine
- [ ] 11.6 proptest: all SIMD variants match scalar exactly (integer ops, no tolerance)

## 12. Replace call sites — KNN index
- [ ] 12.1 `nexus-core/src/index/mod.rs`: the inline `cosine_similarity` test helper and `normalize_vector` method call `simd::distance::*`
- [ ] 12.2 Provide `DistSimdCosine` / `DistSimdL2` structs implementing `hnsw_rs::dist::Distance<f32>` trait, using our kernels
- [ ] 12.3 `KnnIndex::new` picks our `DistSimdCosine` when `CosineMetric`, `DistSimdL2` when `EuclideanMetric`
- [ ] 12.4 Existing KNN tests stay green; numerical drift tolerance `1e-5`

## 13. Replace call sites — graph traversal
- [ ] 13.1 `graph/algorithms/traversal.rs:1607` `cosine_similarity(node1, node2)` refactored to build neighbour bitsets (`Vec<u64>` words) and call `simd::bitmap::and_popcount`
- [ ] 13.2 Same refactor for `jaccard_similarity`
- [ ] 13.3 Graph-level tests keep passing; add 3 new tests at 10k neighbour scale to exercise SIMD path

## 14. Documentation — simd-dispatch spec
- [ ] 14.1 Create `docs/specs/simd-dispatch.md` with the CpuFeatures table and "add a new kernel" checklist
- [ ] 14.2 Document the tail-handling convention (masked load on AVX-512, scalar otherwise)
- [ ] 14.3 Document correctness tolerance (`1e-5` abs for f32 kernels)
- [ ] 14.4 Cross-link from `docs/PERFORMANCE.md`

## 15. Benchmarks — Criterion
- [ ] 15.1 Create `nexus-core/benches/simd_distance.rs` with bench groups per op (dot/l2/cos/normalize)
- [ ] 15.2 Dim sweep: 32, 128, 256, 512, 768, 1024, 1536
- [ ] 15.3 Batch sweep: 1, 10, 100, 1000 (for batched-distance ops)
- [ ] 15.4 Report AVX-512 vs AVX2 vs SSE4.2 vs scalar ratios in `target/criterion/report/`
- [ ] 15.5 Check acceptance targets (documented in proposal §Impact): >=4x AVX2 vs scalar at dim=768
- [ ] 15.6 Add `nexus-core/benches/simd_popcount.rs` for bitmap kernels
- [ ] 15.7 Add Makefile target `make bench-simd` that runs both benches and prints the summary table

## 16. Integration benchmarks (end-to-end KNN)
- [ ] 16.1 Boot server with RPC disabled (HTTP only) and run 10k KNN queries via `knn.search` — measure p50/p95/p99
- [ ] 16.2 Same benchmark with SIMD disabled (env `NEXUS_SIMD_DISABLE=1` forces scalar dispatch)
- [ ] 16.3 Report end-to-end speedup ratio (target: >=2x end-to-end, since HTTP/JSON overhead dilutes the kernel speedup)
- [ ] 16.4 Neo4j compat suite (300 tests) still 300/300 passing

## 17. Cargo + lint + coverage
- [ ] 17.1 `cargo +nightly fmt --all` clean
- [ ] 17.2 `cargo clippy --workspace --all-features -- -D warnings` clean
- [ ] 17.3 Every `unsafe fn` and `unsafe {}` block has a `// SAFETY:` comment referencing the feature gate
- [ ] 17.4 `cargo llvm-cov --package nexus-core --ignore-filename-regex 'examples'` >= 95% coverage on new files
- [ ] 17.5 Cross-compile check: `cargo check --target aarch64-unknown-linux-gnu` succeeds

## 18. Rollout safety net
- [ ] 18.1 Add `NEXUS_SIMD_DISABLE` env var: when set, dispatch returns scalar unconditionally (emergency rollback)
- [ ] 18.2 Log the selected kernel per op at startup so ops can confirm from logs
- [ ] 18.3 Add `GET /stats` entries: `simd.dot_kernel`, `simd.cosine_kernel`, etc.

## 19. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 19.1 Update or create documentation covering the implementation (`docs/specs/simd-dispatch.md` + update `docs/PERFORMANCE.md` with the bench results)
- [ ] 19.2 Write tests covering the new behavior (proptest parity for every kernel, unit tests for dispatch, integration tests for KNN path; >= 50 tests total)
- [ ] 19.3 Run tests and confirm they pass (`cargo test --workspace --all-features --verbose`)
