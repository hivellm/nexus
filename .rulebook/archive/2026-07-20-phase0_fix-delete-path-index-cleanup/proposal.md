# Proposal: phase0_fix-delete-path-index-cleanup

**Priority: HIGH — deleting a node leaves index and storage residue in three places, one of which
is a user-visible false constraint violation.** Found during a write-path/index corruption audit;
not previously reported.

## Why

`Engine::delete_node` (`crates/nexus-core/src/engine/crud/nodes.rs:364-402`) removes the node
record but never evicts the node from three places that reference it by id.

**H-1 — composite B-tree (NODE KEY) never evicted, causing permanent false constraint violations.**
`delete_node` performs no composite-index removal, and the NODE KEY existence check
(`engine/constraints.rs:356-369`) reads `seek_exact` with no `is_deleted` filter.
`index/composite_btree.rs:103-110` defines `remove`, but it has **zero callers**. Node ids are
monotonic and never recycled (`record_store.rs:181-213`), so a deleted node's NODE KEY tuple sits
in the composite tree forever and permanently blocks reuse of that tuple by a new node:

```
CREATE CONSTRAINT FOR (p:Person) REQUIRE (p.tenantId,p.id) IS NODE KEY;
MERGE (p:Person {tenantId:'t1',id:1});       -- populates composite_btree
MATCH (p:Person {tenantId:'t1',id:1}) DETACH DELETE p;
MERGE (p2:Person {tenantId:'t1',id:1});      -- ERR_CONSTRAINT_VIOLATED NODE_KEY, though no live node holds it
```

This is not bloat — it is a false rejection of a legitimate write, permanently, for any tuple ever
assigned to a now-deleted node.

**M-1 — property chains never freed on delete (storage leak).**
`delete_node_properties`/`delete_relationship_properties` (`storage/record_store_ops.rs:1346-1359`)
have **zero callers**; neither `delete_node` nor `delete_node_relationships` calls them. The
property-store file only grows across create/delete cycles — a slow, unbounded storage leak on any
workload that deletes nodes.

**M-3 — typed property B-tree not cleaned on delete.**
`crud/nodes.rs:364-402` never calls `property_index.remove_property`. This is currently masked by
read-side `is_deleted()` re-checks (`scan.rs:96-115`, `lookup.rs:212-223`), so today it is inert
index bloat — but it exposes any future count/cost path that trusts index occupancy without
re-checking liveness, and it compounds the same stale-residue pattern as H-1.

All three share one root cause: `delete_node` frees the record but never walks the index layer
that create populated.

## What Changes

- In `delete_node`, call `composite_btree.find(...).write().remove(id, &tuple)` for each
  registered composite index covering the node's labels, so a deleted node's NODE KEY/composite
  tuple is freed for reuse by a future node.
- Call `delete_node_properties` (and `delete_relationship_properties` from the relationship delete
  path) so the property-store blob for a deleted entity is actually freed instead of orphaned.
- Call `property_index.remove_property` for each typed-indexed property on the node's labels, so
  the typed B-tree no longer carries dead entries.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (DELETE / constraint semantics)
- Affected code: `engine/crud/nodes.rs`, `engine/constraints.rs`, `index/composite_btree.rs`,
  `storage/record_store_ops.rs`
- Breaking change: NO — behavior only becomes more correct (deleted tuples become reusable,
  storage stops leaking); no currently-passing query changes result
- User benefit: DETACH DELETE followed by re-creating the same NODE KEY tuple succeeds instead of
  failing with a permanent false constraint violation; property storage no longer leaks across
  delete/create cycles
- Related: `phase0_fix-delete-node-dangling-relationships` (same delete path, a different defect —
  dangling relationships instead of index residue), `phase0_fix-update-node-index-divergence`
  (same index layer, diverges on the update path instead of the delete path)
