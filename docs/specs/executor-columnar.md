# Executor columnar fast path

> Landed incrementally by `phase3_executor-columnar-wiring` (§3 filter,
> §4 aggregate, §5 planner hint). Cross-linked from
> [`simd-dispatch.md`](./simd-dispatch.md) — the SIMD kernels are the
> layer below this; here we describe how they are wired into the
> executor.

The filter and groupless-aggregate operators both bypass their
row-at-a-time scalar path and call into the canonical `simd::compare`
/ `simd::reduce` kernels when the batch is large enough to amortise
the per-batch materialisation cost. Everything below describes
*when* that happens, *what* data is materialised, and *why* the
columnar result is byte-for-byte identical to the scalar result.

## 1. Column type

The batch representation is
[`nexus_core::execution::columnar::Column`](../../nexus-core/src/execution/columnar.rs):

```rust
#[repr(align(64))] // AVX-512 alignment
pub struct Column {
    pub data_type: DataType,    // Int64 | Float64 | Bool | String
    pub data: Vec<u8>,          // 64-byte-aligned raw buffer
    pub null_mask: Vec<u8>,     // 1 bit per element
    pub len: usize,
}
```

The raw `Vec<u8>` buffer is deliberate: it hands a `&[T]` slice to the
SIMD hot loops via `Column::as_slice::<T>()` without a copy, and the
`#[repr(align(64))]` lines the head of the allocation up to an AVX-512
register boundary so the lane-parallel kernels in `simd::compare` /
`simd::reduce` see aligned reads.

Typed accessors (`as_i64_slice`, `as_f64_slice`, `as_bool_slice`)
return `Option<&[T]>` that only resolves when the column's `data_type`
matches — the safer alternative to the generic `as_slice::<T>()` for
callers that aren't already pinned to a specific SIMD dispatch.

### `Column::materialise_from_rows`

Used by the filter fast path. Walks every row, looks up
`row[variable]` (which must be a `Value::Object` — a node or
relationship), then resolves `property` first at the top level and
then under a nested `"properties"` map (mirrors
`Executor::extract_property`). Returns `None` the first time a row
yields a missing property, `Value::Null`, or a `Number` that can't be
coerced into the requested dtype — this is what makes the NULL and
type-mismatch semantics stay on the scalar path. Only `Int64` and
`Float64` dtypes are supported today; string and bool columnar
filtering stay row-at-a-time.

## 2. Threshold knob

`ExecutorConfig.columnar_threshold: usize` gates the fast path. At
batch sizes below the threshold, the row path stays active — the
per-batch materialisation has a non-zero fixed cost that only
amortises on large inputs.

| Field                 | Default | Meaning |
|-----------------------|---------|---------|
| `columnar_threshold`  | 4096    | Minimum row count before filter / groupless-aggregate consider the columnar path. |

Tuning baseline: 4096 matches the proposal's target — large enough
that msgpack + page-cache overhead dominates and dense-column reads
saturate an L2-worth of float lanes, small enough that an in-flight
query over a medium label slice still benefits. Set `usize::MAX` to
pin everything to the row path (benches do this to produce a scalar
baseline).

## 3. PreferColumnar hint

