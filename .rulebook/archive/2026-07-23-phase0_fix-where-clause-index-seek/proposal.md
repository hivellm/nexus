# Proposal: phase0_fix-where-clause-index-seek

**Priority: HIGH — a WHERE-form equality predicate on an indexed property
silently full-scans instead of using the index (a correctness-preserving but
severe performance cliff), and three related planner index/cost gaps compound
it.** Found during a planner correctness/performance audit; not previously
reported.

## Why

Nexus only builds an index seek for INLINE node-pattern properties.
`node_index_seek_for`
(`crates/nexus-core/src/executor/planner/queries/strategy.rs:1307-1342`) walks
`node.properties` — the `{prop: value}` map attached directly to the pattern
node — and returns a `NodeIndexSeek` when a matching `(label_id, key_id)` index
exists:

```rust
fn node_index_seek_for(
    &self,
    node: &NodePattern,
    label_id: u32,
    variable: &str,
) -> Option<Operator> {
    let prop_idx = self.property_index?;
    let property_map = node.properties.as_ref()?;
    for (prop_name, expr) in &property_map.properties {
        ...
```

It is never consulted for WHERE-clause predicates. Every WHERE clause is
unconditionally lowered to `Filter`/`OptionalFilter` (`strategy.rs:419-441`),
regardless of whether the predicate is a plain equality on an indexed property.
The executor does have a filter→index fast path, `try_index_based_filter`, but
it is a stub: after finding matching entity ids via the index, it discards them
and falls back to a full scan, in BOTH the equality and range branches
(`crates/nexus-core/src/executor/operators/scan.rs:181-186` and `:244-247`):

```rust
if !entity_ids.is_empty() {
    // Convert entity IDs to rows - this would need more context in production
    // For now, return None to use regular filtering
    // TODO: Implement full row construction from indexed entities
    return Ok(None);
}
```

So `MATCH (n:Person {age:30})` seeks the index, but the semantically identical
`MATCH (n:Person) WHERE n.age = 30 RETURN n` full-scans every `:Person` node.
This is distinct from the already-known correlated-predicate bypass (a WHERE
predicate comparing two matched variables) — this is a plain constant-literal
equality, the single most common WHERE form there is.

Three related gaps compound the same code paths:

- **#6 notification gap**: the "unindexed access" notification that should warn
  operators when a WHERE predicate misses an index is only emitted for
  `BinaryOperator::Equal`
  (`crates/nexus-core/src/executor/planner/queries/unindexed.rs:151-152`); range
  (`>`, `<`, `>=`, `<=`), `IN`, `STARTS WITH`, and `CONTAINS` predicates
  full-scan with no diagnostic signal at all (`unindexed.rs:166` only warns when
  `!has_index`, gated behind the same `Equal`-only match). This is the
  regression signal operators would use to notice #5-class perf cliffs, so it
  must exist and be complete once #5 is addressed — otherwise a partial #5 fix
  (e.g. equality-only) leaves range predicates full-scanning silently, with no
  way to tell.
- **#7 composite indexes**: `node_index_seek_for` only probes single-property
  indexes — `prop_idx.has_index(label_id, key_id)` (`strategy.rs:1332`) — so it
  can never find a composite `(a, b)` index. `MATCH (n:L {a:1, b:2})` with only
  a composite index on `(a,b)` full-scans even though results are correct (the
  index is simply dead weight); the planner never constructs
  `Operator::CompositeBtreeSeek` from an inline multi-property selector.
- **#8 join cost/cardinality category error**: `Operator::Join`'s Inner-join
  cost estimator (`crates/nexus-core/src/executor/planner/queries/cost.rs:296-297`)
  computes `left_cardinality`/`right_cardinality` by calling
  `estimate_plan_cost(&[left])` — a COST estimator — and assigning its return
  value (a cost figure, not a row count) directly into a variable named
  `*_cardinality`, which then drives the cartesian-product estimate and output
  cardinality at `:303-304`. The correct helper, `estimate_operator_cardinality`,
  already exists and is used two arms down for `Union` (`:332-333`). This is a
  plan-quality defect (no allocation is sized from it), but every join ordering
  decision in the cost-based planner is built on a category error.

## What Changes

- Lift equality and range WHERE predicates on `(label, property)` combinations
  that have a registered index into `NodeIndexSeek` (or a range-seek operator)
  at plan time — mirroring what already happens for inline pattern properties —
  **or**, as an alternative implementation strategy, finish
  `try_index_based_filter`'s row construction from the entity ids it already
  finds (`scan.rs:181-186`, `:244-247`) so the executor-level fast path actually
  short-circuits instead of discarding its own result.
- Extend the unindexed-access notification (`unindexed.rs`) to cover range,
  `IN`, `STARTS WITH`, and `CONTAINS` WHERE predicates, not just `Equal`, so
  every full-scan-despite-index case is observable.
- Detect inline multi-property selectors covered by a registered composite
  index and emit `Operator::CompositeBtreeSeek` instead of falling through to a
  full scan.
- Replace the `estimate_plan_cost(&[left/right])` calls in the `Join`
  cost-estimation arm (`cost.rs:296-297`) with `estimate_operator_cardinality`,
  matching the pattern already used for `Union`.
- Order the fix so #5 (the seek) lands before #6 (its notification signal):
  landing the notification first would make it fire (correctly, but
  confusingly) on cases #5 hasn't fixed yet; landing the seek first lets #6's
  test suite assert the notification correctly goes SILENT for the now-fixed
  equality case while still firing for the not-yet-covered range/IN/string-match
  forms.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (WHERE-clause index usage,
  notification contract)
- Affected code:
  `crates/nexus-core/src/executor/planner/queries/strategy.rs`
  (`node_index_seek_for` `:1307-1342`, WHERE lowering `:419-441`),
  `crates/nexus-core/src/executor/planner/queries/unindexed.rs` (`:151-152`,
  `:166`), `crates/nexus-core/src/executor/planner/queries/cost.rs`
  (`:296-297`, `:332-333`), `crates/nexus-core/src/executor/operators/scan.rs`
  (`try_index_based_filter` `:181-186`, `:244-247`)
- Breaking change: NO — query results are unchanged; this is a plan-selection
  and diagnostics fix, not a semantics fix (CompositeBtreeSeek and join-cost
  changes are likewise result-preserving)
- User benefit: WHERE-form queries on indexed properties get the same
  index-seek performance as inline-pattern-form queries; operators get complete
  unindexed-access diagnostics; composite indexes are actually used; join
  ordering decisions are based on real cardinality estimates
- Related: `phase0_fix-correlated-predicate-index-seek` — the sibling
  correlated-predicate index bypass in the same code area
