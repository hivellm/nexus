# Proposal: phase0_fix-where-clause-index-seek-extensions

**Priority: MEDIUM — follow-ups to `phase0_fix-where-clause-index-seek`, which
landed WHERE-form equality seeks, unindexed-access notifications, composite-index
seeks, and the join-cost fix. Three scoped-out extensions remain.**

## Why

The parent task deliberately scoped its seek support to constant-literal
equality. Three gaps remain, each observable but not yet optimized:

1. **Range / IN / STARTS WITH / CONTAINS seeking.** `MATCH (n:Person) WHERE
   n.age > 30` (and `IN`, `STARTS WITH`, `CONTAINS`) still full-scan even when an
   index exists. They now emit the unindexed-access notification (from the parent
   task's #6) but never seek. A range seek needs a range-seek operator / B-tree
   range scan; `IN` can lower to a disjunction of point seeks; `STARTS WITH` can
   use a prefix range on a string index.
2. **`$parameter` equality seeking.** `MATCH (n:Person) WHERE n.age = $age`
   cannot be lifted at plan time because the planner has no access to bound
   parameter values (supplied at execution time), and `NodeIndexSeek`'s
   `key_expression` correlated path requires pre-existing driving rows — a bare
   first-scan `WHERE n.prop = $x` has none. Needs either a parameter-aware seek
   operator that resolves the value at execution time, or an execution-time
   rewrite.
3. **EXPLAIN / PROFILE plan accuracy.** `execute_explain_with_string` /
   `execute_profile_with_string` (`crates/nexus-core/src/engine/query_pipeline.rs`,
   ~lines 913 and 960) build their DISPLAY plan via `QueryPlanner::new(...).with_rtree(...)`
   only — they wire neither `property_index` NOR `composite_index`, so an EXPLAIN'd
   plan shows `NodeByLabel` + `Filter` where the actual execution path
   (`Executor::plan_ast`) would show a `NodeIndexSeek`/`CompositeBtreeSeek`. This
   is a pre-existing gap (property_index was never wired there either) surfaced
   during the parent task's composite-index review. EXPLAIN should reflect the
   real plan.

## What Changes

- Add range-seek support (range-seek operator or B-tree range scan) and lift
  range / `IN` / `STARTS WITH` WHERE predicates on indexed properties; keep the
  unindexed notification firing only for the forms that still can't seek.
- Add parameter-aware equality seeking (`WHERE n.prop = $x`) via an execution-time
  value resolution path.
- Wire `property_index` and `composite_index` into the EXPLAIN/PROFILE display
  planners so the shown plan matches execution.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (WHERE-clause index usage)
- Affected code: `crates/nexus-core/src/executor/planner/queries/strategy.rs`
  (`where_equality_index_seek_for`/`composite_index_seek_for` — extend to range /
  IN / STARTS WITH / parameter), `crates/nexus-core/src/executor/types.rs` (a
  range-seek operator if needed), `crates/nexus-core/src/engine/query_pipeline.rs`
  (EXPLAIN/PROFILE planner wiring)
- Breaking change: NO — plan-selection / diagnostics only; results unchanged
- User benefit: range/IN/prefix and parameterized WHERE queries on indexed
  properties get index-seek performance; EXPLAIN reflects the real plan
- Related: `phase0_fix-where-clause-index-seek` (parent, landed in f8eb857e +
  67eba262), `phase0_fix-correlated-predicate-index-seek`
