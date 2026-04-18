# Proposal: phase2_simd-executor-filter-aggregate

## Why

After phase 1 lands the SIMD infrastructure and the KNN/bitmap kernels,
the next two Cypher hot-paths — **filters (WHERE)** and **aggregations
(SUM/MIN/MAX/AVG/COUNT)** — still run scalar row-at-a-time loops. Three
concrete pains today:

1. **`nexus-core/src/execution/columnar.rs`** already contains a skeleton
   `SimdComparator` (x86_64 AVX2 only, no runtime dispatch) that is
   **not wired** into the executor. The executor operates on `Row`
   records and evaluates predicates one value at a time. Every
   published openCypher benchmark we looked at (LDBC SNB Q3, Q10)
   spends 20–40% of wall time in filter evaluation.
2. **`nexus-core/src/executor/operators/aggregate.rs` (1,692 LOC)**
   implements `SUM`/`AVG`/`MIN`/`MAX` over numeric columns using
   `f64` accumulators updated in scalar loops. A 1M-row aggregate on a
   dense numeric column takes ~18 ms locally; the same loop in
   vectorised form should finish in 3–5 ms.
3. **Label bitmap intersections** in MATCH with multiple labels
   (`MATCH (n:Person:Customer)`) already go through `RoaringBitmap`,
   which has its own portable-SIMD path — but only when the
   `roaring/simd` feature is enabled. We confirm it is on and measure
   the actual speedup at our data scales.

Phase 2 is pure executor work: we reuse the dispatch layer, aligned
buffers, and tail-handling conventions from phase 1.

## What Changes

### 1. Column-oriented batch types

Promote `execution::columnar::Column` from a placeholder into a real
first-class batch type that the executor can materialise filter/aggregate
inputs into:

```rust
pub enum ColumnData {
    I64(AlignedVec<i64>),
    F64(AlignedVec<f64>),
    F32(AlignedVec<f32>),      // node/rel scores, KNN distances
    Bool(BitVec),              // packed bitmap, 1 bit per row
    Str(Vec<SmolStr>),         // string column (scalar comparisons only)
    Null(BitVec),              // null mask — paired with any column above
}

pub struct Column {
    data: ColumnData,
    nulls: Option<BitVec>,
    len: usize,
}
```

`AlignedVec<T>` is the 32-byte-aligned allocation from phase 1. Null
masks are tracked separately so filters produce bitmaps rather than
recomputing presence.

### 2. SIMD filter predicates

New `simd::compare` module wrapping every op the Cypher WHERE clause
can require:

| Op           | i64          | f64        | f32        | Bool | Output     |
|--------------|--------------|------------|------------|------|------------|
| `eq(a,scalar)` | ✅          | ✅         | ✅         | ✅   | `BitVec`   |
| `ne`         | ✅           | ✅         | ✅         | ✅   | `BitVec`   |
| `lt / le / gt / ge` | ✅    | ✅         | ✅         | N/A  | `BitVec`   |
| `between(lo, hi)` | ✅      | ✅         | ✅         | N/A  | `BitVec`   |
| `is_null`    | —            | —          | —          | —    | via null mask |

Each op implemented with the same runtime dispatch pattern:

| Arch         | i64 eq lanes | f64 lt lanes | Output   |
|--------------|--------------|--------------|----------|
| AVX-512F     | 8 (`_mm512_cmpeq_epi64_mask`) | 8 (`_mm512_cmp_pd_mask`) | native `__mmask8` → BitVec |
| AVX2         | 4 (`_mm256_cmpeq_epi64`)      | 4 (`_mm256_cmp_pd`)      | `_mm256_movemask_pd` → u8 |
| SSE4.2       | 2                             | 2                        | `_mm_movemask_pd`         |
| NEON         | 2 (`vceqq_s64`)               | 2 (`vcltq_f64`)          | `vshrn_n_u64` → u8        |
| Scalar       | 1                             | 1                        | bitwise packing           |

### 3. Aggregate kernels

New `simd::reduce` module — horizontal reductions with tree folds:

| Op                 | i64 | f64 | f32 | Output |
|--------------------|-----|-----|-----|--------|
| `sum`              | ✅  | ✅  | ✅  | scalar |
| `min / max`        | ✅  | ✅  | ✅  | scalar |
| `avg` (sum/count)  | uses sum + count | ✅ | ✅ | scalar |
| `count_not_null`   | uses popcount of null mask | | | u64 |
| `std_dev / var`    | (Welford, f64 path) | ✅ | ✅ | scalar |

