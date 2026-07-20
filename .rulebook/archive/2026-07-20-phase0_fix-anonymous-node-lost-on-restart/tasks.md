# Tasks: phase0_fix-anonymous-node-lost-on-restart

A node with no labels, no properties and no relationships persists as an all-zero
32-byte `NodeRecord`. The restart scan advances `next_node_id` only past slots with
any non-zero byte (`record_store.rs:123-131`), so an anonymous node is
indistinguishable from a free slot: it is dropped on the next clean restart and its
id is reused. There is no WAL replay into the store — the mmap is the sole source of
truth — so the scan is the only high-water-mark reconstruction.

Order matters: prove the loss with a failing test (§1) before changing the record
format, and settle the on-disk format decision (§2) before touching the write and
recovery paths (§3), because the same encoding must be produced by writes and
recognised by recovery.

## 1. Reproduce the loss first
- [x] 1.1 Write a failing integration test: open a store, `CREATE (:Foo)` then a
  second anonymous node (no labels/props/rels), flush, drop and reopen the store,
  and assert `node_count() == 2` and that the anonymous node is still readable.
  Confirm it fails today (count is 1, node gone)
      Done: `tests/storage/anonymous_node_survives_restart_test.rs`
      `anonymous_trailing_node_survives_restart` — failed pre-fix (count 1), passes now.
- [x] 1.2 Add the extreme case: a store whose only node is anonymous — assert it
  survives reopen. Confirm `next_node_id` resets to 0 today and the node vanishes
      Done: `store_with_only_an_anonymous_node_survives_restart`.
- [x] 1.3 Record exactly which byte pattern the create path writes for the
  anonymous node (`record_store_ops.rs:560-564` → `NodeRecord::new()`), confirming
  it is all-zero, so the fix target is unambiguous
      Confirmed: `NodeRecord::new()` = all-zero (label_bits/first_rel_ptr/prop_ptr/flags=0),
      byte-for-byte indistinguishable from an unallocated slot.

## 2. Decide and document the on-disk format
- [x] 2.1 Choose between (a) an explicit in-use/allocated flag bit on `NodeRecord`
  and (b) a persisted durable store header holding `next_node_id`/`next_rel_id`.
  Record the decision and why in the proposal; (a) is local to the record, (b)
  removes the scan entirely — state the trade-off
      Chose (a) — `FLAG_ALLOCATED = 0b10`. Recorded in proposal "## Decision (§2.1)":
      the bit rides the existing record write (no new durable structure to make
      crash-safe, unlike (b)'s header).
- [x] 2.2 Define backward compatibility: existing stores have live nodes with
  `flags == 0`. The chosen scheme MUST read those correctly (a one-time migration
  that stamps the in-use bit on reopen, or a header written lazily on first write).
  A silent format bump that mis-reads old stores is not acceptable
      Done: recovery scan treats a slot as in-use if `is_allocated()` OR (legacy)
      any byte non-zero — INDEPENDENT of the deleted bit, so a legacy soft-deleted
      record still reserves its id (id reservation != query visibility). One-time
      migration stamps the allocated bit on every legacy non-zero slot on reopen,
      preserving the deleted bit. Caveat (documented): a pre-fix all-zero anonymous
      node is unrecoverable. Regression test:
      `legacy_soft_deleted_node_slot_is_not_reused_after_restart`.
- [x] 2.3 Confirm the relationship store has or needs the same treatment — rel
  records are currently immune via the `u64::MAX` sentinels
  (`records.rs:121-123`), but the degenerate all-zero self-loop (id 0, type 0, no
  props) has the identical defect; decide whether to fix it under the same scheme
      Fixed under the same scheme: `RelationshipRecord` gets `FLAG_ALLOCATED` on
      every `write_rel`; the rel recovery scan uses the same dual predicate. Test:
      `degenerate_self_loop_relationship_survives_restart`.

## 3. Implement the fix
- [x] 3.1 Apply the chosen scheme to the write path so every live node (and, per
  §2.3, relationship) is distinguishable from an unallocated slot
      Done: `write_node`/`write_rel` (`record_store_ops.rs`) OR `FLAG_ALLOCATED` into
      a local copy before writing (caller's record untouched).
- [x] 3.2 Change the recovery scan (`record_store.rs:123-141`) to reconstruct
  `next_node_id`/`next_rel_id` from the in-use bit (or the header), not from
  "any non-zero byte"
      Done: `is_allocated() || (any byte non-zero)` for both node and rel scans.
- [x] 3.3 Update `get_node`/`is_deleted` (`record_store_ops.rs:916`,
  `records.rs:73-75`) and `node_count()` (`record_store.rs:314-315`) to agree with
  the new liveness definition, so a live anonymous node is never reported deleted
  or out of range
      No change needed and verified so (code review): `get_node` gates on
      `id < next_node_id` + `is_deleted()`; `node_count()` derives from `next_node_id`
      (now correctly reconstructed); a new anonymous node is non-zero (`flags=0b10`),
      below `next_node_id`, not deleted → returned. `is_deleted` keeps testing bit 0.
- [x] 3.4 Make the §1 tests pass, then add a migration test: a store written by the
  old format (live node with `flags == 0`) reopens with all nodes intact
      Done: `old_format_store_with_labelled_node_migrates_losslessly` (injects raw
      `flags==0` bytes, reopens, asserts intact) plus the soft-deleted regression.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation:
  `docs/specs/storage-format.md` with the flags/in-use semantics or
  the header layout, and the recovery contract; add a CHANGELOG entry
      Done: `docs/specs/storage-format.md` flags/allocated-bit + recovery contract
      section; CHANGELOG [3.0.0] `### Fixed — phase0_fix-anonymous-node-lost-on-restart`.
- [x] 4.2 Tests: anonymous node survives restart (single and all-anonymous store),
  id is not reused after restart, old-format store migrates losslessly
      Done: 6 tests in `tests/storage/anonymous_node_survives_restart_test.rs`
      (trailing, all-anonymous, id-not-reused, old-format migration, degenerate
      self-loop rel, legacy-soft-deleted id-reservation). All pass.
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)
      Done (scoped, per host-resource limits): full nexus-core suite green (0 failed);
      `cargo +nightly fmt --all --check` clean; `cargo clippy --workspace
      --all-targets --all-features -- -D warnings` exit 0. Code-reviewed; a BLOCKER
      (legacy-deleted id-reuse) was caught and fixed before commit.

## Related
- `phase0_fix-cypher-oom-process-abort`, `phase14_fix-external-id-write-path` —
  other write-path/durability defects in the same stores
