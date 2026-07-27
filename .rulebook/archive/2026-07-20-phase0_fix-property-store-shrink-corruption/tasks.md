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
- [x] 1.1 Write a failing integration test: create two nodes back to back
  so the second is physically laid out right after the first's property
  blob; `SET` the first node's property to a strictly shorter value; close
  and reopen; assert the second node's properties are still correct
      Done: `tests/storage/property_store_shrink_corruption_test.rs`
      `shrink_in_place_preserves_next_entity_on_reopen`. Pre-fix it dropped the
      later entity (scan break on garbage EntityType).
- [x] 1.2 Add a case with three or more entities after the shrunk one, to
  confirm the scan break drops ALL of them
      Done: `shrink_in_place_preserves_multiple_trailing_entities_on_reopen`
      (5 entities; pre-fix count collapses to 1).
- [x] 1.3 Add a case that continues writing after the corrupted reopen and
  assert it does NOT overwrite a still-live entity's blob
      Done: `continued_write_after_reopen_does_not_overwrite_live_entity`
      (asserts victim's props unchanged after a subsequent CREATE).
- [x] 1.4 Confirm the existing corruption-warning fallback is detection-only
      Confirmed by inspection: `record_store_ops.rs` `since`/`type` heuristic
      only logs the symptom; it cannot recover data or repair the scan. The
      real fix removes the mis-stride, so the fallback stays as defense-in-depth.

## 2. Decide the physical-size representation
- [x] 2.1 Choose between (a) physical-size header field and (b) never
  shrinking in place. Record the decision and the trade-off
      Chose (b) grow-only — recorded in proposal "## Decision (§2.1)". In-place
      only when `new_data_size == existing_data_size`; strict shrink/grow
      allocates fresh, so `data_size` on disk always equals physical footprint.
      No header change; reuses the existing grow allocation path.
- [x] 2.2 Define on-disk backward compatibility
      Done: the two rebuild scanners RESYNC forward past a stale tail (old-format
      shrunk-in-place entry) instead of breaking, recovering later entities.
      `next_offset` derives from the last successfully parsed entry, never a raw
      cursor. Test: `old_format_stale_tail_entry_recovers_via_resync`. Caveat
      (accepted): an entry whose header was already overwritten pre-fix is
      unrecoverable.

## 3. Implement the fix
- [x] 3.1 Apply the chosen scheme to `update_properties`'s in-place branch
      Done: in-place rewrite gated on `new_data_size == existing_data_size`
      (`property_store.rs:345`); everything else allocates fresh at `next_offset`
      via the else branch (advances `next_offset`, repoints `index`/`reverse_index`).
- [x] 3.2 Change `rebuild_index` to stride correctly
      Done: strides by `data_size` (now always == physical) via shared
      `scan_entry_at`/`try_parse_entry`; `next_offset` = end of last valid entry.
- [x] 3.3 Change `ensure_index_populated` identically
      Done: both scanners now delegate to the SAME `scan_entry_at` /
      `resync_to_next_entry` helpers — byte-for-byte identical classification and
      resync, so they cannot diverge (verified by code review).
- [x] 3.4 Make the §1 tests pass, then add a migration/compaction test
      Done: all §1 tests pass; `old_format_stale_tail_entry_recovers_via_resync`
      injects raw pre-fix stale-tail bytes and asserts lossless recovery.
- [x] 3.5 (a)-only compaction pass — N/A: option (b) was chosen. Dead old blobs
  accumulate until a future compaction pass; filed as a follow-up (out of scope,
  documented in the proposal and tests). Not required for correctness under (b).

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation:
  `docs/specs/storage-format.md` property-entry header layout and the
  rebuild-scan (grow-only + resync) contract; add a CHANGELOG entry
      Done: new "Property Store Entry Layout & Rebuild Contract" section in the
      spec; CHANGELOG [3.0.0] `### Fixed — phase0_fix-property-store-shrink-corruption`.
- [x] 4.2 Write tests covering the new behavior: shrink-then-reopen preserves
  every entity (single and multi), continued writes after reopen never
  overwrite a live entity, old-format stale-tail store rebuilds losslessly
      Done: 4 discriminating tests in
      `tests/storage/property_store_shrink_corruption_test.rs` (all pass;
      code-reviewed as genuinely fail-pre-fix).
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)
      Done (scoped per host-resource limits): full nexus-core suite green
      (0 failed); `cargo +nightly fmt --all --check` clean; `cargo clippy
      --workspace --all-targets --all-features -- -D warnings` exit 0.
      Code-reviewed: no correctness defects.

## Related
- `phase0_fix-update-node-index-divergence`,
  `phase0_fix-delete-node-dangling-relationships` — other write-path/
  index-corruption defects from the same audit
