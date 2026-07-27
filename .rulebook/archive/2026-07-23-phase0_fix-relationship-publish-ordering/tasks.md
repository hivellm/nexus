# Tasks: phase0_fix-relationship-publish-ordering

`create_relationship` publishes the new edge's `first_rel_ptr` on the source
node (`record_store_ops.rs:836`) *before* writing the relationship record
itself (`record_store_ops.rs:877`). Nexus has no record-level MVCC, so a
lock-free read (`server/.../cypher/execute/handler.rs:514-599`, which runs a
query under `spawn_blocking` while holding no engine lock) issued
concurrently with a `CREATE`/`MERGE` that adds an edge can observe the
published pointer, `read_rel` the still-zeroed slot it points to, and either
see a phantom edge to node 0 or terminate its linked-list walk on the
`next_src_ptr == 0` sentinel — silently dropping every older relationship in
that node's adjacency list.

Order matters here in two senses: prove the race exists (§1) before touching
anything, confirm exactly which interleaving causes it (§2) so the fix targets
the right instant, then fix the write order and the memory ordering together
(§3) — publishing the pointer correctly but without the matching fence (or
vice versa) leaves the same race reachable on architectures with weaker memory
models.

## 1. Reproduce the race first
- [x] 1.1 Concurrent-read stress test written
  (`tests/storage/relationship_publish_ordering_test.rs::concurrent_reads_never_observe_truncated_or_phantom_adjacency_during_create`):
  node `a` (id 0) seeded with 100 outgoing edges, 6 lock-free reader threads
  each holding an `engine.storage.clone()` walk `a`'s chain while the writer
  appends ~4000 more edges; asserts no `dst_id == 0` phantom and no count below
  the seeded floor
- [x] 1.2 Ran against current code and confirmed it fails: **100% (11/11 runs)**
  reproduced a phantom edge — all 6 readers observed `dst_id=0, src_id=0,
  next_src_ptr=0` (an all-zero slot) at the chain head, in <1s per run
- [x] 1.3 Deterministic variant
  (`::zeroed_relationship_slot_reads_as_live_edge_to_node_zero`):
  `allocate_rel_id()` then `read_rel` the unwritten slot, asserting
  `is_deleted() == false`, `src_id == 0`, `dst_id == 0`, `next_src_ptr == 0`

## 2. Confirm the interleaving
- [x] 2.1 Confirmed the write sequence: `allocate_rel_id`,
  `source_node.first_rel_ptr = rel_id + 1`, `write_node(from)` (publish),
  then `write_rel(rel_id)` (record). The node/rel mmaps are separate
  `Arc<RwLock<MmapMut>>`, so each record read/write is atomic but the
  cross-mmap pair is not — the mispositioned `Release` fence (after the node
  write) did not prevent a reader seeing the published pointer before the
  record write
- [x] 2.2 Confirmed the reader: `path.rs::find_relationships` reads
  `first_rel_ptr`, derives `rel_id = ptr - 1`, `read_rel`s it; `is_deleted()`
  is `false` on the all-zero slot and `next_src_ptr == 0` doubles as the
  end-of-chain sentinel, so the walk truncates instead of erroring
- [x] 2.3 `Engine::find_relationship_between` hits its `else { break }` on the
  zeroed slot — a second observable symptom of the same root cause

## 3. Fix the write ordering and memory ordering
- [x] 3.1 Build the complete `RelationshipRecord`
  (`next_src_ptr`/`next_dst_ptr` = captured old heads) and `write_rel` it first
- [x] 3.2 Moved the source-node `first_rel_ptr` publish (`write_node`) to after
  `write_rel` — the last, externally-visible step
- [x] 3.3 `fence(Release)` between the record write and the pointer publish;
  added a matching `fence(Acquire)` before the reader's `read_node` in
  `path.rs::find_relationships`
- [x] 3.4 Re-ran: concurrent stress test 8/8 pass (was 11/11 fail); the
  deterministic slot test still passes; storage group 43/0

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation —
  `docs/specs/wal-mvcc.md` gained the record-before-pointer publish-ordering
  contract; CHANGELOG entry added
- [x] 4.2 Write tests covering the new behavior — the concurrent stress test
  and the deterministic slot-hazard test kept in
  `tests/storage/relationship_publish_ordering_test.rs`
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` green;
  full `cargo +nightly test --workspace` run to confirm

## Related
- `phase0_fix-store-size-per-clone-divergence` — a different lock-free
  reader/writer divergence in the same record stores (stale size snapshot vs.
  shared mmap)
- `phase0_fix-delete-node-dangling-relationships` — a different relationship
  linked-list integrity defect in the same storage layer
