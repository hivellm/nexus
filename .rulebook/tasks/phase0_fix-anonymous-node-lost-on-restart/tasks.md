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
- [ ] 1.1 Write a failing integration test: open a store, `CREATE (:Foo)` then a
  second anonymous node (no labels/props/rels), flush, drop and reopen the store,
  and assert `node_count() == 2` and that the anonymous node is still readable.
  Confirm it fails today (count is 1, node gone)
- [ ] 1.2 Add the extreme case: a store whose only node is anonymous — assert it
  survives reopen. Confirm `next_node_id` resets to 0 today and the node vanishes
- [ ] 1.3 Record exactly which byte pattern the create path writes for the
  anonymous node (`record_store_ops.rs:560-564` → `NodeRecord::new()`), confirming
  it is all-zero, so the fix target is unambiguous

## 2. Decide and document the on-disk format
- [ ] 2.1 Choose between (a) an explicit in-use/allocated flag bit on `NodeRecord`
  and (b) a persisted durable store header holding `next_node_id`/`next_rel_id`.
  Record the decision and why in the proposal; (a) is local to the record, (b)
  removes the scan entirely — state the trade-off
- [ ] 2.2 Define backward compatibility: existing stores have live nodes with
  `flags == 0`. The chosen scheme MUST read those correctly (a one-time migration
  that stamps the in-use bit on reopen, or a header written lazily on first write).
  A silent format bump that mis-reads old stores is not acceptable
- [ ] 2.3 Confirm the relationship store has or needs the same treatment — rel
  records are currently immune via the `u64::MAX` sentinels
  (`records.rs:121-123`), but the degenerate all-zero self-loop (id 0, type 0, no
  props) has the identical defect; decide whether to fix it under the same scheme

## 3. Implement the fix
- [ ] 3.1 Apply the chosen scheme to the write path so every live node (and, per
  §2.3, relationship) is distinguishable from an unallocated slot
- [ ] 3.2 Change the recovery scan (`record_store.rs:123-141`) to reconstruct
  `next_node_id`/`next_rel_id` from the in-use bit (or the header), not from
  "any non-zero byte"
- [ ] 3.3 Update `get_node`/`is_deleted` (`record_store_ops.rs:916`,
  `records.rs:73-75`) and `node_count()` (`record_store.rs:314-315`) to agree with
  the new liveness definition, so a live anonymous node is never reported deleted
  or out of range
- [ ] 3.4 Make the §1 tests pass, then add a migration test: a store written by the
  old format (live node with `flags == 0`) reopens with all nodes intact

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/storage-format.md` with the flags/in-use semantics or
  the header layout, and the recovery contract; add a CHANGELOG entry
- [ ] 4.2 Tests: anonymous node survives restart (single and all-anonymous store),
  id is not reused after restart, old-format store migrates losslessly
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-cypher-oom-process-abort`, `phase14_fix-external-id-write-path` —
  other write-path/durability defects in the same stores
