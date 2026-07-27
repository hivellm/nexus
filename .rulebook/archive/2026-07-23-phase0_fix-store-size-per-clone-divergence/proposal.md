# Proposal: phase0_fix-store-size-per-clone-divergence

**Priority: MEDIUM — a `RecordStore` clone's cached file-size fields can
diverge from the shared mmap they bound-check against, causing spurious
`NotFound` results under concurrent growth and an out-of-bounds panic under
concurrent `clear_all`.** Found independently by two audits during the same
review pass; not previously reported.

## Why

`RecordStore` shares its mmaps across clones via `Arc<RwLock<MmapMut>>`
(`crates/nexus-core/src/storage/record_store.rs:37-39`), but caches the file
sizes used to bound-check against them as plain, per-instance `usize` fields
(`:59-61`):

```rust
pub(super) nodes_mmap: Arc<RwLock<MmapMut>>,
...
pub(super) nodes_file_size: usize,
pub(super) rels_file_size: usize,
```

`Clone` (`record_store.rs:364-377`) `Arc::clone`s the mmaps (`:368-369`) but
copies the size fields **by value** (`:375-376`):

```rust
nodes_mmap: Arc::clone(&self.nodes_mmap),
rels_mmap: Arc::clone(&self.rels_mmap),
...
nodes_file_size: self.nodes_file_size,
rels_file_size: self.rels_file_size,
```

`RecordStore` is cloned on every `refresh_executor` (per the `nodes_mmap`
field doc comment, `record_store.rs:33-36`), so the engine's `RecordStore`
and the executor's cloned `RecordStore` share one physical mmap but hold two
independent size snapshots from the moment of the clone onward. The read
path bound-checks against the **cached** size, then indexes the **shared**
mmap: `read_node` (`record_store_ops.rs:182`)
`if offset + NODE_RECORD_SIZE > self.nodes_file_size { return NotFound }`
then `:189-190` `guard[start..end]`; `read_rel` is the same shape at
`:288`/`:297`.

### Grow case — spurious `NotFound`

`grow_nodes_file`/`grow_rels_file` (`record_store.rs:273-311`) replace the
**shared** mmap in place (`*self.nodes_mmap.write().unwrap() = ...`,
`:286-287`) but update only the **caller's own** `nodes_file_size`
(`:289`). A reader holding an older clone (stale-small `nodes_file_size`)
now bound-checks a node that physically exists — because the shared mmap
was grown by a different clone — against its own smaller size, and
incorrectly returns `NotFound`. This is a missing-rows correctness bug, not
a crash.

### `clear_all` case — out-of-bounds panic

`clear_all` (`record_store_ops.rs:1064-1087`) **shrinks** the shared mmap
back to `INITIAL_*_FILE_SIZE` (`:1064-1065`, `:1085-1087`) and updates only
the caller's own size fields (`:1081`
`self.nodes_file_size = INITIAL_NODES_FILE_SIZE;`). The admin entry point,
`Engine::clear_all_data` (`crates/nexus-core/src/engine/maintenance.rs:128-137`),
calls `self.storage.clear_all()` but does **not** call `refresh_executor`
afterward, so any executor clone created before the `clear_all` keeps its
stale-**large** `nodes_file_size`. A concurrent lock-free reader running on
that clone (per the isolation model documented in the durability audit: the
read path clones the executor under a brief lock then executes via
`spawn_blocking` holding no engine lock) evaluates
`offset + NODE_RECORD_SIZE > self.nodes_file_size` as **false** (the cached
size is still the large, pre-clear value), so the bound check passes, and
`guard[start..end]` slices the now-**smaller**, shrunk mmap — out of bounds
— and panics in the `spawn_blocking` task. The reader's `std::sync::RwLock`
read guard is not poisoned by the panic, so only that one query task dies;
other queries survive.

This is **mostly** gated in practice today: `clear_all` also resets the
shared `next_node_id`/`next_rel_id` atomics, and most read entry points
(e.g. `get_node`, `record_store_ops.rs:916`) check `id < next_*_id` before
reaching the size-based bound check at all — closing off the common path.
But it is a real invariant violation (the size fields are documented and
used as if they tracked the shared mmap, and do not), and any read path
that reaches the size check without first going through the `next_*_id`
gate is a live OOB vector, not merely a theoretical one.

The store already has a **correct** precedent for bound-checking against
live shared state instead of a cached snapshot: `read_all_node_headers`
(`record_store.rs:255-258`) takes the mmap read lock and clamps against
`guard.len()` (the mapping's actual current length) rather than a cached
size field.

## What Changes

- Bound-check every read against the **live mapping length**, inside the
  same lock acquisition used to read the data, instead of the cached
  `nodes_file_size`/`rels_file_size` fields — mirroring the pattern already
  used by `read_all_node_headers`. This removes the size-field/mmap
  divergence entirely for the read path, since there is no longer a second
  piece of state that can go stale.
- Where a size value is still needed outside the lock (e.g. for capacity
  planning before a write/grow decision), make it genuinely shared —
  `Arc<AtomicUsize>` updated under the same mmap write lock at grow/clear
  time — rather than a plain per-clone `usize`, so every clone observes the
  same value without a `refresh_executor` round-trip.
- Wire `Engine::clear_all_data` (`maintenance.rs:128-137`) to refresh any
  cached executor state after `storage.clear_all()`, as defense in depth
  alongside the shared-state fix (belt-and-suspenders: the shared-state fix
  closes the class of bug; the refresh closes the specific `clear_all`
  window promptly rather than leaving it to the next natural
  `refresh_executor`).

## Impact

- Affected specs: `docs/specs/storage-format.md` (record-store bounds
  contract for shared/cloned stores)
- Affected code: `crates/nexus-core/src/storage/record_store.rs` (fields
  `:59-61`, `Clone` `:364-377`, `grow_nodes_file`/`grow_rels_file` `:273-311`,
  `read_all_node_headers:255-258` as the reference pattern),
  `crates/nexus-core/src/storage/record_store_ops.rs` (`read_node:182,189-190`,
  `read_rel:288,297`, `clear_all:1064-1087`),
  `crates/nexus-core/src/engine/maintenance.rs` (`clear_all_data:128-137`)
- Breaking change: NO — internal bound-checking mechanism only; no public
  API or on-disk format change
- User benefit: concurrent reads no longer spuriously miss rows during a
  grow, and can no longer panic a query task during a concurrent
  `clear_all`/admin drop-database flow
- Related: `phase0_fix-storage-oob-panics` (sibling record-store bounds
  defects in the same two files, found by the same audit pass — that task
  covers arithmetic-overflow and header-over-read bounds bugs; this task
  covers the shared-vs-cached-state divergence, a structurally different
  defect)
