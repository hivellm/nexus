# Proposal: phase0_fix-storage-oob-panics

**Priority: HIGH — three independent record/property-store bounds defects
each let corrupt-or-crafted on-disk offsets slip past their length guard and
panic on an out-of-bounds slice, aborting the query (or the calling thread)
instead of returning a storage error.** Found during a storage-layer audit;
not previously reported. All three are reachable from ordinary graph
traversal over data that is merely corrupt or adversarially crafted, not
from any Rust-level API misuse.

## Why

All three defects are amplified by the workspace release profile
(`Cargo.toml:121-125`) not enabling `overflow-checks`: `[profile.release]`
sets `opt-level`/`lto`/`codegen-units`/`strip` but no `overflow-checks` key,
so Cargo's default (`false`) applies in release builds — the `usize`/`u64`
arithmetic below wraps silently instead of panicking on overflow, which is
exactly the property defect #2 exploits.

### #2 — record-store offset arithmetic is not overflow-safe

`read_node` (`crates/nexus-core/src/storage/record_store_ops.rs:180-191`),
`write_node` (`:69-80`), `read_rel` (`:286-298`), `write_rel` (`:264-275`)
all compute the byte offset the same unchecked way, e.g. `read_node`:

```rust
let offset = (node_id as usize * NODE_RECORD_SIZE) as u64;

if offset + NODE_RECORD_SIZE as u64 > self.nodes_file_size as u64 {
    return Err(Error::NotFound(format!("Node {} not found", node_id)));
}

let start = offset as usize;
let end = start + NODE_RECORD_SIZE;
let mut record: NodeRecord = {
    let guard = self.nodes_mmap.read().unwrap();
    *bytemuck::from_bytes(&guard[start..end])
};
```

With `overflow-checks` off, `node_id as usize * NODE_RECORD_SIZE` (32 bytes)
wraps modulo 2^64 in release. Choosing `node_id` so that
`node_id * 32 mod 2^64` lands in `[2^64-32, 2^64-1]` (the multiple of 32
nearest the top is `2^64-32`) makes `offset = 0xFFFF_FFFF_FFFF_FFE0`. The
guard then computes `offset + 32`, which **also wraps**, to `0` — trivially
`<= self.nodes_file_size` — so the bounds check **passes**, and
`guard[start..end]` (`start` ≈ `usize::MAX-31`) slices past the mmap and
panics. `read_rel`/`write_rel` have the identical shape with `* 52`
(`REL_RECORD_SIZE`). `write_node`/`write_rel` are `pub` and accept an
arbitrary `node_id`/`rel_id` argument, and `id as usize` additionally
**truncates** on a 32-bit target, giving wrong-record reads/writes there
even without the wraparound. Trigger: an ordinary expand — `path.rs:572`
copies a relationship record's `dst_id`, and the executor later calls
`read_node(dst_id)` on it; a single corrupt or self-referential `dst_id`
equal to an overflow-inducing value panics on a plain `MATCH ()-->()`.

### #3 — property-store header read over-reads past EOF

`get_entity_info_at_offset` (`crates/nexus-core/src/storage/property_store.rs:263-277`)
and `load_properties_at_offset` (`:233-259`) each guard only with:

```rust
if offset as usize >= self.mmap.len() {
    return None;
}
```

then read a 9–13-byte header through `read_u64` (`:667-678`, indexes
`mmap[offset..offset+7]`), `read_u8` (`:691-693`, `offset+8`), and
`read_u32` (`:681-688`, up to `offset+12`) — none of which re-check bounds.
The single `offset >= len` guard permits `offset == len - 1`; for any
`offset` in `[len-12, len)` the header reads walk past `mmap.len()` and
panic. Trigger: `read_node` (`record_store_ops.rs:198-204`) unconditionally
calls `get_entity_info_at_offset(record.prop_ptr)` whenever a node's
on-disk `prop_ptr` is non-zero — a corrupt `prop_ptr` landing in that
12-byte pre-EOF window panics a plain node read. The same call is also
reached from `repair_corrupt_node_prop_ptrs`
(`record_store_ops.rs:133-139`, run at startup) and
`load_node_properties_inner:1158` — both of which exist specifically to
**defend against** corrupt pointers, yet crash on the corrupt input they
are meant to sanitize.

