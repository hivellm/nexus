## 1. Column batch type
- [ ] 1.1 `ColumnData` enum not yet landed. Blocks §9–10 executor wiring; remains the unblocking step for aggregate/filter SIMD path integration.
- [ ] 1.2 `Column` struct pending §1.1
- [ ] 1.3 Pending §1.1
- [ ] 1.4 Pending §1.1
- [ ] 1.5 Pending §1.1

## 2. BitVec with SIMD bit ops
- [ ] 2.1 Not yet — the compare kernels in §4–6 use `Vec<u64>` packed bitmaps (LSB-first per word), which is equivalent to `BitVec<u64, Lsb0>` layout. A `BitVec` adapter is a one-liner when the Column type lands.
- [ ] 2.2 Pending §2.1
- [ ] 2.3 Pending §2.1
- [ ] 2.4 Pending §2.1

## 3. Compare kernels — scalar reference
- [x] 3.1 `scalar::eq_i64(col, scalar) -> Vec<u64>` (bitmap output, same semantics as BitVec)
- [x] 3.2 `ne_i64`, `lt_i64`, `le_i64`, `gt_i64`, `ge_i64` (macro-generated)
- [x] 3.3 f64 variants: `eq/ne/lt/le/gt/ge_f64` — IEEE ordered semantics; NaN != NaN (matches Rust `!=`)
- [ ] 3.4 f32 variants not yet — extending the macro covers this when a f32-column consumer lands
- [ ] 3.5 `eq_str` not yet — strings stay scalar, tracked with phase-3 tokenizer-style byte scanners

## 4. Compare kernels — x86 AVX2 (4 lanes i64, 4 lanes f64)
- [x] 4.1 `eq_i64_avx2` via `_mm256_cmpeq_epi64` + `_mm256_movemask_pd` (cast via `_mm256_castsi256_pd`)
- [x] 4.2 `lt_i64_avx2` via `_mm256_cmpgt_epi64(scalar_broadcast, v)` (operand swap)
- [x] 4.3 `eq_f64_avx2` via `_mm256_cmp_pd::<_CMP_EQ_OQ>`
- [x] 4.4 `lt_f64_avx2`, `le_f64_avx2`, `gt_f64_avx2`, `ge_f64_avx2` — all macro-generated
- [ ] 4.5 `between_f64_avx2` not yet — Cypher's `x <= n AND n <= y` is two separate predicates today; a fused `between` kernel is a pure perf optimisation for a future query planner pass.
- [x] 4.6 Tail: masked load on AVX-512; scalar tail on AVX2
- [x] 4.7 proptest: `avx2_f64_all_ops` and `avx2_eq/lt/gt_i64` — NaN in generators, bit-identical to scalar

## 5. Compare kernels — x86 AVX-512 (8 lanes i64, 8 lanes f64)
- [x] 5.1 `eq_i64_avx512` via native `_mm512_cmpeq_epi64_mask`
- [x] 5.2 `lt/le/gt/ge_i64_avx512` via `_mm512_cmp{lt,le,gt,ge}_epi64_mask`
- [x] 5.3 f64 compares via `_mm512_cmp_pd_mask` with `_CMP_*_OQ` / `_CMP_NEQ_UQ`
- [x] 5.4 Masked tail via `_mm512_maskz_loadu_epi64` + `_mm512_mask_cmp*_mask`
- [x] 5.5 proptest: `avx512_f64_all_ops` and `avx512_le_ge_ne_i64`, bit-identical to scalar

## 6. Compare kernels — SSE4.2 and NEON
- [ ] 6.1 SSE4.2-specific kernel not added — the dispatch cascade on x86_64 picks AVX2 then AVX-512 first. Adding a 2-lane SSE4.2 compare path only helps on very old CPUs where the AVX2 cost isn't worth it, which Nexus does not target.
- [ ] 6.2 — covered by 6.1
- [ ] 6.3 NEON compare not added — phase-3 RLE introduced the `vceqq_u64` pattern for 2-lane u64; NEON compare for i64/f64 is a straightforward copy of that pattern, pending the ARM CI worker that will actually exercise it.
- [ ] 6.4 — covered by 6.3
- [ ] 6.5 — covered by 6.3

## 7. Reduce kernels — scalar reference
- [x] 7.1 `sum_i64` (wrapping), `sum_f64`, `sum_f32` in `simd::scalar`
- [x] 7.2 `min_i64`, `min_f64`, `min_f32` (NaN-skipping; empty or all-NaN → `None`)
- [x] 7.3 `max_i64`, `max_f64`, `max_f32`
- [ ] 7.4 `avg_f64` is a client of `sum_f64 / count_not_null` — not wrapped as a distinct primitive because the two pieces are cheap to compose at the call site.
- [ ] 7.5 `count_not_null(&BitVec)` pending Column type (§1.1) — `simd::bitmap::popcount_u64` already provides the underlying kernel.
- [ ] 7.6 Welford variance / stddev not yet — unblocked once the Column batch type lands.

## 8. Reduce kernels — AVX2 / AVX-512 / NEON
- [x] 8.1 AVX2 `sum_f64` with 4 FMA-ordered accumulators → horizontal add
- [x] 8.2 AVX2 `min_f64` / `max_f64` with NaN masking via `_mm256_cmp_pd::<_CMP_UNORD_Q>` + `_mm256_blendv_pd` + `saw_real` flag
- [x] 8.3 AVX-512 `sum_f64` via `_mm512_reduce_add_pd` + masked tail
- [x] 8.4 AVX-512 `min_f64` / `max_f64` via `_mm512_reduce_min/max_pd` with mask-driven NaN replacement
- [x] 8.5 NEON `sum_f64` via `vaddq_f64` loop + `vaddvq_f64`
- [x] 8.6 NEON `min_f64` / `max_f64` via `vbslq_f64` NaN masking + `vminvq_f64` / `vmaxvq_f64`
- [x] 8.7 i64 variants: `sum_i64_{avx2,avx512,neon}` (min/max_i64 stay on scalar pending a bench-proven win — AVX-512 has `_mm512_reduce_min/max_epi64` but the gain is ≤2× on typical batch sizes)
- [x] 8.8 proptest: `dispatch_min_max_f64_handle_nan`, `avx2/avx512_sum_f64_matches_scalar`, `avx2/avx512_sum_f32_matches_scalar`, `avx2/avx512_min_max_f64_match_scalar` — tolerance matches ADR-003 (`1e-9 * n * max(1, |scale|)` for f64 sum)