Query-level override that wins against the threshold check, for
benchmarking and focused tests. Lives in
[`executor::planner::preparse`](../../nexus-core/src/executor/planner/preparse.rs):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanHint {
    PreferColumnar(bool),
}
```

Embedded in the query text as an SQL-style hint comment:

```cypher
/*+ PREFER_COLUMNAR */  MATCH (n:Person) WHERE n.age > 30 RETURN n
/*+ DISABLE_COLUMNAR */ MATCH (n:Person) WHERE n.age > 30 RETURN n
```

The `extract_plan_hints(query) -> (cleaned, Vec<PlanHint>)`
preparser runs before the main Cypher parser, strips only recognised
`/*+ ... */` blocks so unknown hints pass through untouched (future
hint tokens land non-invasively), and populates
`ExecutionContext.plan_hints` before operator dispatch. Matching is
ASCII-only via `str::find` — UTF-8 safe on arbitrary query text.

`ExecutionContext::should_use_columnar(row_count, threshold) -> bool`
is the single decision point both operators call. A `PreferColumnar`
hint wins unconditionally:

- `/*+ PREFER_COLUMNAR */`  → always take the fast path
- `/*+ DISABLE_COLUMNAR */` → always take the row path

Without a hint, `row_count >= threshold` is the only gate.

## 4. Dispatch per op

### Filter: `execute_filter`

When `should_use_columnar(rows.len(), threshold)` returns true AND
the predicate shape is `variable.property OP numeric-literal` with a
comparison op (`=`, `<>`, `<`, `<=`, `>`, `>=`),
`try_columnar_filter_mask` materialises the numeric column and
dispatches through `simd::compare::{eq,ne,lt,le,gt,ge}_{i64,f64}`.
The returned packed bitmap expands into a row-level `Vec<bool>`; the
fast path then runs the same dedup-key loop
(`compute_row_dedup_key` — shared with the scalar path) and pushes
surviving rows in input order.

Every other predicate shape (string compare, `IS NULL`, AND/OR
trees, function calls, parameter RHS, `literal OP property`,
multi-column, subqueries) makes `try_columnar_filter_mask` return
`None` and falls through to the unmodified row-at-a-time path.

### Aggregate: `execute_aggregate_with_projections`

When `group_by` is empty AND
`should_use_columnar(group_rows.len(), threshold)` is true,
`compute_columnar_agg_cache` pre-computes a
`Vec<Option<Value>>` positionally aligned with `aggregations`.
`Some(value)` entries short-circuit the inner scalar match;
`None` entries fall through.

| Aggregation | Kernel                           | Notes |
|-------------|----------------------------------|-------|
| `SUM`       | `simd::reduce::sum_f64`          | Always f64 accumulation — matches scalar's `filter_map.sum::<f64>()` precision regardless of input dtype. Wraps back as `Value::Number::from(i64)` when `sum.fract() == 0.0`, else `from_f64(sum)`. |
| `AVG`       | `simd::reduce::sum_f64` / `len`  | `sum / len as f64` — same precision shape as scalar. |
| `MIN`       | `simd::reduce::min_i64` then fallback `min_f64` | Pure-integer column → `min_i64`, wrap as `Value::Number::from(i64)` (byte-identical to scalar's "keep-original" Value). Mixed / float → `min_f64` + second-pass row lookup to recover the original `Value` (mirrors scalar's first-occurrence strict-less-than loop). |
| `MAX`       | `simd::reduce::max_i64` then fallback `max_f64` | Symmetric to MIN. |
| `COUNT`, `COLLECT`, `PercentileDisc`, `PercentileCont`, `StDev`, `StDevP`, `CountStarOptimized` | — | Always the scalar arm. |

Group-by stays on the row path in this slice (explicit proposal
carve-out — hash-grouping complexity lands after the groupless path
proves the architecture).

### NaN semantics

`serde_json::Number::from_f64(NaN)` returns `None`, so no
`Value::Number` ever holds NaN in the row feed — the materialisers
therefore never observe one. If NaN were ever injected from
elsewhere, the kernels preserve the Cypher expectation:
`sum_f64` propagates NaN (matching the scalar `sum += num` reduction
when any summand is NaN), and `min_f64` / `max_f64` ignore NaN
operands (matching scalar's `num < min_num.unwrap()` where
`NaN < X` evaluates to `false`). Both paths agree in every case.

## 5. Planner integration rules

1. **The threshold is advisory.** A `PreferColumnar` hint in the
   query's `ExecutionContext.plan_hints` wins against the threshold,
   in either direction, without additional planner logic. The
   planner doesn't need to know about the hint — it flows through
   the context to the operators.
2. **Materialisation is the safety valve.** If `materialise_from_rows`
   (filter) or `compute_columnar_agg_cache` (aggregate) returns
   `None` for any reason — missing property, NULL value, wrong
   dtype, mixed int/float in MIN/MAX's fast leg — the operator
   transparently falls back to the row path. The scalar path is
   the authoritative fallback; the fast path is an optimisation
   that must never change observable results.
3. **Dedup parity is structural.** Both filter paths call the same
   `compute_row_dedup_key` helper, so any future change to the
   dedup shape can't drift between the two paths. A byte-for-byte
   parity test (`filter_columnar_matches_row_path_on_10k_*`) would
   fail if they did.
4. **Hint ordering is first-wins.** `ExecutionContext::should_use_columnar`
   scans `plan_hints` in insertion order and returns the first
   `PreferColumnar` match. Callers embedding multiple hints should
   rely on that ordering (the preparser preserves textual order).

## 6. Testing

Parity coverage lives with the operators:

- [`executor::operators::filter::tests`](../../nexus-core/src/executor/operators/filter.rs) —
  `filter_columnar_matches_row_path_on_10k_{int,float}_predicates`,
  `prefer_columnar_hint_forces_fast_path_below_threshold`,
  `disable_columnar_hint_forces_row_path_above_threshold`.
- [`executor::operators::aggregate::tests`](../../nexus-core/src/executor/operators/aggregate.rs) —
  `aggregate_columnar_matches_row_path_on_10k_{i64,f64}`,
  `prop_aggregate_columnar_matches_row_path` (proptest, 20 cases ×
  8 op-column combos), `prefer_columnar_hint_forces_aggregate_fast_path_below_threshold`,
  `disable_columnar_hint_forces_aggregate_row_path_above_threshold`.
- [`executor::planner::preparse::tests`](../../nexus-core/src/executor/planner/preparse.rs) —
  8 recognition-matrix tests for `/*+ ... */` extraction.

Every parity assertion is strict `assert_eq!` — no tolerance. The
proptest fixture keeps every intermediate sum / average exactly
representable as `f64` (ages × 0.5 lands on half-integers) so the
equality is real, not approximate.
