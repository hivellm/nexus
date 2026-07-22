# Proposal: phase0_fix-loadcsv-reorder-drops-predicates

**Priority: HIGH — `LOAD CSV` binds a fresh per-row variable that the
cost-based operator-reordering pass leaves in the `others` bucket, recombined
AFTER `filters`, so a `WHERE` predicate referencing the CSV row (or a
correlated `MATCH` seeded from it) is silently dropped — the exact bug class
already fixed for `VariableLengthPath`/`QuantifiedExpand`/`SpatialSeek`.**
Surfaced during the code review of `phase0_fix-plan-reorder-drops-predicates`;
traced but intentionally left out of that surgical fix's scope.

## Why

`optimize_operator_order`
(`crates/nexus-core/src/executor/planner/queries/cost.rs`) buckets each
operator then recombines them as
`scans -> expansions -> unwinds -> filters -> joins -> others`. `Operator::Unwind`
has an explicit bucket (`unwinds`) that is recombined before `filters`, and it
is the ONLY per-row binder recognized by the `unwind_before_scan` detection
loop (cost.rs:567-598) that also guards the `UNWIND ... MATCH (a:P {id: r.s})`
correlated-seek shape.

`Operator::LoadCsv` (`crates/nexus-core/src/executor/types.rs:611-621`) binds a
fresh `variable` once per CSV row, generated independently of upstream input —
structurally identical to `Unwind`. But it is not in `scans`, `expansions`, or
`unwinds`, so it falls through the bucketing match's `_ => others` catch-all
and is recombined AFTER `filters`. A `Filter` referencing the CSV row variable
therefore runs before that variable is bound; the unbound variable evaluates to
`Null` (falsy) in `value_to_bool` (`eval/predicate.rs:490`), so the predicate
silently drops every row.

Worse, the `unwind_before_scan` detection loop only recognizes
`Operator::Unwind`, not `Operator::LoadCsv`, so the "binder precedes scan"
special-casing that protects `UNWIND ... MATCH (a:P {id: r.s})` does NOT extend
to `LOAD CSV`. `node_index_seek_for`
(`planner/queries/strategy.rs:1360-1376`) builds a correlated `NodeIndexSeek`
(`key_expression: Some(expr)`) for any `Expression::PropertyAccess`/`Variable`
regardless of whether the referenced variable came from `UNWIND` or
`LOAD CSV`, so `LOAD CSV FROM '...' AS row MATCH (n:Label {id: row.id})`
reproduces the correlated-seek bug class, and
`LOAD CSV ... AS row MATCH (n) WHERE row.x = n.id` reproduces the
filter-drops-everything symptom.

## What Changes

- Add `Operator::LoadCsv { .. }` to the `unwind_before_scan` detection loop
  (cost.rs:567-598) alongside `Operator::Unwind`, so a `LOAD CSV` that precedes
  a scan/seek is detected and the CSV-before-scan recombine order is used.
- Route `Operator::LoadCsv` into a bucket recombined BEFORE `filters` — mirror
  the `unwinds` bucket treatment (either extend the `unwinds` arm or add a
  dedicated `load_csv` bucket ordered immediately alongside `unwinds` in both
  recombine branches). Confirm the relative order of a `LOAD CSV` and an
  `UNWIND` in the same query is preserved / correct.
- The general invariant comment added by
  `phase0_fix-plan-reorder-drops-predicates` already states that every
  variable-binding operator must be bucketed before `filters`; this fix brings
  `LoadCsv` into compliance with it.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (LOAD CSV, query planning)
- Affected code:
  `crates/nexus-core/src/executor/planner/queries/cost.rs`
  (`optimize_operator_order` — detection loop + bucketing match),
  `crates/nexus-core/src/executor/types.rs` (`Operator::LoadCsv` shape),
  `crates/nexus-core/src/executor/planner/queries/strategy.rs`
  (`node_index_seek_for` correlated seek)
- Breaking change: NO — correctness fix; `LOAD CSV` queries with a `WHERE` on
  the row variable, or a correlated `MATCH` seeded from the row, return correct
  rows instead of an empty/unfiltered result
- User benefit: bulk-load / ETL queries via `LOAD CSV` no longer silently drop
  `WHERE` predicates or degrade to full scans
- Related: `phase0_fix-plan-reorder-drops-predicates` (same bug class, fixed
  for variable-length paths, quantified path patterns, spatial seeks),
  `phase0_fix-correlated-predicate-index-seek` (correlated `NodeIndexSeek`)
