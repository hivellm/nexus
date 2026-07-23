# Proposal: phase0_fix-where-in-prefix-param-index-seek

**Priority: MEDIUM — the remaining WHERE index-seek forms split out of
`phase0_fix-where-clause-index-seek-extensions`, whose range (`>`/`>=`/`<`/`<=`)
seek and EXPLAIN/PROFILE plan-accuracy landed. Three forms still full-scan.**

## Why

`phase0_fix-where-clause-index-seek-extensions` landed range seeking (a new
`Operator::NodeIndexRangeSeek` over the B-tree, `where_range_seek_operand` in
`strategy.rs`) and fixed EXPLAIN/PROFILE to plan via the real path. It scoped
out three forms that still emit the `Nexus.Performance.UnindexedPropertyAccess`
notification and full-scan even when an index exists:

1. **`IN [a, b, c]`** — lowerable to a union of point seeks (`find_exact` per
   element, OR the bitmaps). Needs an `Operator::NodeIndexInSeek { values }` (or
   equivalent) + plan detection.
2. **`STARTS WITH 'x'`** — a string prefix range. The property index already has
   `find_prefix` (`cache/property_index.rs`); needs a prefix-seek operator +
   plan detection.
3. **`$parameter` equality (`WHERE n.prop = $x`)** — the planner has no bound
   values at plan time, and `NodeIndexSeek`'s `key_expression` correlated path
   needs driving rows a bare first scan lacks. Needs a parameter-aware seek that
   resolves the value from `context.params` at execution time without requiring
   driving rows (a new operator, or a first-scan single-seek path).

## What Changes

- Add `IN` → union-of-point-seeks and `STARTS WITH` → prefix-range-seek lifting
  on indexed properties (the index already supports both natively:
  `find_exact` and `find_prefix`).
- Add `$parameter` equality seeking that resolves the value at execution time.
- Keep the unindexed notification firing only for forms that still can't seek
  (e.g. `CONTAINS`).

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (WHERE-form index-seek coverage)
- Affected code: `crates/nexus-core/src/executor/types.rs` (new seek operators),
  `crates/nexus-core/src/executor/operators/scan.rs` (handlers),
  `crates/nexus-core/src/executor/planner/queries/strategy.rs` (lift detection,
  next to `where_range_seek_operand`), the exhaustive `Operator` matches
  (dispatch/cost)
- Breaking change: NO — plan-selection only; results unchanged
- User benefit: `IN`, `STARTS WITH`, and parameterized-equality WHERE queries on
  indexed properties get index-seek performance instead of a full scan
- Related: `phase0_fix-where-clause-index-seek-extensions` (parent: range +
  EXPLAIN landed), `phase0_fix-where-clause-index-seek` (grandparent: equality)
