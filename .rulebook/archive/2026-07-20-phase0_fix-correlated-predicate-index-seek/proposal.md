# Proposal: phase0_fix-correlated-predicate-index-seek

**Priority: HIGH — a property index is silently bypassed whenever the value it is
compared against comes from an earlier row.** Found while diagnosing
`phase0_fix-cypher-oom-process-abort`, of which this is the underlying performance
cause. Not previously reported.

## Why

A property predicate is only index-backed when its value is a **constant**. The
moment the value is correlated — it comes from an `UNWIND` row, or any earlier
binding — the planner falls back to scanning every node of the label and filtering
afterwards, once per driving row.

This is the single most important access pattern for bulk loading and for any
parameterized batch operation, and it is quadratic today.

### Confirmed empirically (Nexus 2.5.0, release build)

3 000 `:P` nodes, index on `:P(id)`:

| Query | Behaviour |
|---|---|
| `MATCH (a:P {id: 42}) RETURN a.id` | index seek, instant |
| `UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id` | **30 rows/s** |

And it is worse than linear in the driving rows, because the cross product is
materialized before the filter is applied:

| Driving rows | Time | |
|---:|---:|---|
| 200 | 6.7 s | |
| 400 | 22.7 s | 3.4× for 2× the input |

Notably the planner emits **no** `UnindexedPropertyAccess` notification for the
correlated form, so the one diagnostic Nexus offers for exactly this problem stays
silent — the query looks indexed and is not.

### Consequences

- `phase0_fix-cypher-oom-process-abort`: this is why a 5 000-row load builds a
  1.25e11-cell table and asks for 4 TB. Bounding the allocation there stops the
  crash; only this task makes the query actually work.
- `phase7_ldbc-snb-benchmark`: loading 576 896 SF0.1 edges means resolving endpoints
  by LDBC id per row. At 30 rows/s that is over five hours for SF0.1 alone.
- Every SDK batch-write path built on `UNWIND` inherits the same quadratic
  behaviour.

## What Changes

- Teach the planner that a property predicate whose value is a *row-local
  expression* is still index-eligible: the seek key is evaluated per driving row
  rather than once at plan time.
- Add the corresponding execution shape — for each driving row, evaluate the key and
  perform an index seek, instead of scanning the label and filtering after a cross
  product. This is the standard `NodeIndexSeek` with a runtime-evaluated key.
- Until the notification logic can distinguish "indexed" from "correlated and
  therefore not indexed", make `UnindexedPropertyAccess` fire for the correlated
  case — a silent slow path is worse than a noisy one.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (index usage rules)
- Affected code: `crates/nexus-core/src/executor/planner/queries/` (predicate →
  index-seek selection), the scan/seek operators, and the notification logic in
  `executor/planner/queries/unindexed.rs`
- Breaking change: NO — same results, different plan
- User benefit: parameterized batch reads and writes stop being quadratic; bulk
  loading by natural key becomes viable
- Related: `phase0_fix-cypher-oom-process-abort` (the crash this causes),
  `phase0_fix-ingest-bulk-path` (the other half of the bulk-load story)
