# Proposal: phase6_add-src-type-dst-edge-index

Follow-up from `phase6_fix-planner-merge-unindexed-on2` (item 3.3).

## Why
Edge-MERGE existence (`find_relationship_between`) walks the source node's
relationship chain — O(out-degree of source). The node-MERGE index fix
removed the dominant O(N) cost (endpoint resolution), so this is no longer
the meltdown driver. But a single hub node accumulating very high same-type
out-degree via repeated `MERGE (hub)-[:T]->(x)` still pays O(degree) per
merge → O(degree²) to build the hub. A `(src, type, dst)` existence index
gives true O(1) edge lookup for that pathological hub case.

## What Changes
- Add a relationship-existence index keyed by `(src_id, type_id, dst_id)`
  (hash set / map), maintained on relationship create/delete.
- `find_relationship_between` consults it first (O(1)); falls back to the
  source-chain walk only when the index is unavailable.
- Keep it consistent under WAL replay and the typed adjacency store.

## Impact
- Affected specs: storage / adjacency, cypher-subset / merge
- Affected code: `crates/nexus-core/src/storage/` (new edge-existence index),
  `crates/nexus-core/src/engine/mod.rs` (`find_relationship_between`,
  create/delete relationship maintenance)
- Breaking change: NO
- User benefit: high-degree hub MERGE workloads stay O(1) per edge instead
  of O(degree).
