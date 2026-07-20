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
- [x] 1.1 Failing integration test written (`node_key_tuple_reusable_after_detach_delete` in
  `tests/executor/node_key_delete_reuse_test.rs`): CREATE CONSTRAINT ... IS NODE KEY, MERGE a node,
  DETACH DELETE it, MERGE the identical tuple. Confirmed RED today —
  `ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY tuple=["tenantId","id"] not unique`.
- [x] 1.2 Confirmed: `index/composite_btree.rs` `remove(node_id, &[PropertyValue])` had ZERO
  callers; `engine/constraints.rs` NODE KEY check uses `seek_exact` with only an `exclude_node_id`
  filter, no `is_deleted` filter — a stale tuple is indistinguishable from a live one.

## 2. Fix H-1: evict composite B-tree entries on delete
- [x] 2.1 Added `unindex_composite_tuples` (engine/crud/index_maintenance.rs), the exact inverse of
  `index_composite_tuples` (rebuild tuple in key order, `find(lbl,&keys).write().remove(id,&tuple)`),
  and call it from `delete_node` after the live-relationship guard, reading props while the record's
  prop_ptr still points at the live blob.
- [x] 2.2 §1.1 test passes; added `node_key_tuple_reusable_after_detach_delete_of_relationship_bearing_node`
  (populate via MERGE, add a relationship, DETACH DELETE) confirming eviction on the DETACH path.
  (Note: the executor CREATE operator does not populate the composite index, so the second case
  populates via MERGE to be a real eviction test.)

## 3. Fix M-1: free property-store blobs on delete
- [x] 3.1 `delete_node` now calls `self.storage.delete_node_properties(id)`; the relationship-delete
  paths (`delete_relationship` and the DETACH `delete_node_relationships` loop) call
  `delete_relationship_properties(rel_id)` — both previously-zero-caller functions are wired in.
- [x] 3.2 `property_blob_freed_after_node_delete` — creates 5 nodes with properties, deletes all,
  asserts `RecordStore::property_count()` (deterministic live-entry count, NOT a file-size
  heuristic) returns to the pre-test baseline, proving the leak is closed.

## 4. Fix M-3: clean the typed property B-tree on delete
- [x] 4.1 Added `unindex_node_properties` (index_maintenance.rs, inverse of
  `maintain_indexed_properties`) calling `property_index.remove_property` per registered
  (label,key); called from `delete_node`.
- [x] 4.2 `typed_index_has_no_dead_entry_after_delete` — creates a typed-indexed node, asserts
  `property_index.find_exact` (RAW node-id bitmap, no `is_deleted` filter) contains it, deletes the
  node, asserts the bitmap is now empty — inspecting index occupancy directly, not via queries.

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 CHANGELOG entry added under `[3.0.0]` (`### Fixed — phase0_fix-delete-path-index-cleanup`,
  covering H-1/M-1/M-3). The `docs/specs/cypher-subset.md` edit is WAIVED: the fix makes DELETE
  behavior match the already-documented constraint/DELETE semantics (freeing a tuple on delete was
  always the intent); there is no new user-facing semantic to specify beyond the CHANGELOG note.
- [x] 5.2 Tests: NODE KEY tuple reuse after DETACH DELETE (§1.1 + rel-bearing variant), property
  storage does not leak (§3.2), typed index has no dead entries (§4.2) — 4/4 green.
- [x] 5.3 Green: `cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core --all-targets --all-features -- -D warnings` (0 warnings), full
  `cargo +nightly test -p nexus-core` (2422 lib + all integration groups, 0 failed) plus the
  workspace test run below.

## Related
- `phase0_fix-delete-node-dangling-relationships` — same delete path, a different defect:
  dangling relationships instead of index residue
- `phase0_fix-update-node-index-divergence` — same index layer, diverges on the update path
  instead of the delete path
