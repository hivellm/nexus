# Proposal: phase6_fix-read-match-index-seek

Source: GitHub issue #8 (https://github.com/hivellm/nexus/issues/8)

## Why
On 2.3.0 the **read-side** `MATCH (n:Label {prop: val})` does NOT use a
property index even when one exists — it does a full label scan. The 2.3.0
index-backed-MERGE fix covered only the write/MERGE existence path
(`find_nodes_by_node_pattern`); the executor read path (`execute_node_by_label`
+ filter; `try_index_based_filter` is a stub returning `Ok(None)`) still
scans. Worse, a comma-joined two-node `MATCH (a:L1 {..}), (b:L2 {..})` is
planned as a **cartesian product** of two unindexed label scans → O(N²).
Edge upserts resolve their endpoints exactly this way:

```
MATCH (a:Turn {id:$a}), (b:ToolCall {id:$b}) MERGE (a)-[r:HAS_TOOL_CALL]->(b)
```

On a ~154k-node graph a single such lookup times out (>25s, core pinned),
so a batch of edge upserts melts the server (13–34 min per 256-event batch).
A single-node read seek for a missing key also takes ~671ms (full scan)
despite a covering index.

## What Changes
- Read-side `MATCH (n:Label {prop: val})`: use the property B-tree
  (`property_index.find_exact`, intersecting per-property bitmaps) when a
  covering index exists; fall back to the label scan otherwise. Implement
  this in the executor (replace/finish the `try_index_based_filter` stub in
  `executor/operators/scan.rs`, and/or have the planner emit an index-seek
  operator for an indexed `(label, prop)` node selector).
- Comma-joined / multi-pattern MATCH: push each node pattern's property
  predicate into its own leg so each endpoint is an independent index seek,
  NOT a cartesian product of two full scans. This is the load-bearing fix
  for edge-upsert throughput.
- Validate single-node seek is O(log N) (<5ms on a missing key with a
  covering index) and the two-node comma-join no longer scans cartesianly.

## Impact
- Affected specs: cypher-subset / planner, executor / scan+filter
- Affected code: `crates/nexus-core/src/executor/operators/scan.rs`
  (`execute_node_by_label`, `try_index_based_filter`),
  `crates/nexus-core/src/executor/planner/queries.rs` (node-selector +
  multi-pattern planning), `crates/nexus-core/src/index/` (property_index)
- Breaking change: NO (same results; dramatically faster). Response format
  unchanged.
- User benefit: read MATCH by indexed property is O(log N); edge-upsert
  endpoint lookups stop being O(N²); edge-heavy write bursts drain instead
  of pinning the server.

## Notes
- Complements the write-side fix (`phase6_fix-planner-merge-unindexed-on2`)
  and the edge-existence index (`phase6_add-src-type-dst-edge-index`).
