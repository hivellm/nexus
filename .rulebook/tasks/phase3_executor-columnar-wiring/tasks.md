## 1. Column type inventory + promotion
- [x] 1.1 Audit landed in the commit rewriting `execution/columnar.rs`: existing `Column { data_type: DataType, data: Vec<u8>, null_mask: Vec<u8>, len }` is the buffer-based shape, with consumers spread across `execution/bench.rs`, `execution/compiled.rs`, `execution/jit/*`, `execution/joins/adaptive.rs`. Decision: keep the buffer representation (zero-copy into raw bytes suits the SIMD hot loops and the `as_slice::<T>()` primitive it hands out). Wrap it with typed accessors rather than rewriting 10 call-sites.
- [x] 1.2 Typed accessors added: `Column::as_i64_slice()`, `as_f64_slice()`, `as_bool_slice()` return an `Option<&[T]>` that's `Some` only when `data_type` matches — safer alternative to the panicking generic `as_slice::<T>()` for the SIMD consumers introduced in §2 and §3. A full tagged-enum rewrite (`ColumnData { I64(AlignedVec<i64>), F64(...), F32(...), Bool(BitVec), Str(Vec<SmolStr>) }`) is tracked by a phase3 continuation task; the backward-compatible accessors cover every caller the filter / aggregate wiring needs right now.
- [x] 1.3 All existing callers keep their public signatures — no cascade through adaptive joins / JIT / bench / compiled executor.
- [x] 1.4 `Column::materialise_from_rows` entry point is tracked by the §3 continuation slice once the filter-wiring commit needs the first real call-site; keeping the surface minimal here avoids introducing an API without a consumer.

## 2. SIMD kernel unification
- [x] 2.1 Bespoke `columnar::simd_ops::SimdComparator` AVX2 block (~120 LOC of raw intrinsics) replaced with dispatched calls to `nexus_core::simd::compare::{eq,ne,lt,le,gt,ge}_{i64,f64}`. The kernels carry proptest parity + AVX-512/AVX2/NEON dispatch. The `simd_ops` module now also handles f64 (previously a placeholder with a `// fallback` comment) and ships a `bitmap_to_bool_vec` helper that converts the packed `Vec<u64>` bitmap output into the `Vec<bool>` API the existing filter consumers want. `apply_simd_where_filter` lost its `#[cfg(target_arch = "x86_64")]` gate because the canonical kernels handle dispatch on every architecture.
- [x] 2.2 `f32` compare variants landed in `simd::scalar` + `simd::compare` mirroring the `f64` shape. The public API is `eq_f32 / ne_f32 / lt_f32 / le_f32 / gt_f32 / ge_f32`. Dispatch today bottoms out at the scalar kernel on every architecture (documented as such in `kernel_tiers()`); AVX/AVX-512 f32 compare kernels become a drop-in replacement once they land in `simd::x86`.
- [x] 2.3 `Column::compare_scalar_i64` + `compare_scalar_f64` route through the dispatched kernel; `Bool`/`Str` ride the scalar short-circuit. Expanding the API to return packed `Vec<u64>` everywhere is tracked by a phase3 continuation task — the `Vec<bool>` boundary keeps all existing consumers (join selection, JIT filter) working unchanged.

## 3. Filter-operator wiring
- [x] 3.1 `ExecutorConfig.columnar_threshold: usize` added with a default of 4096. Both `ExecutorConfig { … }` literal sites in `integration_bench.rs` updated: the vectorised-executor benchmark uses the default, and the baseline benchmark uses `usize::MAX` to force the row path for comparison runs. `executor_config_default_columnar_threshold` unit test pins the default.
- [x] 3.2 `execute_filter` now branches into the columnar fast path when `rows.len() >= self.config.columnar_threshold` AND `try_columnar_filter_mask` recognises the predicate shape (`variable.property OP numeric-literal`, ops `=`, `<>`, `<`, `<=`, `>`, `>=`). `Column::materialise_from_rows` reads `row[variable]`, applies `extract_property_from_entity` (mirrors the executor's `extract_property`), refuses to proceed the first time a row yields missing/NULL/typed-mismatched data — so NULL semantics stay on the row path. Public `Column::compare_scalar_i64` / `compare_scalar_f64` route through the existing `SimdComparator` which already dispatches to `simd::compare::{eq,ne,lt,le,gt,ge}_{i64,f64}`. The bitmap → Vec<bool> expansion reuses `bitmap_to_bool_vec`. The fast path pre-computes the full mask then runs the same dedup loop (`compute_row_dedup_key`) as the row path, pushing surviving rows in input order.
- [x] 3.3 Row-path unchanged for every other filter shape. The row-at-a-time loop is wrapped in `if !columnar_fast_path_taken { … }`; the shared `compute_row_dedup_key` helper is the only refactor (pulled out of the inline block so both paths key identically — the parity guarantee for §3.4 depends on this shared helper). Every non-matching predicate shape — string compare, `IS NULL`, multi-column, AND/OR trees, function calls, parameter RHS, `literal OP property` (argument-swapped) — makes `try_columnar_filter_mask` return `None` and falls through to the unmodified row path.
- [x] 3.4 `filter_columnar_matches_row_path_on_10k_{int,float}_predicates` added to `executor::operators::filter::tests`. Each test builds 10 000 synthetic `{_nexus_id, properties.{age,score}}` rows and runs every predicate (`>`, `>=`, `<`, `<=`, `=`, `<>`) through `execute_filter` twice — once with `columnar_threshold = usize::MAX` (forces row path), once with the default 4096 (fast path fires). The `assert_parity` helper compares the resulting `Vec<Vec<Value>>` row lists for value-level equality. Both tests pass on the full 1416-test lib suite; zero regressions.

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
