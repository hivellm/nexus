# Proposal: phase0_fix-relationship-publish-ordering

**Priority: HIGH — a concurrent lock-free read can see a truncated or phantom
relationship adjacency list while a CREATE/MERGE is still in progress.** Found
during a concurrency/MVCC audit; not previously reported.

## Why

There is no record-level MVCC versioning in Nexus. The lock-free read path
(`crates/nexus-server/src/api/cypher/execute/handler.rs:514-599`) clones
`engine.executor` under a brief `engine.read()`, releases that lock, and then
runs the query via `spawn_blocking` while holding **no** engine lock — a
concurrent writer may be mid-way through `engine.write()` at that exact moment.
Because there is no per-record version to fall back on, the *order* in which a
write operation publishes its individual record writes is the entire isolation
contract. For relationship insert, that order is wrong.

`crates/nexus-core/src/storage/record_store_ops.rs::create_relationship`
performs the writes in this sequence:

- `:609` `let rel_id = self.allocate_rel_id();` — the slot for the new
  relationship is reserved but its backing record is still whatever was on
  disk before (zeroed for a fresh slot).
- `:731` `source_node.first_rel_ptr = rel_id + 1;` — the in-memory copy of the
  source node is updated to point at the new relationship.
- `:836` `self.write_node(from, &source_node)?;` — **this publishes
  `first_rel_ptr` into the shared `nodes_mmap`**, making the new relationship
  reachable from `from`.
- `:877` `self.write_rel(rel_id, &record)?;` — only *here* is the relationship
  record itself (src/dst/type/next pointers) written to the shared
  `rels_mmap`.

Between `:836` and `:877`, the new node pointer is live but the relationship
slot it points to is not yet initialized.

### The interleaving (confirmed by code inspection)

Writer **W** (holding `engine.write()`) executes `write_node(from, ..)` at
`record_store_ops.rs:836`, committing `from.first_rel_ptr = rel_id + 1` into
the shared `nodes_mmap`, then releases the mmap write guard. Reader **R**
(lock-free, holding no engine lock), expanding `from` inside
`crates/nexus-core/src/executor/operators/path.rs::find_relationships`, reads
`node_record.first_rel_ptr` (`path.rs:228`) and gets `rel_id + 1`, decodes
`current_rel_id = rel_ptr.saturating_sub(1)` (`path.rs:559`), and calls
`store.read_rel(current_rel_id)` (`path.rs:569`) against the **same shared**
`rels_mmap`. W has not yet reached `:877`, so slot `rel_id` is still all-zero:
`src_id=0, dst_id=0, type_id=0, next_src_ptr=0, flags=0`
(`RelationshipRecord::is_deleted()` is false for this pattern). R therefore
treats the slot as a live edge and either:

- (a) surfaces a phantom edge to node 0, or
- (b) — since `next_src_ptr == 0` is also the end-of-chain sentinel —
  terminates the linked-list walk immediately, silently dropping every
  *older* relationship in `from`'s adjacency list.

`Engine::find_relationship_between` (`crates/nexus-core/src/engine/write_exec.rs:1178-1184`)
hits its `else { break }` branch on the zeroed slot and reports "edge not
found" for an edge that genuinely exists.

### Trigger

Any `MATCH (a)-[r]->(b)` / edge-count / traversal read issued concurrently
with a `CREATE`/`MERGE` that adds an outgoing edge to `a`. The size of the
loss window scales with `a`'s out-degree: a high-out-degree node has a wider
window during which readers walking its list observe the zeroed slot before
truncating.

## What Changes

Reorder the writes inside `create_relationship` to match what a lock-free
linked-list prepend requires: the new node must be **fully initialized before
it is linked in**, and the pointer publish must be the last, externally
visible step.

- Allocate `rel_id`, then build the complete `RelationshipRecord` — including
  `next_src_ptr = old_first_rel_ptr` (source) and `next_dst_ptr =
  old_first_rel_ptr` (destination) — using the *old* head values captured
  before any node write.
- Write the fully-populated relationship record to `rels_mmap` **first**.
- Only then update and publish `first_rel_ptr` on the source (and, where
  applicable, destination) node record via `write_node`.
- Insert a `Release` fence between the relationship-record write and the
  node-pointer publish (mirroring the existing `Acquire`/`Release` fences
  already present in `create_relationship`, currently placed around the wrong
  operations) so a reader that observes the new head is guaranteed — under
  the platform's memory model — to also observe the fully-initialized
  relationship record it points to.

With this ordering, a reader that has not yet observed the new
`first_rel_ptr` sees the prior, still-consistent adjacency list; a reader that
has observed the new `first_rel_ptr` always finds a fully-initialized record
whose `next_*_ptr` correctly chains to the prior list head. There is no
window in which the published pointer target is uninitialized.

A secondary, non-blocking observation surfaced by the same audit: `write_lock`
and the epoch-visibility field in `crates/nexus-core/src/transaction/mod.rs:188,251`
are unused — single-writer exclusion rests entirely on the outer
`RwLock<Engine>`, and this dead surface should eventually be deleted or wired
up. It does not affect correctness of this fix and is out of scope here.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md` (lock-free read isolation contract
  for relationship insert)
- Affected code: `crates/nexus-core/src/storage/record_store_ops.rs`
  (`create_relationship`, write ordering `:609-891`),
  `crates/nexus-core/src/executor/operators/path.rs` (the reader path this
  protects, `find_relationships` `:224-569`)
- Breaking change: NO — on-disk record layout is unchanged; only the order of
  writes within a single relationship-creation call changes
- User benefit: concurrent lock-free reads (`MATCH`, traversal, edge lookup)
  running alongside a `CREATE`/`MERGE` that adds an edge no longer observe a
  phantom edge to node 0 or a silently truncated adjacency list
- Related: `phase0_fix-store-size-per-clone-divergence` (another lock-free
  reader/writer divergence in the same record stores),
  `phase0_fix-delete-node-dangling-relationships` (a different relationship
  linked-list integrity defect in the same storage layer)