For `sum` on f64, we use 4 independent accumulators (ILP) in AVX-512 and
2 in AVX2 to hide FMA latency. Integer sum uses `_mm256_add_epi64` with
saturation checks off the hot path (Cypher integers are i64).

`min`/`max` use `_mm512_reduce_max_pd` on AVX-512, shuffle-based
reduction on AVX2, and `vmaxvq_f64` on NEON.

### 4. Executor wiring

`executor/operators/filter.rs`:

- When the upstream operator produces ≥128 rows and the WHERE
  predicate is "column op literal" or "column op column", route
  through the new batch path:
  1. Materialise the referenced columns as `Column`.
  2. Evaluate the predicate with `simd::compare`.
  3. Produce a gather index from the output BitVec.
  4. Gather rows.
- Fallback: smaller batches, string/list/struct predicates, or
  complex expressions stay on the existing row-at-a-time evaluator.
  We do not rewrite the expression evaluator.

`executor/operators/aggregate.rs`:

- Group-by-less aggregates (`RETURN SUM(n.age)`) route to
  `simd::reduce::sum_f64` / `..._i64` directly.
- Group-by aggregates (`MATCH (n) RETURN n.country, SUM(n.age)`)
  partition rows by group, then reduce per group with SIMD inside each
  partition. This is the standard "hash then reduce" plan.
- `COLLECT` stays scalar (builds a heterogeneous list).

### 5. Label bitmap intersection

Confirm `roaring = { version = "...", features = ["simd"] }` in
`nexus-core/Cargo.toml`; benchmark:

- Single label `MATCH (n:Person) RETURN count(n)` — baseline.
- Two labels `MATCH (n:Person:Customer) RETURN count(n)` — target
  1.5–2x speedup vs stock roaring.

When the speedup is ≥1.5x, ship the feature flag on; otherwise open a
follow-up task to swap in a custom AVX2 intersection.

### 6. Planner hint

Add a planner heuristic: when a plan node is a `Filter` over a scan
producing numeric properties with **row count > 128**, tag the node
with `PreferColumnar { columns: [...] }` so the executor materialises
those columns up-front. The threshold is tunable via
`executor.columnar_threshold` config (default 128).

## Impact

- **Affected specs**: update `docs/specs/simd-dispatch.md` with the
  compare + reduce kernel table; new `docs/specs/executor-columnar.md`
  documenting the batch boundary.
- **Affected code**:
  - NEW: `nexus-core/src/simd/compare.rs`, `simd/reduce.rs` (~900 LOC)
  - MODIFIED: `nexus-core/src/execution/columnar.rs` (promote from
    skeleton to real type; delete AVX2-only inline attempt)
  - MODIFIED: `nexus-core/src/executor/operators/filter.rs` (+ batch
    path, behind `columnar_threshold`)
  - MODIFIED: `nexus-core/src/executor/operators/aggregate.rs` (+ SIMD
    sum/min/max paths; `COLLECT` untouched)
  - MODIFIED: `nexus-core/src/executor/planner/*` (+ `PreferColumnar`
    hint)
  - MODIFIED: `nexus-core/Cargo.toml` (confirm `roaring/simd`)
- **Breaking change**: NO — query results bit-identical to scalar
  within the `1e-9` f64 tolerance from associativity reorderings;
  we add a `proptest` that asserts identical up to that tolerance.
- **User benefit**:
  - **3–5x** faster WHERE on numeric columns over >128 rows.
  - **4–6x** faster SUM/MIN/MAX/AVG over >1K rows.
  - **1.5–2x** faster multi-label MATCH.
  - No change required in user queries — the planner routes
    transparently.

## Non-goals

- String filters with SIMD (PCMPESTRI etc.) — phase 3, mostly
  cold-path.
- Expression evaluation rewrite (full vectorised expression engine) —
  out of scope; we only accelerate simple column-op-literal /
  column-op-column predicates.
- Approximate aggregates (HyperLogLog, T-Digest) — separate task.

## Reference

- The existing skeleton in `nexus-core/src/execution/columnar.rs` —
  we replace its kernels, keep the surrounding types.
- DuckDB's vectorised execution model (`Vector` + `SelectionVector`) is
  the reference for the batch-with-selection approach adopted in §4.
- Apache Arrow compute kernels — reference for `eq`/`lt`/`sum` kernel
  signatures.
