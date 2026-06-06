# Proposal: phase6_fix-planner-merge-unindexed-on2

Source: production incident (Cortex stack, 12 services) — Nexus melted under a
write burst; recovery took ~4 min after restart. Reporter's diagnosis
("phase25"): the planner does not use indexes for MERGE, so edge-MERGE is
O(n²); under a write backlog (large new session, re-bootstrap, accumulated
queue) the pathological edge-MERGE re-fires and Nexus degrades.

## Why
`MERGE` must look up whether the target node/relationship already exists. If
that lookup does a full scan instead of an index seek, a batch of M merges
against a graph of N elements is O(M·N) — quadratic — which stalls the
single writer thread and wedges the server under load. The
`phase6_merge-unindexed-property-warning` work only *warns* about unindexed
property access; it does not make MERGE use an index. This task makes the
MERGE match path index-backed so writes stay near-linear under load.

## What Changes
- Identify the exact O(n²) site(s):
  - node MERGE `MERGE (n:Label {key: $v})` — existence lookup must use the
    label + property (B-tree / composite) index, not an all-nodes scan + filter.
  - relationship/edge MERGE `MERGE (a)-[r:T {..}]->(b)` — finding an existing
    `:T` edge between `a` and `b` must use the source node's typed adjacency,
    not a scan of all relationships.
- Make the MERGE planner emit index-seek operators when a covering index
  exists (mirroring how read MATCH already uses `NodeByLabel`/index seeks),
  with a correct fallback (and the existing unindexed warning) when no index
  exists.
- Rewrite the edge-MERGE existence check to use the source node's typed
  adjacency rather than a global relationship scan.
- Add a benchmark/regression proving MERGE cost scales ~linearly with graph
  size, not quadratically.

## Impact
- Affected specs: cypher-subset / planner, storage / adjacency
- Affected code: `crates/nexus-core/src/executor/planner/` (MERGE planning),
  `crates/nexus-core/src/executor/operators/` (merge / expand / create),
  `crates/nexus-core/src/index/`, adjacency in `crates/nexus-core/src/storage/`
- Breaking change: NO (same results; faster). Response format unchanged.
- User benefit: write bursts (bootstrap, large sessions, backlog drain) no
  longer melt the server; MERGE-heavy workloads stay responsive.

## Notes
- Separate from the 2.3.0 correctness fixes (#3–#6) already shipped.
- A flaky test in this area (`tests/unindexed_property_notification_e2e_test.rs`
  — notifications leak across consecutive queries) currently blocks the
  release push; it is a test-isolation bug to address alongside this work.
