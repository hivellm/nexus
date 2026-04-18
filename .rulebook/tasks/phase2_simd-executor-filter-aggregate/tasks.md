## 1. Column batch type
- [ ] 1.1 Define `ColumnData` enum (I64, F64, F32, Bool, Str, Null) in `execution/columnar.rs`
- [ ] 1.2 Define `Column { data, nulls: Option<BitVec>, len }` and constructor helpers `from_i64(AlignedVec<i64>)` etc
- [ ] 1.3 Add `as_slice::<T>()` typed accessors returning `&[T]` with panic on type mismatch (debug only)
- [ ] 1.4 Remove the old placeholder inline AVX2 `SimdComparator` from the same file — superseded by `simd::compare`
- [ ] 1.5 Unit tests: construct, read, and convert columns for each variant

## 2. BitVec with SIMD bit ops
- [ ] 2.1 Reuse `bitvec = "1"` crate from Cargo.toml (add if missing)
- [ ] 2.2 Add helpers `simd::bitmap::and_bitvec`, `or_bitvec`, `not_bitvec` that call our popcount kernels
- [ ] 2.3 Add `simd::bitmap::selectivity(&BitVec) -> f64` using popcount — used by the planner hint in §9
- [ ] 2.4 Integration test: AND/OR/NOT of 1M-bit BitVecs match scalar

## 3. Compare kernels — scalar reference
- [ ] 3.1 `simd::compare::scalar::eq_i64(col, scalar) -> BitVec`
- [ ] 3.2 Same for `ne`, `lt`, `le`, `gt`, `ge`, `between(lo, hi)`
- [ ] 3.3 f64 variants (with NaN semantics: `NaN` never equal, `NaN < x` false)
- [ ] 3.4 f32 variants
- [ ] 3.5 `eq_str(&[SmolStr], &str) -> BitVec` (scalar only; string SIMD is phase 3)

## 4. Compare kernels — x86 AVX2 (4 lanes i64, 4 lanes f64)
- [ ] 4.1 `eq_i64_avx2` using `_mm256_cmpeq_epi64` + `_mm256_movemask_pd` (reinterpret i64 → pd)
- [ ] 4.2 `lt_i64_avx2` using `_mm256_cmpgt_epi64` flipped
- [ ] 4.3 `eq_f64_avx2` using `_mm256_cmp_pd(_, _, _CMP_EQ_OQ)`
- [ ] 4.4 `lt/le/gt/ge_f64_avx2`
- [ ] 4.5 `between_f64_avx2` — two compares + AND
- [ ] 4.6 Tail: masked load on AVX-512 path; scalar on AVX2 path (len < 4)
- [ ] 4.7 proptest: AVX2 vs scalar bit-identical for BitVec output, matches scalar including NaN semantics

## 5. Compare kernels — x86 AVX-512 (8 lanes i64, 8 lanes f64)
- [ ] 5.1 `eq_i64_avx512` using `_mm512_cmpeq_epi64_mask` (native `__mmask8`)
- [ ] 5.2 Same for lt/le/gt/ge on i64 via `_mm512_cmp_epi64_mask`
- [ ] 5.3 f64 compares via `_mm512_cmp_pd_mask`
- [ ] 5.4 Tail via `_mm512_maskz_loadu_epi64` + `_mm512_mask_cmp_epi64_mask`
- [ ] 5.5 proptest: AVX-512 vs scalar bit-identical

## 6. Compare kernels — SSE4.2 and NEON
- [ ] 6.1 `eq_i64_sse42` using `_mm_cmpeq_epi64`
- [ ] 6.2 `lt_f64_sse42` using `_mm_cmplt_pd`
- [ ] 6.3 `eq_i64_neon` using `vceqq_s64` + `vshrn_n_u64` to pack bits
- [ ] 6.4 `lt_f64_neon` using `vcltq_f64`
- [ ] 6.5 proptest: every variant bit-identical to scalar

## 7. Reduce kernels — scalar reference
- [ ] 7.1 `sum_i64`, `sum_f64`, `sum_f32`
- [ ] 7.2 `min_i64`, `min_f64`, `min_f32` (with NaN → first non-NaN)
- [ ] 7.3 `max_i64`, `max_f64`, `max_f32`
- [ ] 7.4 `avg_f64` = sum / count_not_null
- [ ] 7.5 `count_not_null(nulls: &BitVec) -> u64`
- [ ] 7.6 `variance_f64` / `stddev_f64` via Welford accumulator

## 8. Reduce kernels — AVX2 / AVX-512 / NEON
- [ ] 8.1 AVX2 `sum_f64` with 4 FMA-ordered accumulators → horizontal add
- [ ] 8.2 AVX2 `min_f64` / `max_f64` shuffle-based reduction (`_mm256_min_pd` + permute)
- [ ] 8.3 AVX-512 `sum_f64` via `_mm512_reduce_add_pd`
- [ ] 8.4 AVX-512 `min_f64` / `max_f64` via `_mm512_reduce_min/max_pd`
- [ ] 8.5 NEON `sum_f64` via `vaddvq_f64` (pairwise fold across 4×f64)
- [ ] 8.6 NEON `min_f64` / `max_f64` via `vminvq_f64` / `vmaxvq_f64`
- [ ] 8.7 i64 variants for all three archs (`_mm256_add_epi64`, `vaddvq_s64`)
- [ ] 8.8 proptest: associativity-tolerant parity vs scalar within `1e-9 * max(abs(result), 1.0)` for f64

