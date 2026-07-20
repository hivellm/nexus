# Proposal: phase0_fix-anonymous-node-lost-on-restart

**Priority: CRITICAL — a committed node is silently lost on the next restart, and
its id is reused, whenever the node has no labels, no properties, and no
relationships.** Found during a durability/crash-recovery audit; not previously
reported.

## Why

A live `NodeRecord` has no explicit "in-use"/allocated bit. `mark_deleted` sets
`flags |= 1`, so a live node has `flags == 0`, and `NodeRecord::new()` is
`Default` — all 32 bytes zero
(`crates/nexus-core/src/storage/records.rs:25-42,67-75`). A node created with no
labels, no properties, and no relationships therefore persists as a **byte-for-byte
all-zero record** (`label_bits=0`, `first_rel_ptr=0`, `prop_ptr=0`, `flags=0`).

On startup the record store reconstructs `next_node_id` by scanning the mmap and
advancing past any slot where **any byte is non-zero**
(`crates/nexus-core/src/storage/record_store.rs:123-131`):

```rust
if slice.iter().any(|&b| b != 0) {
    next_node_id = (i + 1) as u64;
}
```

An all-zero live record is indistinguishable from an unallocated slot. There is no
WAL-into-store replay — the mmap store is the sole source of truth — so the scan is
the only thing that reconstructs the id high-water mark.

### Consequences (confirmed by code inspection)

- Any trailing anonymous node is treated as free space after a **clean restart** —
  no crash required. `node_count()` (= `next_node_id`,
  `record_store.rs:314-315`) drops it; `get_node(id)` returns `None`
  (`id >= next_node_id`, `record_store_ops.rs:916`); the next
  `allocate_node_id()` reuses its id and overwrites the slot.
- If **every** node in the store is anonymous, `next_node_id` resets to 0 on
  restart and all of them vanish.
- Relationship records are immune: `RelationshipRecord::new` seeds
  `next_src_ptr`/`next_dst_ptr`/`prop_ptr` to `u64::MAX`
  (`records.rs:121-123`), so a rel record is never all-zero (except a degenerate
  self-loop of id 0 / type 0 / no properties).

This is silent, permanent data loss plus id reuse — the graph loses committed nodes
and can later hand the same id to a different node, corrupting any external
reference to the lost id.

## What Changes

- Give live records an explicit **in-use / allocated bit** set on every write
  (e.g. `flags` bit 1 = allocated, keeping bit 0 = deleted), and change the
  recovery scan **and** `get_node`/`is_deleted` to test that bit instead of
  "any non-zero byte". A live anonymous node then has a non-zero `flags` and is no
  longer mistaken for free space.
- **Or** persist `next_node_id`/`next_rel_id` in a durable store header rather than
  reconstructing them by mmap scan, so the high-water mark survives regardless of
  record contents.
- Whichever is chosen must remain compatible with existing on-disk stores (a slot
  written by the current code has `flags == 0` for a live node) — migration or a
  scan that recognises both encodings is required, not a silent format bump.

## Impact

- Affected specs: `docs/specs/storage-format.md` (node record layout, flags
  semantics, recovery contract)
- Affected code: `crates/nexus-core/src/storage/records.rs` (flags/in-use bit),
  `crates/nexus-core/src/storage/record_store.rs` (recovery scan `:123-141`),
  `crates/nexus-core/src/storage/record_store_ops.rs` (node create `:560-564`,
  `get_node` `:916`)
- Breaking change: NO for query semantics; on-disk format changes and needs a
  compatible migration path
- User benefit: `CREATE ()` and any labelless/propertyless node survives restart;
  node ids are never silently reused
- Related: `phase0_fix-cypher-oom-process-abort`,
  `phase14_fix-external-id-write-path` (the other write-path/durability tasks)
