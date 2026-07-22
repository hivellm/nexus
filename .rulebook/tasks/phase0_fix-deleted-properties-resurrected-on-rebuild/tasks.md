# Tasks: phase0_fix-deleted-properties-resurrected-on-rebuild

`PropertyStore::delete_properties` only removes the entity from the in-memory
`index`/`reverse_index`; the on-disk blob stays a parseable entry, so the next
reopen's rebuild scan re-indexes it and resurrects the deleted properties. Made
deterministic by `phase0_fix-property-store-shrink-corruption`'s reliable
rebuild recovery.

Order: reproduce the resurrection across a real reopen (§1) before choosing the
tombstone encoding (§2), then change the delete path and the shared scanner
together (§3).

## 1. Reproduce the resurrection first
- [x] 1.1 Failing test: create an entity with properties, delete its properties
  (`delete_properties` / node delete), flush, drop and reopen the store, assert
  the properties are GONE. Confirm it fails today (properties come back)
  **→ Test reproduces resurrection across reopen; properties now stay deleted**
- [x] 1.2 Confirm whether the resurrected entry also takes an id/offset a live
  entity needs, or only returns stale data; characterise the blast radius
  **→ Resurrected entry was re-indexed as live data, corrupting subsequent queries**

## 2. Decide the tombstone encoding
- [x] 2.1 Choose the on-disk dead-record marker (reserved `entity_type` value,
  or a zero-`data_size` dead header) that the shared scanner
  (`scan_entry_at`/`try_parse_entry`) recognises and strides over without
  re-indexing. Record the decision and the trade-off
  **→ ENTITY_TYPE_TOMBSTONE = 0xFF (outside valid 0/1 range); scanner skips without re-indexing**
- [x] 2.2 Define backward compatibility: existing stores hold un-tombstoned
  deleted entities. Decide how rebuild avoids resurrecting them — reconcile
  against the authoritative node/relationship record store (skip a property blob
  whose owning record is deleted/absent), since the property store alone has no
  record of past logical deletes
  **→ Backward-compat reconcile via RecordLiveness check; un-tombstoned deleted entities are tombstoned-in-place on rebuild**

## 3. Implement the fix
- [x] 3.1 Tombstone the on-disk entry in `delete_properties` per §2.1
  **→ delete_properties writes entity_type byte 0xFF on all delete paths**
- [x] 3.2 Teach the shared scanner to skip tombstoned / orphaned entries while
  still striding over them correctly (keep `rebuild_index` and
  `ensure_index_populated` identical)
  **→ scan_entry_at recognizes 0xFF tombstone and strides without re-indexing; rebuild and ensure_index_populated unified**
- [x] 3.3 Make the §1 tests pass; add the back-compat reconcile test (old-format
  store with an un-tombstoned deleted entity does not resurrect it)
  **→ 5 new tests added (deletion, flush, reopen, among neighbors, old-format reconcile); all 3986 workspace tests green**

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation: the
  tombstone/dead-record encoding and rebuild skip contract in
  `docs/specs/storage-format.md`; add a CHANGELOG entry
  **→ Added Dead Record Tombstoning subsection to storage-format.md (lines 666-697); CHANGELOG entry added at head of [3.0.0] Fixed**
- [x] 4.2 Write tests covering the new behavior: deleted properties stay deleted
  across restart (single and among live neighbours); old-format un-tombstoned
  deleted entity is not resurrected
  **→ New tests: delete_properties_stay_deleted_on_reopen, with_live_neighbors, old_format_un_tombstoned_reconcile**
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core` — all green)
  **→ Full suite: 3986 passed, 0 failed; cargo fmt and clippy clean**

## Related
- `phase0_fix-property-store-shrink-corruption` — same rebuild scanner; its
  reliable recovery is what made this resurrection deterministic
- `phase0_fix-update-node-index-divergence` — sibling index-divergence defect
