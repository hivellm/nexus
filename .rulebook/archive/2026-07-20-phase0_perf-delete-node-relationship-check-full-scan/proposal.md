# Proposal: phase0_perf-delete-node-relationship-check-full-scan

**Priority: MEDIUM (performance) — every non-DETACH node delete now does an
O(total_relationships) full store scan.** Found by code review of
`phase0_fix-delete-node-dangling-relationships`.

## Why

`node_has_live_relationship` (`crates/nexus-core/src/engine/crud/nodes.rs`,
added by the dangling-edge fix) scans the ENTIRE relationship store to decide
whether a node still has a live edge:

```rust
for rel_id in 0..self.storage.relationship_count() {
    let rel = self.storage.read_relationship(rel_id)?;
    if !rel.is_deleted() && (rel.src_id == node_id || rel.dst_id == node_id) {
        return Ok(true);
    }
}
```

It is called on every non-DETACH `delete_node` (Cypher, REST, RPC, RESP3). This
mirrors the existing `delete_node_relationships` scan, so it is not a new
pattern — but it makes each delete O(total edges) instead of O(degree). On a
graph with millions of relationships this blows the project's stated <1ms
point-operation target for deletes. Correctness is fine (bounded, read-only,
ignores soft-deleted edges); this is purely about cost.

## What Changes

- Replace the full-store scan with a bounded lookup over the node's own
  adjacency: walk the outgoing chain via `first_rel_ptr`/`next_src_ptr`, and
  find incoming edges via the existing relationship index
  (`self.cache.relationship_index()`, already used by
  `delete_node_relationships`) or a dst-keyed lookup — not a `0..count` scan.
- Apply the same optimization to `delete_node_relationships` if it shares the
  full-scan cost, so both the guard and the DETACH clear are O(degree).

## Impact

- Affected specs: none
- Affected code: `crates/nexus-core/src/engine/crud/nodes.rs`
  (`node_has_live_relationship`, and possibly `delete_node_relationships`)
- Breaking change: NO — same results, faster
- User benefit: node deletes stay O(degree) on large graphs instead of
  O(total relationships)
- Related: `phase0_fix-delete-node-dangling-relationships` (introduced the guard
  this optimizes)
