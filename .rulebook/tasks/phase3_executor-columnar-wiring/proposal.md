# Proposal: phase3_executor-columnar-wiring

## Why

`phase2_simd-executor-filter-aggregate` landed the SIMD compare / reduce
kernels at a high standard (22 compare parity proptests + 18 reduce
parity proptests, AVX-512 `sum_f64` 7.9× at 262K inputs, AVX-512
`lt_i64` 4.4× at 262K, NEON variants of all reduce kernels). The
kernels are fully exercised at the `nexus-core/src/simd/*` layer.

What it did not land: the executor-side wiring that would funnel
filter and aggregate inputs through those kernels. The bottleneck
is a missing type — there is no `Column` / `ColumnData` batch type
that `execute_filter` / `execute_aggregate` can materialise into
before calling the SIMD primitives. Today the executor operates on
`Vec<Row>` and evaluates predicates per-row.

Every downstream item from phase2 — executor filter path
(§9), aggregate SIMD path (§10), PreferColumnar planner hint (§11),
end-to-end executor benchmarks (§14.3–14.4) — is blocked on this one
architectural decision. Splitting it out as its own phase-3 task
makes the dependency explicit.

## What Changes

- Introduce `nexus-core/src/execution/column.rs` (or promote the
  existing `execution::columnar::Column` placeholder into a real
  type) carrying:

  ```rust
  pub enum ColumnData {
      I64(AlignedVec<i64>),
      F64(AlignedVec<f64>),
      F32(AlignedVec<f32>),
      Bool(BitVec),
      Str(Vec<SmolStr>),
  }

  pub struct Column {
      data: ColumnData,
      nulls: Option<BitVec>,
      len: usize,
  }
  ```

- Add `executor.columnar_threshold: usize` config (default 4096) that
  routes filter + aggregate through the column path when the row count
  exceeds the threshold.
- Wire `execute_filter` to materialise inputs into `Column`, run the
  `simd::compare` kernel matching the predicate, and emit a row
  bitmap.
- Wire `execute_aggregate` groupless `SUM` / `MIN` / `MAX` / `AVG` on
  numeric columns through the matching `simd::reduce` kernel.
- Add the `PreferColumnar` planner hint that forces the columnar
  branch regardless of threshold (for benchmarks + tests).
- Extend `docs/specs/executor-columnar.md` describing the new Column
  type, the threshold knob, and the per-kernel dispatch.

## Impact
- Affected specs: `docs/specs/executor-columnar.md` (new),
  `docs/specs/simd-dispatch.md` (cross-link to Column type).
- Affected code: `nexus-core/src/execution/column.rs`,
  `nexus-core/src/executor/operators/{filter,aggregate}.rs`,
  `nexus-core/src/executor/planner/mod.rs`,
  `nexus-core/benches/executor_filter.rs` (new),
  `nexus-core/benches/executor_aggregate.rs` (new).
- Breaking change: NO (internal; row-at-a-time path remains the
  fallback for small batches and non-numeric columns).
- User benefit: 3–8× filter speedup on WHERE clauses over dense
  numeric columns; 3–7× groupless aggregation speedup. End-to-end
  numbers blocked on this wiring.

## Source

- `phase2_simd-executor-filter-aggregate` (archived with §1 / §9 /
  §10 / §11 / §13.2 / §14.3 / §14.4 marked as deferred here) —
  every SIMD kernel below the Column type is landed and benched.

## Out of Scope
- String compare kernels — tracked as a phase-3 follow-up on its own
  because byte-scanner design differs from numeric compare.
- Welford variance / stddev SIMD — tracked separately once Column
  type lands; the scalar path is correct and handles NaN today.
- GROUP BY columnar path — the groupless path proves the
  architecture first; group-by adds hash-grouping complexity best
  landed after the groupless numbers are validated.