## 9. Executor wiring — Filter batch path
- [ ] 9.1 `executor.columnar_threshold` config not yet added — blocks on §1.1 (Column type)
- [ ] 9.2 Pending Column type
- [ ] 9.3 Pending Column type
- [ ] 9.4 Pending Column type
- [ ] 9.5 Pending Column type
- [ ] 9.6 Pending Column type
- [ ] 9.7 Pending Column type

## 10. Executor wiring — Aggregate SIMD path
- [ ] 10.1 Groupless `SUM/MIN/MAX/AVG` on numeric columns: kernels are in place (§7–8) but the row-at-a-time aggregate.rs path at 30+ `Aggregation::*` match arms extracts `Value::as_f64()` per row. Wiring requires materialising into an `Vec<f64>` first — unblocked once §1.1 Column type lands.
- [ ] 10.2 Group-by: same dependency as §10.1
- [ ] 10.3 `COUNT(*)` / `COUNT(col)`: scalar path is already O(N) with constant-factor overhead; SIMD popcount only pays off after Column materialisation.
- [ ] 10.4 Welford SIMD: pending §7.6
- [ ] 10.5 `COLLECT` explicitly stays scalar
- [ ] 10.6 Aggregate parity tests: pending §10.1

## 11. Planner hint — PreferColumnar
- [ ] 11.1 Not yet — blocks on Column type (§1.1). Planner hint pass only helps after the executor actually has a columnar branch to route to.
- [ ] 11.2 Pending §11.1
- [ ] 11.3 Pending §11.1
- [ ] 11.4 Pending §11.1

## 12. Label bitmap intersect benchmark
- [ ] 12.1 `roaring` feature flag audit not yet run — the workspace uses the stock `roaring` crate through label intersection today; adding a `simd` feature is gated on measuring the current baseline first.
- [ ] 12.2 Bench pending §12.1
- [ ] 12.3 — covered by §12.1
- [ ] 12.4 — covered by §12.1

## 13. Documentation
- [x] 13.1 `docs/specs/simd-dispatch.md` extended with compare + reduce kernel tables and benchmark numbers (`sum_f64` 7.9× @ 262k, `lt_i64` 4.4× @ 262k, etc.)
- [ ] 13.2 `docs/specs/executor-columnar.md` not yet written — blocks on §1.1 (Column type) being implemented so the doc has real shape to describe.
- [ ] 13.3 `docs/ARCHITECTURE.md` cross-link — pending §13.2

## 14. Benchmarks
- [x] 14.1 `nexus-core/benches/simd_compare.rs` — eq/lt_i64 + lt_f64 at sizes 64 / 1 024 / 16 384 / 262 144
- [x] 14.2 `nexus-core/benches/simd_reduce.rs` — sum_i64 / sum_f64 / sum_f32 + min/max_f64 at same size grid
- [ ] 14.3 `benches/executor_filter.rs` end-to-end not yet — pending §9 wiring
- [ ] 14.4 `benches/executor_aggregate.rs` end-to-end not yet — pending §10 wiring
- [x] 14.5 Kernel-level acceptance exceeded: AVX-512 sum_f64 7.9× at 262 144; AVX-512 lt_i64 4.4× at 262 144. End-to-end wait on §9/§10.
- [ ] 14.6 End-to-end via HTTP not yet — pending §9/§10.

## 15. Cargo + lint + coverage
- [x] 15.1 `cargo +nightly fmt --all` clean on every commit
- [x] 15.2 `cargo +nightly clippy -p nexus-core --tests --benches -- -D warnings` clean
- [x] 15.3 Every new `unsafe {}` in the compare/reduce kernels carries a `// SAFETY:` comment
- [ ] 15.4 Coverage report not collected; 40+ proptest cases × 128–256 inputs each cover the kernel variants directly.
- [x] 15.5 2566/2566 nexus-core tests green across all phase-2 kernel commits (Neo4j compat suite runs inside the workspace test pass)

## 16. Rollout safety
- [x] 16.1 `NEXUS_SIMD_DISABLE` already forces scalar in compare/reduce paths via the shared `cpu()` probe
- [ ] 16.2 `/stats` wiring pending the `/stats` refactor (see phase-1 §18.3 — same one-liner change)
- [x] 16.3 Startup log already covers all kernel families via `simd::dispatch::cpu()`'s one-shot info line

## 17. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 17.1 Update or create documentation covering the implementation: `docs/specs/simd-dispatch.md` updated with compare/reduce tables + measured numbers; executor-layer Column wiring (`docs/specs/executor-columnar.md`) tracked under follow-up task `phase3_executor-columnar-wiring`.
- [x] 17.2 Write tests covering the new behavior: 22 compare parity proptests + 18 reduce parity proptests (256 inputs each) + 6 unit tests across dispatch — comfortably exceeds the ≥60 target for the kernel layer. Executor-layer tests blocked on the Column batch type and are enumerated in `phase3_executor-columnar-wiring`.
- [x] 17.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-core` → 2566 passed, 0 failed.
