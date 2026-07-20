# Tasks: phase0_fix-delete-path-index-cleanup

`Engine::delete_node` frees the node record but never walks the index layer that create populated:
the composite B-tree keeps a deleted node's NODE KEY tuple forever (H-1, a user-visible false
constraint violation on tuple reuse), the property-store blob is never freed (M-1, a storage leak),
and the typed property B-tree keeps a dead entry (M-3, masked bloat today). All three are missing
calls in the same function, `delete_node` (`engine/crud/nodes.rs:364-402`).

Order matters: prove H-1's false-rejection first since it is the only one with user-visible wrong
behavior today (§1), fix it (§2), then add the two lower-severity cleanup calls (§3, §4) — all
three edits land in the same function, so each must be verified independently before the tail
exercises them together.

## 1. Reproduce H-1 first
- [ ] 1.1 Write a failing integration test: `CREATE CONSTRAINT ... IS NODE KEY`, `MERGE` a node,
  `DETACH DELETE` it, then `MERGE` the identical tuple again. Confirm it fails today with
  `ERR_CONSTRAINT_VIOLATED NODE_KEY` even though no live node holds the tuple
- [ ] 1.2 Confirm via code inspection that `index/composite_btree.rs:103-110`'s `remove` has zero
  callers (grep the crate) and that `engine/constraints.rs:356-369`'s NODE KEY check
  (`seek_exact`) applies no `is_deleted` filter, so the stale entry is indistinguishable from a
  live one

## 2. Fix H-1: evict composite B-tree entries on delete
- [ ] 2.1 In `delete_node` (`engine/crud/nodes.rs:364-402`), for each composite index registered
  on the node's labels, call `composite_btree.find(...).write().remove(id, &tuple)` with the
  node's current property tuple
- [ ] 2.2 Make the §1.1 test pass; add a second case covering `DETACH DELETE` of a
  relationship-bearing node (not just a bare node) to confirm the composite eviction runs on the
  same code path used by both delete forms

## 3. Fix M-1: free property-store blobs on delete
- [ ] 3.1 Call `delete_node_properties` (`storage/record_store_ops.rs:1346-1359`) from
  `delete_node`, and `delete_relationship_properties` from the relationship-delete path, so both
  currently-zero-caller functions are wired in
- [ ] 3.2 Write a test that creates and deletes N nodes with properties in a loop and asserts the
  property-store file size stops growing unboundedly (or the freed-space accounting reflects the
  delete), proving the leak is closed

## 4. Fix M-3: clean the typed property B-tree on delete
- [ ] 4.1 Call `property_index.remove_property` for each typed-indexed property key on the node's
  labels, from `delete_node`
- [ ] 4.2 Write a test that creates a node with a typed-indexed property, deletes it, then inspects
  index occupancy directly (not just query results, since `is_deleted()` re-checks already mask
  this at read time) to confirm the dead entry is actually gone

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 5.1 Update `docs/specs/cypher-subset.md` (or the constraint-enforcement section) noting that
  DELETE evicts composite/typed index entries and frees property storage; add a CHANGELOG entry
- [ ] 5.2 Tests: NODE KEY tuple reuse after DETACH DELETE succeeds (§1.1 regression), property
  storage does not leak across delete/create cycles (§3.2), typed index has no dead entries after
  delete (§4.2)
- [ ] 5.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-delete-node-dangling-relationships` — same delete path, a different defect:
  dangling relationships instead of index residue
- `phase0_fix-update-node-index-divergence` — same index layer, diverges on the update path
  instead of the delete path
