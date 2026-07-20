# Tasks: phase0_fix-fts-async-writer-ordering

`fts_refresh_node` enqueues `Del{node}` then `Add{node, newContent}` on every SET/REMOVE of an
indexed node's fulltext-indexed properties (`engine/crud/index_maintenance.rs:43-70`), but
`apply_batch` (`index/fulltext_writer.rs:280-293`) applies all adds in a batch before all dels —
inverting the intended order and leaving the node's own fresh content deleted while the registry's
`members` bookkeeping still claims it is indexed. This only fires when `enable_async_writers` is
on (`index/mod.rs:72-77`), but it is a shipped path, not experimental-only code.

Order matters: reproduce the visibility loss and the registry/Tantivy divergence first (§1) so the
fix (§2) has a concrete regression target, then verify the fix does not regress cross-id batching
throughput (§3), since the whole point of batching is to coalesce unrelated ids efficiently.

## 1. Reproduce the ordering bug first
- [ ] 1.1 Write a failing integration test with `enable_async_writers` on: create a fulltext-
  indexed node, run a `SET` that changes the indexed property, flush/await the async writer, then
  run a fulltext search that should match the new content. Confirm it returns 0 hits today even
  though the node is live and its content matches
- [ ] 1.2 In the same test, inspect the registry's `indexes_containing`/`members` state for the
  node and confirm it reports the node as indexed — proving the divergence is between the
  registry's bookkeeping and the actual Tantivy index, not a simple "never indexed" bug
- [ ] 1.3 Confirm via code inspection that `apply_batch` (`fulltext_writer.rs:280-293`) applies
  `adds.for_each(apply_add)` before `dels.for_each(apply_del)` unconditionally, so a same-id
  Add-then-Del pair within one batch always nets to "deleted" regardless of enqueue order

## 2. Fix: preserve per-node del→add ordering in apply_batch
- [ ] 2.1 Change `apply_batch` to either (a) apply queued commands strictly in arrival order, or
  (b) coalesce a `Del{id}` immediately followed by an `Add{id, content}` for the same id into one
  replace operation before applying the batch. Record the choice and why
- [ ] 2.2 Make the §1.1 test pass: the SET'd node is visible to fulltext search after the async
  writer flushes, and the §1.2 registry/Tantivy divergence no longer occurs
- [ ] 2.3 Add a REMOVE case (property removed, not just changed) confirming the node correctly
  drops out of the fulltext index when the update removes its only indexed content

## 3. Verify cross-id batching is unaffected
- [ ] 3.1 Write a test that enqueues adds/dels for multiple distinct node ids in one batch
  (including at least one unrelated Add-only and one unrelated Del-only entry alongside a same-id
  Del+Add pair) and confirms all entries resolve correctly — the fix must not serialize or slow
  down unrelated-id batching

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Document the async-writer batching ordering contract (per-id ordering preserved,
  cross-id batching unaffected) near `apply_batch` or in a fulltext-index note; add a CHANGELOG
  entry
- [ ] 4.2 Tests: SET on indexed node keeps it fulltext-visible under async writers (§1/§2
  regression), REMOVE correctly evicts (§2.3), cross-id batch correctness (§3.1)
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-update-node-index-divergence` — a different index-maintenance gap
  (`update_node` skips the refresh suite entirely) reached through the same
  `engine/crud/index_maintenance.rs` refresh helpers
