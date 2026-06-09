# Proposal: phase6_propagate-relindex-add-error

Source: GitHub issue #18 (https://github.com/hivellm/nexus/issues/18)

## Why
On `create_relationship`, the exact-edge index update is best-effort and
discards errors (`crates/nexus-core/src/engine/crud.rs:992-998`):

```rust
if let Err(e) = self.cache.relationship_index()
    .add_relationship(rel_id, from, to, type_id)
{
    tracing::warn!("Failed to update relationship index: {}", e);
    // Don't fail the operation, just log the warning
}
```

A silently-failed add (poisoned RwLock after a panic, or the #17
nested-lock hazard) leaves the `(src,type,dst)` exact-edge entry missing.
The next `MERGE (a)-[:R]->(b)` on that edge misses the fast path in
`find_relationship_between` (mod.rs:3293) and falls back to the O(degree)
chain walk (#12 pathology); worst case the index is stale enough that
MERGE creates a duplicate edge. Silent index corruption -> query-level
correctness degradation.

## What Changes
- Propagate the error from the `relationship_index().add_relationship`
  call (crud.rs:992) instead of swallowing it — or, if the write must not
  fail, set a dirty-bit that forces an index rebuild before the next query
  so the exact-edge index can never silently diverge from storage.
- Leave the phase-8 manager updates (crud.rs:1017/1029/1370/1384)
  best-effort but logged (they are not on the MERGE fast path) — or apply
  the same dirty-bit approach if low-cost.

## Impact
- Affected specs: relationship index integrity / MERGE existence
- Affected code: `crates/nexus-core/src/engine/crud.rs`
- Breaking change: NO
- User benefit: MERGE edge existence stays correct (no duplicate edges)
  and O(log N) (no silent fallback to O(degree)) even when an index update
  fails.

## Notes
- Audit finding #4. Couples with #17 (a fixed nested-lock removes one cause
  of the failure) and #12 (the O(degree) fallback this prevents).
