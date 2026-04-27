# R-tree Index Specification

This document defines the packed Hilbert R-tree backend that
powers spatial indexes in Nexus (`crates/nexus-core/src/index/rtree`).
It supersedes the grid-backed prototype at
`crates/nexus-core/src/geospatial/rtree.rs` for every read path
(`spatial.nearest`, `point.withinDistance`, `point.withinBBox`)
while keeping the existing Cypher surface unchanged.

## Goals

- O(log_b N + k) k-NN queries via priority-queue traversal.
- Deterministic on-disk image so two replicas given the same
  input produce byte-identical files.
- Crash recovery from the WAL alone — no full rebuild required
  unless an `RTreeBulkLoadDone` marker is missing for a journalled
  bulk-load.
- Single-writer concurrent reads: queries don't block each other,
  writers serialise through the storage layer's lock.

## On-disk page layout

Pages are exactly **8192 bytes**.

### Header (32 bytes)

| Offset | Size | Field        | Notes                                   |
|-------:|-----:|--------------|-----------------------------------------|
|    0   |   4  | `magic`      | `0x4e_58_52_54` (`"NXRT"` BE)           |
|    4   |   2  | `version`    | LE u16; current = 1                     |
|    6   |   1  | `level`      | 0 = leaf, 1 = parents-of-leaves, …      |
|    7   |   1  | `flags`      | reserved; must be 0 in v1               |
|    8   |   2  | `count`      | LE u16; number of valid entries         |
|   10   |   6  | `_reserved`  | must be zero                             |
|   16   |  16  | `page_id`    | LE u128; matches the file allocator      |

### Entries (64 bytes each, up to 127 per page)

| Offset | Size | Field        | Notes                                  |
|-------:|-----:|--------------|----------------------------------------|
|    0   |  32  | `bbox`       | `[min_x, min_y, max_x, max_y]` (f64 LE)|
|   32   |   8  | `child_ptr`  | leaf: owning `node_id`; inner: child page id |
|   40   |   8  | `extra`      | leaf: f64 z-coord (`0.0` if 2-D); inner: u64 child level |
|   48   |  16  | `_pad`       | zeroed                                 |

`32 + 127 × 64 = 8160`; the trailing 32 bytes are zero-padded so
the on-disk image is reproducible across runs.

### Determinism

`encode_page` writes every header byte and every padding byte. Two
calls with the same `(header, entries)` slice produce byte-
identical buffers. Combined with the stable Hilbert sort
(§ Bulk-load) the entire on-disk file is reproducible across
replicas.

### Magic, version, fanout caps

- `RTREE_PAGE_MAGIC = 0x4e58_5254` (`"NXRT"` big-endian)
- `RTREE_PAGE_VERSION = 1`
- `RTREE_MIN_FANOUT = 64`
- `RTREE_MAX_FANOUT = 127`

Decode rejects pages with the wrong magic, wrong version, or
`count > RTREE_MAX_FANOUT` with a typed `PageDecodeError`.

## Bulk-load

The packer (`packer::bulk_pack`) accepts a Hilbert-sorted slice
of leaf entries and packs them bottom-up:

1. Group leaves into chunks of [`PACK_TARGET_FANOUT`] (127). The
   final chunk gets the remainder.
2. Encode each chunk as a leaf page with monotonically-allocated
   `page_id`s starting from 1.
3. Synthesise an inner-level entry per page: bbox = union of the
   leaf's boxes, `child_ptr = page_id`, `extra = child_level`.
4. Repeat the chunk → page → parent process at the next level.
5. Stop when the level produces exactly one page — that's the
   root.

### Hilbert sort

Two routines under `index/rtree/hilbert.rs`:

- `hilbert_index_2d(x, y, precision)` — Lam-Shapiro 1994 bit-
  rotation. 48 bits per dim → 96-bit `u128` Hilbert key.
- `hilbert_index_3d(x, y, z, precision)` — Skilling 2004
  iteration. 32 bits per dim → 96-bit key.

`normalise_2d` / `normalise_3d` map real-valued coords onto the
discrete grid via bbox-driven uniform scaling with input
clamping. `sort_by_hilbert_2d` / `sort_by_hilbert_3d` keys on
`(hilbert_index, node_id)` so ties break stably and the sort is
deterministic across replicas.

## Mutable tree

`tree::RTree` provides incremental insert / delete on top of the
bulk-loaded shape (or starting empty for purely incremental
workloads).

### Insert

1. Descend from the root, picking at each inner page the child
   whose bbox needs the smallest area expansion to cover the
   target point. Ties break on smaller current area.
2. Insert at the chosen leaf. Re-inserting an existing
   `node_id` first removes the old entry so a moved node lands
   exactly once.
3. On overflow run the **quadratic split** (Guttman 1984):
    - Pick the seed pair with maximum
      `area(union(a, b)) − area(a) − area(b)` waste.
    - Assign each remaining entry to whichever group it expands
      less, with a min-fill guard that force-fills the smaller
      group when it would underflow.