## 9. Executor wiring — Filter batch path
- [ ] 9.1 Add config `executor.columnar_threshold: usize = 128` to `nexus-core/src/executor/config.rs`
- [ ] 9.2 In `operators/filter.rs`, detect predicates of shape `column OP literal` or `column OP column` (binary expressions)
- [ ] 9.3 When row count >= threshold and all referenced columns are numeric, materialise into `Column` batches
- [ ] 9.4 Dispatch to `simd::compare::*` and produce a selection BitVec
- [ ] 9.5 Gather matching rows: convert BitVec → `Vec<u32>` selection vector; gather via indexed access
- [ ] 9.6 Unit tests: WHERE with 1K / 10K / 100K rows produce same result as scalar path
- [ ] 9.7 Unit tests: IS NULL / IS NOT NULL use null mask directly (no SIMD call)

## 10. Executor wiring — Aggregate SIMD path
- [ ] 10.1 In `operators/aggregate.rs`, route groupless `SUM(col) / MIN / MAX / AVG` over numeric columns to SIMD kernels
- [ ] 10.2 Group-by: partition rows by group hash, then per-group reduce via SIMD
- [ ] 10.3 `COUNT(*)` and `COUNT(col)` use `count_not_null` on the null mask
- [ ] 10.4 `STDDEV`, `VARIANCE` via SIMD Welford path (split accumulator per lane, combine at end)
- [ ] 10.5 `COLLECT` stays scalar (note: non-numeric, heterogeneous output)
- [ ] 10.6 Unit tests: every aggregate matches scalar result under tolerance `1e-9` for f64, exact for i64

## 11. Planner hint — PreferColumnar
- [ ] 11.1 Extend `LogicalPlan::Filter` with optional `hint: FilterHint { columnar_cols: Vec<ColumnRef> }`
- [ ] 11.2 Planner pass `add_columnar_hint`: when child cardinality estimate >= threshold and predicate is simple, populate hint
- [ ] 11.3 Planner test: query `MATCH (n:Person) WHERE n.age > 30 RETURN count(n)` at 10K scale produces a hinted Filter
- [ ] 11.4 Unit tests: hint-less queries (strings, lists) stay on scalar path

## 12. Label bitmap intersect benchmark
- [ ] 12.1 Confirm `roaring = { version = "0.10", features = ["simd"] }` in `nexus-core/Cargo.toml` (add feature if missing)
- [ ] 12.2 Bench: 1M rows, 1 label; 1M rows, 2 labels; 1M rows, 3 labels
- [ ] 12.3 When multi-label speedup vs stock roaring >= 1.5x: ship the feature flag and close the bullet
- [ ] 12.4 When below 1.5x: open a follow-up rulebook task `phase4_simd-label-intersect-custom` before archive

## 13. Documentation
- [ ] 13.1 Extend `docs/specs/simd-dispatch.md` with the new compare/reduce kernel tables
- [ ] 13.2 Create `docs/specs/executor-columnar.md` documenting the batch boundary, when the planner switches, and how `PreferColumnar` works
- [ ] 13.3 Cross-link from `docs/ARCHITECTURE.md` executor section

## 14. Benchmarks
- [ ] 14.1 `nexus-core/benches/simd_compare.rs` — i64/f64 eq/lt/gt at batch sizes 128/1K/10K/100K
- [ ] 14.2 `nexus-core/benches/simd_reduce.rs` — sum/min/max/avg at same batch sizes
- [ ] 14.3 `nexus-core/benches/executor_filter.rs` — end-to-end `RETURN count(n) WHERE n.age > 30` at 1M rows
- [ ] 14.4 `nexus-core/benches/executor_aggregate.rs` — `RETURN SUM(n.score)` at 1M rows
- [ ] 14.5 Acceptance: AVX2 >= 3x scalar for filter, >= 4x scalar for sum at 1M rows
- [ ] 14.6 End-to-end >= 1.5x on the same queries (HTTP overhead included)

## 15. Cargo + lint + coverage
- [ ] 15.1 `cargo +nightly fmt --all`
- [ ] 15.2 `cargo clippy --workspace --all-features -- -D warnings`
- [ ] 15.3 Every new `unsafe {}` has `// SAFETY:` comment
- [ ] 15.4 Coverage on new files >= 95% via `cargo llvm-cov`
- [ ] 15.5 300/300 Neo4j compat suite still green

## 16. Rollout safety
- [ ] 16.1 Extend `NEXUS_SIMD_DISABLE` env handling to force scalar in compare/reduce paths too
- [ ] 16.2 Extend `/stats` with `simd.eq_i64_kernel`, `simd.sum_f64_kernel`
- [ ] 16.3 Log selected kernels at startup (single info line per kernel family)

## 17. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 17.1 Update or create documentation covering the implementation (`docs/specs/simd-dispatch.md` + `docs/specs/executor-columnar.md` + benchmark numbers in `docs/PERFORMANCE.md`)
- [ ] 17.2 Write tests covering the new behavior (proptest parity for every kernel, unit tests for executor batch path, integration tests on 1M-row WHERE/SUM queries; >= 60 tests total)
- [ ] 17.3 Run tests and confirm they pass (`cargo test --workspace --all-features --verbose`)
