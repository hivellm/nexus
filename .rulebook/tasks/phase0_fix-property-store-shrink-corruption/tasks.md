# Tasks: phase0_fix-property-store-shrink-corruption

`PropertyStore::update_properties`'s in-place shrink branch
(`storage/property_store.rs:307-312`) overwrites `data_size` and the
leading bytes but never zeroes or reclaims the freed tail of the old,
longer payload. On reopen, `rebuild_index` (`:539-599`) and
`ensure_index_populated` (`:723-756`) both stride by the (now smaller)
stored `data_size`, so they land inside the stale tail instead of at the
next entity's true header, hit a garbage `EntityType` byte, and break
early — dropping every later entity from `reverse_index` and leaving
`next_offset` at a mid-file position that the next write then overwrites.

Trigger:
```
CREATE (a:Person {name:'Alice Alice Alice'})   -- long value, node b created right after it on disk
SET a.name = 'Al'                              -- in-place shrink; stale tail bytes left on disk
-- restart / reopen the store --
CREATE (c:Person {name:'Carol'})               -- allocates from the now-wrong next_offset
MATCH (b) WHERE id(b) = <b's id> RETURN b      -- b's properties are now garbage / overwritten
```

Order matters: reproduce the corruption end-to-end across a real reopen
(§1) before touching the format, because the bug is invisible on a live
(never-reopened) store — `self.index`/`self.reverse_index` stay correct in
memory across the shrink itself; only the rebuild scan after reopen is
wrong. Decide the on-disk physical-size representation (§2) before
changing the write path and the two scanners together (§3), because the
write path must persist whatever the scanners will trust, and both
scanners must agree with each other and with the write path or the same
divergence reappears asymmetrically.

## 1. Reproduce the corruption first
- [ ] 1.1 Write a failing integration test: create two nodes back to back
  so the second is physically laid out right after the first's property
  blob; `SET` the first node's property to a strictly shorter value
  (in-place shrink branch, `new_data_size <= existing_data_size` at
  `property_store.rs:308`); close and reopen the store; assert the second
  node's properties are still correct. Confirm it fails today (garbage,
  truncated, or missing properties)
- [ ] 1.2 Add a case with three or more entities after the shrunk one, to
  confirm the scan break drops ALL of them, not just the immediate
  neighbor — assert `node_count()`/readability for each. Confirm it fails
  today
- [ ] 1.3 Add a case that continues writing after the corrupted reopen (a
  `CREATE`/`SET` that allocates from the wrong `next_offset`) and assert it
  does NOT silently overwrite a still-live entity's blob. Confirm it fails
  today (cross-entity overwrite)
- [ ] 1.4 Confirm the existing corruption-warning fallback
  (`record_store_ops.rs:1184-1258`, the `since`/`type` key heuristic) fires
  on the corrupted read in 1.1-1.3 but does not recover correct data —
  record this as evidence the fallback is detection-only, not a fix

## 2. Decide the physical-size representation
- [ ] 2.1 Choose between (a) an explicit "physical/allocated size" header
  field, separate from the payload `data_size`, that only ever grows on an
  in-place update, and (b) never shrinking in place at all — every update
  to a smaller size still allocates fresh space at `next_offset` like the
  grow branch does today. Record the decision and the trade-off (a saves
  space reclaimed by a future compaction pass; b is simpler and matches the
  grow branch's existing code path)
- [ ] 2.2 Define on-disk backward compatibility: existing stores have
  entries whose header has no physical-size field (if (a) is chosen) or may
  already contain shrunk-in-place entries with stale tails (either
  choice). The chosen scheme MUST rebuild correctly against such stores —
  a migration/compaction pass on first open of an old-format store, not a
  scan that silently mis-strides on old data written by the previous code

## 3. Implement the fix
- [ ] 3.1 Apply the chosen scheme to `update_properties`'s in-place branch
  (`property_store.rs:280-334`): per §2.1, either zero the freed tail bytes
  (`offset+13+new_data_size` .. `offset+13+existing_data_size`) and persist
  the physical size, or stop shrinking in place and always allocate fresh
  space via the existing grow branch
- [ ] 3.2 Change `rebuild_index` (`:539-599`) to stride by the persisted
  physical size (or, if 2.1 chose (b), confirm `data_size` alone is now
  always correct because no in-place shrink ever occurs)
- [ ] 3.3 Change `ensure_index_populated` (`:723-756`) identically, so the
  two scanners cannot diverge from each other
- [ ] 3.4 Make the §1 tests pass, then add a migration/compaction test: a
  store written by the old code (containing an unzeroed stale tail from an
  in-place shrink) reopens with all entities intact under the new scanner
- [ ] 3.5 If (a) was chosen in §2.1, add a lazy or startup compaction pass
  that reclaims the zeroed freed tails, so shrink-heavy workloads don't
  grow the property store unboundedly forever

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/storage-format.md` with the property-entry
  header layout (physical size vs payload `data_size`) and the
  rebuild-scan contract; add a CHANGELOG entry
- [ ] 4.2 Tests: shrink-then-reopen preserves every entity (single and
  multi-entity-after cases), continued writes after reopen never overwrite
  a live entity, old-format store with a stale-tail entry migrates/rebuilds
  losslessly
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-update-node-index-divergence`,
  `phase0_fix-delete-node-dangling-relationships` — other write-path/
  index-corruption defects from the same audit
