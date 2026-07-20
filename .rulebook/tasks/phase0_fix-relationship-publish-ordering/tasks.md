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
- [ ] 1.1 Write a concurrent-read stress test: create a node `a` with N
  pre-existing outgoing relationships (N >= 50, to widen the window), then
  spawn one thread issuing `CREATE (a)-[:R]->(x)` in a loop while several
  reader threads/tasks concurrently run `MATCH (a)-[r]->(b) RETURN count(r)`
  (or walk `find_relationships` directly) against the same engine, mimicking
  the lock-free read path's no-engine-lock execution. Assert every read
  either sees the pre-write count or the post-write count — never a count
  less than the pre-write count (truncated list) and never an edge whose
  target node id is `0` (phantom edge)
- [ ] 1.2 Run it against the current code and confirm it fails: capture at
  least one observed truncated-count or phantom-target-0 read, and record the
  reproduction rate (races are timing-dependent — note how many iterations it
  took) so the fix can be checked against the same harness
- [ ] 1.3 Add a narrower, deterministic variant that does not depend on
  thread timing: manually reorder the two writes in a test-only harness (or
  use a sync point / channel to pause the writer between `record_store_ops.rs:836`
  and `:877`) and have a reader call `read_rel` on the new slot in that
  window, asserting today it returns a live-looking all-zero record
  (`is_deleted() == false`, `src_id == 0`, `dst_id == 0`, `next_src_ptr == 0`)

## 2. Confirm the interleaving
- [ ] 2.1 Trace and record the exact write sequence in
  `create_relationship` (`record_store_ops.rs:601-891`): `allocate_rel_id`
  at `:609`, `source_node.first_rel_ptr = rel_id + 1` at `:731`,
  `write_node(from, ..)` at `:836`, `write_rel(rel_id, ..)` at `:877`. Confirm
  in the proposal/task notes that no fence or lock currently prevents a reader
  from observing the `:836` write before the `:877` write completes
  (the existing `Acquire`/`Release` fences at `:640` and `:850` are
  positioned around the wrong pair of operations — verify and note exactly
  what they currently protect)
- [ ] 2.2 Trace the reader side that hits the zeroed slot:
  `path.rs::find_relationships` reads `node_record.first_rel_ptr` at `:228`,
  derives `current_rel_id` at `:559`, and calls `store.read_rel(current_rel_id)`
  at `:569` against the same shared `rels_mmap`. Confirm `RelationshipRecord::is_deleted()`
  returns `false` for the all-zero pattern (so the reader does not reject the
  slot as deleted) and that `next_src_ptr == 0` is the same sentinel value
  used for end-of-chain, causing the walk to stop instead of erroring
- [ ] 2.3 Confirm `Engine::find_relationship_between`
  (`crates/nexus-core/src/engine/write_exec.rs:1178-1184`) hits its
  `else { break }` branch on the zeroed slot and returns "not found" for an
  edge that was in fact created, giving a second, independently-observable
  symptom of the same root cause

## 3. Fix the write ordering and memory ordering
- [ ] 3.1 In `create_relationship`, build the complete `RelationshipRecord`
  (including `next_src_ptr`/`next_dst_ptr` set to the previously-captured old
  head pointers) before any node record is written, and write it to
  `rels_mmap` via `write_rel` first
- [ ] 3.2 Move the source (and, where applicable, destination) node's
  `first_rel_ptr` update — the `write_node` call currently at `:836` — to
  after the `write_rel` call, so the pointer publish is the last step
- [ ] 3.3 Insert a `Release` fence (`std::sync::atomic::fence(Ordering::Release)`)
  immediately after the relationship-record write and before the node-pointer
  publish; confirm the reader path's node read (`path.rs:227`,
  `store.read_node(node_id)`) is preceded by an `Acquire` (add one if it is
  missing) so the two fences form a real happens-before edge, not just
  program-order-on-one-thread
- [ ] 3.4 Re-run the §1.1 stress test and the §1.3 deterministic variant;
  both must pass — no truncated count, no phantom edge to node 0, and the
  deterministic harness's paused-writer window must now show a read of the
  new relationship slot returning a fully-initialized record (not zeroed) at
  every point after the record write, before or after the pointer publish

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/wal-mvcc.md` with the write-ordering contract for
  `create_relationship` (record-before-pointer, with the fence) so future
  writers to this function do not reintroduce the race; add a CHANGELOG entry
- [ ] 4.2 Keep the §1.1 concurrent stress test and §1.3 deterministic test in
  the regression suite (e.g. `crates/nexus-core/tests/`), named so their
  intent — protecting the publish ordering — is obvious from the test name
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-store-size-per-clone-divergence` — a different lock-free
  reader/writer divergence in the same record stores (stale size snapshot vs.
  shared mmap)
- `phase0_fix-delete-node-dangling-relationships` — a different relationship
  linked-list integrity defect in the same storage layer
