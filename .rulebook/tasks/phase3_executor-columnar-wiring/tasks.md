## 1. Column type inventory + promotion
- [ ] 1.1 Audit the existing `nexus-core/src/execution/columnar.rs::Column` surface against the proposal. Document variants present (today: generic `DataType` + raw buffer), variants missing (`I64`/`F64`/`F32`/`Bool`/`Str` tagged enum, `nulls: Option<BitVec>`). Decision: promote existing `Column` in-place rather than adding a second type.
- [ ] 1.2 Replace `Column { data_type: DataType, buffer: Vec<u8>, len: usize }` with `Column { data: ColumnData, nulls: Option<BitVec>, len: usize }` where `ColumnData` is an enum of `I64(AlignedVec<i64>)`, `F64(AlignedVec<f64>)`, `F32(AlignedVec<f32>)`, `Bool(BitVec)`, `Str(Vec<SmolStr>)`. `AlignedVec` comes from `nexus-core/src/simd/` (or a 32-byte aligned allocator in a fresh `execution/aligned.rs`).
- [ ] 1.3 Migrate every internal caller of the old buffer-based `Column` API (ColumnarResult::filter_column, push_row, …) to the tagged enum. Keep the same public method signatures where possible; add new typed accessors (`as_i64_slice()`, `as_f64_slice()`) for the SIMD consumers.
- [ ] 1.4 Add `Column::materialise_from_rows(rows: &[Row], column: usize, dtype: DataType) -> Result<Self>` — the entry point the executor filter/aggregate use to cross the row→column boundary. Zero-copy where possible (numeric PropertyValue variants); fall back to copy for heterogeneous mixes.

## 2. SIMD kernel unification
- [ ] 2.1 Drop the bespoke `columnar::simd_ops::SimdComparator` (in-file AVX2 block inside `execution/columnar.rs`). Route every compare through `nexus_core::simd::compare::{eq,ne,lt,le,gt,ge}_{i64,f64}`. The canonical kernels already have proptest parity against scalar and runtime dispatch + AVX-512 coverage.
- [ ] 2.2 Add `f32` compare variants to `nexus_core::simd::compare` mirroring the f64 shape. F32 is a pre-existing omission the proposal calls out; landing it now keeps the compare story uniform across ColumnData variants.
- [ ] 2.3 Wire `execution/columnar.rs::Column::compare_scalar()` to call the dispatched kernel for `I64`/`F64`/`F32`, route `Bool`/`Str` through a scalar short-circuit. Return a packed bitmap `Vec<u64>` (not `Vec<bool>`); expose a `Column::as_mask_bitmap()` helper that reuses the existing `simd::bitmap` ops for popcount + and_popcount.

## 3. Filter-operator wiring
- [ ] 3.1 Add `executor.columnar_threshold: usize` (default 4096) to `ExecutorConfig` / `planner::Planner::config`. Plumb through `planner::plan_filter` so it can mark a filter as columnar-eligible.
- [ ] 3.2 Modify `executor/operators/filter.rs::execute_filter` to branch: when input batch length ≥ `columnar_threshold` AND the predicate's LHS is a dense numeric column AND RHS is a scalar, materialise `Column::materialise_from_rows`, call the SIMD compare, decode the bitmap into a row bitmap, and emit the surviving rows.
- [ ] 3.3 For every other filter shape (string predicates, multi-column predicates, subqueries, heterogeneous row widths), reuse the existing row-at-a-time path unchanged.
- [ ] 3.4 Add an `execute_filter` unit test asserting byte-for-byte equality between row-path and columnar-path outputs on 10k-row fixtures (numeric `<`, `<=`, `>`, `>=`, `==`, `!=`).

