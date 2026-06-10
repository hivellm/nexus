# Proposal: phase6_fix-set-indexed-property-stale-index

## Why
Manual Docker validation found that `SET` on an indexed property leaves the
typed property index stale, producing wrong query results in BOTH
directions: a seek by the new value returns nothing (the node is
invisible), and a seek by the old value returns the renamed node (stale
entry serves the seek; the projection then reads the current storage
value). Reproduced on the published 2.3.2 image. While fixing it, a second
member of the same family surfaced: a non-transactional Cypher `CREATE`
(executor write path) never adds the node to the typed index at all — the
node is invisible to seeks (and a follow-up `MATCH {prop} SET` silently
no-ops) until a restart or an explicit-tx commit rebuilds the index.

## What Changes
- `persist_node_state` (the choke point for SET property / SET += /
  REMOVE / label changes) captures the pre-write property bag + labels and
  calls a new `typed_index_refresh_node` helper: evict the node's old
  `(label, key, value)` entries, re-add from the new state (registered
  indexes only) — joining the existing FTS and spatial refresh siblings.
- The standalone-CREATE dispatch branch captures a node-count watermark
  around `executor.execute` and indexes the allocated id range via a new
  `index_typed_properties_for_new_nodes` helper (same write-set source as
  the #15 scoped-commit maintenance; exact under the single-writer model).

## Impact
- Affected specs: indexing / write paths
- Affected code: `crates/nexus-core/src/engine/crud/{lookup,index_maintenance}.rs`,
  `crates/nexus-core/src/engine/query_pipeline.rs`
- Breaking change: NO (restores intended index-correctness semantics)
- User benefit: index-backed MATCH returns correct rows after SET / CREATE —
  no invisible nodes, no stale matches.
