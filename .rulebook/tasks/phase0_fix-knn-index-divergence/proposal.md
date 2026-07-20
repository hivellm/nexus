# Proposal: phase0_fix-knn-index-divergence

**Priority: HIGH — currently LATENT (no production caller wires KNN/HNSW writes through Cypher
today), but a real index-layer defect: re-inserting a vector for an existing node leaks the old
HNSW entry, and delete never evicts it, so any future write-path wiring will corrupt KNN results
on day one.** Found during a write-path/index corruption audit; not previously reported.

## Why

`add_vector` (`crates/nexus-core/src/index/knn_index.rs:152-164`), when called for a node id that
already has a vector, has an empty "update" branch, then unconditionally inserts a **second** HNSW
vector for the same node and overwrites `node_to_index`, orphaning `index_to_node[old_index]` —
the old HNSW entry is never removed, only the forward mapping is repointed.

`remove_vector` (`:174-189`) only drops the *current* `node_to_index` mapping — it has no way to
reach the orphaned old entry left behind by a prior re-insert, so that entry stays reachable in the
HNSW graph forever.

There is also no `knn_evict_node` anywhere in `engine/crud/*`, in contrast to the sibling
maintenance functions `fts_evict_node`/`spatial_evict_node` that already exist for the fulltext and
spatial indexes. Delete therefore has nothing to call even if it wanted to clean up KNN state.

### Consequences (confirmed by code inspection)

- Re-adding a vector for an existing node leaves BOTH the old and new HNSW entries mapping to the
  same node: KNN queries can return the node **twice**, or return the **stale embedding** instead
  of the current one.
- A subsequent `remove_vector` leaves the orphaned old entry reachable — a **phantom hit after
  delete**.
- Dimension mismatch is correctly rejected today (`:138-144`); this is specifically a same-
  dimension re-insert/evict defect.

### Why LATENT, not reachable today

`add_vector`/`remove_vector` have **no production callers** — only tests and the
`nexus-knn-bench` benchmark harness call them. The KNN index is not yet maintained through Cypher
writes (no CREATE/SET/DELETE path calls into `knn_index.rs`), so no user-facing query can trigger
this today. It becomes live-and-wrong the moment any write path is wired to maintain the KNN
index — which is a stated project direction (native vector search is a core feature per
`docs/specs/knn-integration.md`), so this is a defect to fix before that wiring lands, not one to
leave for whoever adds the wiring to discover.

## What Changes

- Decide (see tasks.md §1) whether to (a) wire a minimal KNN write-path maintenance hook now
  (mirroring `fts_evict_node`/`spatial_evict_node`) so the fix has a real caller to test end-to-
  end, or (b) fix the two mapping bugs in isolation behind the existing test/bench-only callers and
  leave full write-path wiring as an explicit, separate follow-up task. Either way, the mapping
  bugs themselves must be fixed in this task.
- Make `add_vector` remove the stale `index_to_node[old_index]` entry (and the corresponding HNSW
  node) when re-inserting for an id that already has a vector, so only one HNSW entry ever maps to
  a given node id.
- Add `knn_evict_node`, called from `delete_node` (or left as a standalone callable, per the §1
  decision), so a deleted node's vector is fully evicted from both `node_to_index` and
  `index_to_node`.

## Impact

- Affected specs: `docs/specs/knn-integration.md` (vector index maintenance contract)
- Affected code: `index/knn_index.rs` (`add_vector:152-164`, `remove_vector:174-189`),
  `engine/crud/` (new `knn_evict_node` + call site, mirroring `fts_evict_node`/`spatial_evict_node`)
- Breaking change: NO — no production caller exists yet, so no observable behavior changes for
  current users; this closes a defect before it becomes reachable
- User benefit: once KNN write-path maintenance is wired (in this task or a follow-up),
  re-inserting or deleting a vector will not silently duplicate results or leak phantom hits
- Related: none of the other write-path tasks in this audit touch the KNN index; this is the only
  vector-index defect found