## 4. Aggregate-operator wiring
- [ ] 4.1 Extend the config threshold to apply to groupless `SUM`/`MIN`/`MAX`/`AVG` in `executor/operators/aggregate.rs::execute_aggregate`. Group-by continues on the row path in this phase (explicit out-of-scope carve-out in the proposal).
- [ ] 4.2 Route groupless numeric aggregates through `nexus_core::simd::reduce::{sum_f64, sum_i64, min_f64, min_i64, max_f64, max_i64}`. `AVG` = `sum / count`, with `count` straight from `Column::len - nulls.popcount()`.
- [ ] 4.3 Handle NaN correctly: `SUM`/`AVG` over `F64` with any NaN collapse to NaN (matches the scalar path proved by the existing aggregate tests); `MIN`/`MAX` ignore NaN operands consistent with Cypher semantics.
- [ ] 4.4 Add a unit test asserting byte-for-byte equality on 10k-row fixtures for each (dtype × op) combination and a property test on the aggregate result against the scalar baseline.

## 5. Planner hint
- [ ] 5.1 Add `PreferColumnar(bool)` variant to `planner::PlanHint` (or the equivalent enum in `executor/planner/mod.rs`). True = force columnar regardless of threshold; False = force row path.
- [ ] 5.2 Wire the hint through `cypher::preparse::Hint` so callers can embed it via a `/*+ PREFER_COLUMNAR */` comment for bench + test control.
- [ ] 5.3 Honour the hint in `execute_filter` / `execute_aggregate` by overriding the threshold check. Document the hint in `docs/specs/executor-columnar.md`.

## 6. Spec + docs
- [ ] 6.1 Extend `docs/specs/executor-columnar.md` with: Column layout, ColumnData variants, null bitmap semantics, SIMD dispatch per op, threshold knob, PreferColumnar hint, planner integration rules.
- [ ] 6.2 Cross-link from `docs/specs/simd-dispatch.md` to the Column type so readers of the kernel spec see the executor-side consumer.
- [ ] 6.3 Add a short note in `docs/performance/PERFORMANCE_V1.md` explaining how to interpret the new columnar bench numbers (speedup vs. row path at different batch sizes).

## 7. Benchmarks
- [ ] 7.1 `nexus-core/benches/executor_filter.rs` — Criterion bench that runs a 100k-row WHERE over integer + float columns via both the row path and the columnar path, reports the ratio. Runs in-process; no server needed.
- [ ] 7.2 `nexus-core/benches/executor_aggregate.rs` — Criterion bench for SUM/MIN/MAX/AVG on i64 + f64 columns at 10k / 100k / 1M sizes, row vs columnar.
- [ ] 7.3 Register both in `nexus-core/Cargo.toml`'s `[[bench]]` list with `harness = false`.
- [ ] 7.4 Extend `scripts/benchmarks/run-protocol-suite.sh` (or add `scripts/benchmarks/run-executor-suite.sh` if the shapes diverge) to include the new benches and emit a numeric speedup summary.

## 8. Regression coverage
- [ ] 8.1 Run the full Neo4j compatibility suite (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`) — expected 299/300 unchanged (or 300/300 if the columnar path fixes the one outlier around aggregate over large batches).
- [ ] 8.2 Run the SDK transport test matrix (`pwsh sdks/run-all-comprehensive-tests.ps1 -Transport rpc`) to confirm no server-side regressions from planner changes.
- [ ] 8.3 `cargo test --workspace` passes. `cargo +nightly clippy --workspace -- -D warnings` passes. `cargo +nightly fmt --check` passes.

## 9. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 9.1 Update or create documentation covering the implementation (`docs/specs/executor-columnar.md`, `docs/specs/simd-dispatch.md` cross-link, `docs/performance/PERFORMANCE_V1.md` note).
- [ ] 9.2 Write tests covering the new behavior (byte-for-byte parity tests for filter + aggregate; Criterion benches; compat suite stays green).
- [ ] 9.3 Run tests and confirm they pass (`cargo test --workspace`, compat suite, `cargo bench --bench executor_filter` + `--bench executor_aggregate`).
