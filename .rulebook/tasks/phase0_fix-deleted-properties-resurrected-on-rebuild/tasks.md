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
- [ ] 1.1 Failing test: create an entity with properties, delete its properties
  (`delete_properties` / node delete), flush, drop and reopen the store, assert
  the properties are GONE. Confirm it fails today (properties come back)
- [ ] 1.2 Confirm whether the resurrected entry also takes an id/offset a live
  entity needs, or only returns stale data; characterise the blast radius

## 2. Decide the tombstone encoding
- [ ] 2.1 Choose the on-disk dead-record marker (reserved `entity_type` value,
  or a zero-`data_size` dead header) that the shared scanner
  (`scan_entry_at`/`try_parse_entry`) recognises and strides over without
  re-indexing. Record the decision and the trade-off
- [ ] 2.2 Define backward compatibility: existing stores hold un-tombstoned
  deleted entities. Decide how rebuild avoids resurrecting them — reconcile
  against the authoritative node/relationship record store (skip a property blob
  whose owning record is deleted/absent), since the property store alone has no
  record of past logical deletes

## 3. Implement the fix
- [ ] 3.1 Tombstone the on-disk entry in `delete_properties` per §2.1
- [ ] 3.2 Teach the shared scanner to skip tombstoned / orphaned entries while
  still striding over them correctly (keep `rebuild_index` and
  `ensure_index_populated` identical)
- [ ] 3.3 Make the §1 tests pass; add the back-compat reconcile test (old-format
  store with an un-tombstoned deleted entity does not resurrect it)

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation: the
  tombstone/dead-record encoding and rebuild skip contract in
  `docs/specs/storage-format.md`; add a CHANGELOG entry
- [ ] 4.2 Write tests covering the new behavior: deleted properties stay deleted
  across restart (single and among live neighbours); old-format un-tombstoned
  deleted entity is not resurrected
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core` — all green)

## Related
- `phase0_fix-property-store-shrink-corruption` — same rebuild scanner; its
  reliable recovery is what made this resurrection deterministic
- `phase0_fix-update-node-index-divergence` — sibling index-divergence defect