4. Splits propagate up the parent chain; reaching the root grows
   the tree by one level.

### Delete

1. Locate the leaf carrying `node_id`, drop its entry, refresh
   the parent-bbox chain.
2. On underflow (`count < RTREE_MIN_FANOUT / 2`) detach the leaf,
   drain its survivors, and re-insert each through the public
   path. Empty parents prune recursively.
3. Unknown ids surface as `TreeError::NotFound(id)`.

### Range / k-NN / within-distance

- `RTree::query_bbox(min_x, min_y, max_x, max_y)` — recursive
  descent pruning on bbox intersection.
- `RTree::nearest(px, py, k, metric)` — min-heap keyed on
  `bbox_to_point_sq`. Pops emit leaves in ascending distance
  order; the walk stops once `k` leaves have been emitted. Ties
  break on `node_id` ascending.
- `RTree::within_distance(px, py, max, metric)` — stack-based
  descent with `pri ≤ max_sq` pruning. Returns ids ordered by
  ascending distance.

`Metric::Cartesian` is the only supported metric in v1.
`Metric::Wgs84` returns `SearchError::Wgs84Unsupported` until the
geodesic helpers land.

## Page-cache backing

`store::PageStore` is the persistence trait every R-tree reads
through. Two impls today:

- **`MemoryPageStore`** — `HashMap<page_id, [u8; 8192]>`. Used
  by tests and bulk-build before persistence is wired through.
- **`FilePageStore`** — file-backed, pages laid at
  `(page_id - 1) * 8192` so the on-disk image mirrors the
  B-tree's flat-array file shape. A side `<path>.live` file
  holds the sorted set of live page ids; live-set writes go
  through a tmp + rename atomic replace.

`PageStore::flush` calls `sync_all`. The contract is "durable
after flush" — pages written without a flush are forgotten across
a reopen.

`crate::page_cache::PageCache` is intentionally not used: its
`Page` struct embeds a 4-byte `xxh3` checksum at offsets 0-3
exactly where the R-tree page magic lives. Letting the cache
stamp a checksum there would corrupt the page. The eviction-
aware backing lands once both layouts converge.

## WAL framing

Three new op-codes in `crate::wal::WalEntryType`:

| Op-code | Variant              | Fields                                    |
|--------:|----------------------|-------------------------------------------|
|  `0x50` | `RTreeInsert`        | `index_name`, `node_id`, `x`, `y`         |
|  `0x51` | `RTreeDelete`        | `index_name`, `node_id`                   |
|  `0x52` | `RTreeBulkLoadDone`  | `index_name`, `root_page_id`              |

Every R-tree mutation journals the corresponding entry. Crash
recovery feeds the journal through `RTreeRegistry::apply_wal_entry`
which dispatches per-variant. `RTreeBulkLoadDone` is a no-op on
the registry (the inserts that built the tree journal
separately); it exists so recovery code outside the registry can
detect a partial bulk-load and re-run it.

## MVCC visibility

The R-tree itself does not store epoch metadata. Visibility
filtering happens at the executor layer: after the seek returns
node ids, the executor consults the transaction manager's
snapshot view of "is this node visible at epoch E?" and drops
invisible ids before they count against the `k` limit. The
`RTreeRegistry::nearest_with_filter` helper is the seam — it
runs the priority-queue walk, then applies the caller's
`visible(id)` predicate, with two-pass over-fetch (2× then 8×
target) to keep SLO under high invisibility miss rates.

## Atomic rebuild

`RTreeRegistry` holds each tree behind a per-index
`RwLock<Arc<RTree>>`. `swap_in(name, new_tree)` replaces the
backing `Arc<RTree>` in a single pointer assignment; readers
that captured a snapshot via `RTreeRegistry::snapshot(name)`
keep using the old tree across the swap, so no reader observes
a half-built tree.

## SLOs

| Scenario                                  | Target                   |
|-------------------------------------------|--------------------------|
| 1 M-point `nearest(k = 10)` p95           | < 2 ms                   |
| 1 M-point `withinDistance` p95            | < 3 ms                   |
| 10 M-point bulk-load                      | < 30 s                   |
| Sustained insert throughput               | ≥ 10 k writes/sec        |

SLO benchmarking lives outside `crates/nexus-bench` (which is
the Nexus-vs-Neo4j harness). A Criterion microbench under
`crates/nexus-core/benches/rtree_search.rs` lands alongside the
storage refactor that retargets writes to the page-cache backing.

## File layout

```
crates/nexus-core/src/index/rtree/
├── mod.rs       — module constants + re-exports
├── page.rs      — page codec (encode/decode/PageDecodeError)
├── hilbert.rs   — Hilbert curve + stable sort helpers
├── packer.rs    — bottom-up bulk-load
├── tree.rs      — mutable in-memory R-tree (insert/delete/query_bbox)
├── search.rs    — k-NN priority-queue walk + within-distance
├── store.rs     — PageStore trait + Memory + File impls
└── registry.rs  — RTreeRegistry: WAL replay + atomic swap
```