### #4 — file grow is not sized to the write's target offset

`grow_nodes_file` (`crates/nexus-core/src/storage/record_store.rs:273-291`)
and `grow_rels_file` (`:295-311`) compute:

```rust
let calculated_size = ((self.nodes_file_size as f64) * FILE_GROWTH_FACTOR) as usize;
let new_size = calculated_size.max(self.nodes_file_size + min_growth);
```

`new_size` never references the offset the caller is about to write to.
`write_node`/`write_rel` (`record_store_ops.rs:72-80`, `:267-275`) call
`grow_*_file()` once when `offset + SIZE > file_size`, then unconditionally
slice `mmap[start..end]` — if the target `offset` is more than roughly the
`min_growth` (2 MB) ahead of the current file size (an id jump of more than
~65,536 records), the single grow is insufficient and the subsequent
`copy_from_slice` runs past the freshly-remapped mmap and panics.
`property_store::ensure_capacity` (`property_store.rs:620-641`) already
does this correctly — `.max(required_size)` — making this an inconsistency
within the same module, not a novel pattern to invent.

## What Changes

- #2: switch offset computation to `checked_mul`/`checked_add`; on
  overflow, treat as `Error::NotFound`/out-of-range rather than proceeding.
  Additionally gate every read/write by `id < next_*_id` (the logical
  high-water mark) before computing any offset, so ids that are merely
  "in range of the physical file" but never allocated are rejected the same
  way. Use a non-truncating conversion (`u64`-native arithmetic, or a
  fallible `usize::try_from`) so 32-bit targets do not silently truncate.
- #3: change the bounds guard in both `get_entity_info_at_offset` and
  `load_properties_at_offset` from `offset >= mmap.len()` to
  `offset.checked_add(HEADER_LEN).map_or(true, |end| end > mmap.len() as u64)`
  (13-byte header), and apply the same checked-bounds discipline inside
  `read_u64`/`read_u32`/`read_u8` themselves so any future caller is
  protected even if a call site's own guard is later loosened or missed.
- #4: change `new_size` in both `grow_nodes_file` and `grow_rels_file` to
  `calculated_size.max(self.nodes_file_size + min_growth).max(offset + SIZE)`,
  threading the caller's target offset through (mirroring
  `property_store::ensure_capacity`'s existing `.max(required_size)`
  pattern).

## Impact

- Affected specs: `docs/specs/storage-format.md` (record-store and
  property-store bounds/recovery contract)
- Affected code: `crates/nexus-core/src/storage/record_store_ops.rs`
  (`read_node:180-191`, `write_node:69-80`, `read_rel:286-298`,
  `write_rel:264-275`), `crates/nexus-core/src/storage/property_store.rs`
  (`get_entity_info_at_offset:263-277`, `load_properties_at_offset:233-259`,
  `read_u64:667-678`, `read_u32:681-688`, `read_u8:691-693`),
  `crates/nexus-core/src/storage/record_store.rs`
  (`grow_nodes_file:273-291`, `grow_rels_file:295-311`)
- Breaking change: NO — all three fixes turn a panic into an existing
  `Result`/`Option` error path already used elsewhere in the same functions;
  no on-disk format or public signature changes
- User benefit: a single corrupt or adversarially-crafted on-disk pointer
  (`dst_id`, `prop_ptr`, or an id-space gap) can no longer crash a query
  thread or abort the process; corruption-defense code
  (`repair_corrupt_node_prop_ptrs`) can no longer be defeated by the exact
  input it exists to sanitize
- Related: `phase0_fix-store-size-per-clone-divergence` (adjacent
  record-store bounds-checking defect in the same files, found by the same
  audit pass)
