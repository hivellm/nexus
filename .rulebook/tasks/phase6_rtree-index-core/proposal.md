# Proposal: phase6_rtree-index-core

## Why

`phase6_opencypher-geospatial-predicates` slice A shipped the
Cypher surface (`point.*` predicates, `spatial.*` procedures,
`spatial.nearest` streaming) on top of the existing grid-backed
`RTreeIndex` at `crates/nexus-core/src/geospatial/rtree.rs`. The
"R-tree" in the module name is a misnomer: it is a
`HashMap<(i32, i32), RoaringBitmap>` with a fixed `cell_size =
100.0`. Consequences:

- `query_bbox((min_x, min_y, max_x, max_y))` does grid-cell
  enumeration over `(max_x - min_x) / cell_size * (max_y -
  min_y) / cell_size` cells. A world-sized bbox blows the walk
  up to ~4e9 cells — slice A's first attempt at
  `spatial.nearest` hung the integration test and had to switch
  to `RTreeIndex::entries()` linear scan.
- `spatial.nearest(p, 'Store', k)` is `O(N)` in the number of
  indexed points. The priority-queue k-NN walk the parent
  task's design doc promised (p95 < 2 ms at 1 M points) is not
  deliverable against this backend.
- There is no WAL journalling, no MVCC snapshot-aware read
  path, no crash recovery. A restart with any pending spatial
  writes loses them silently.
- Cluster replicas cannot converge bit-for-bit because the grid
  stores an unordered `HashMap` — deterministic bulk-load is
  impossible.

The parent task's design doc already specified the correct
structure: a packed Hilbert R-tree with 8 KB memory-mapped
pages, fanout 64-127, deterministic bulk-load via Hilbert-curve
sort, quadratic-split insert, MVCC-gated reads, and a
priority-queue NN walk. This task delivers it.

## What Changes

1. **New R-tree module** `crates/nexus-core/src/index/rtree/`
   with submodules `mod.rs` (node / leaf structs),
   `hilbert.rs` (bulk-load via Hilbert-curve sort), `search.rs`
   (NN priority queue + bbox range + within-distance),
   `page.rs` (page codec + page-cache glue).
2. **8 KB memory-mapped pages** backed by the same page cache
   (`crate::page_cache`) the B-tree already uses. Page layout
   per the parent design: 32 B header + N x 64 B ChildRef
   entries (bbox: [f64; 4] + child_ptr: u64 + _pad: u32) — max
   fanout 127, min fanout 64 enforced after bulk-load.
3. **WAL op-codes** `WalEntry::RTreeInsert`,
   `WalEntry::RTreeDelete`, `WalEntry::RTreeBulkLoadDone`,
   each with the same framing the existing FTS / B-tree ops
   use.
4. **MVCC integration**: R-tree entries carry the owning
   node-id; readers filter results by the epoch visibility
   rules the existing `TransactionManager` already implements.
5. **Index-layer promotion**: add `IndexManager::rtree:
   RTreeRegistry` paralleling `IndexManager::fulltext`. The
   `SpatialIndex` type alias in the executor crate is
   re-pointed at the new R-tree. The grid backend at
   `geospatial/rtree.rs` is removed once callers migrate; the
   two tests that reference it directly get ported to the new
   module.
6. **Atomic rebuild**: concurrent writes during bulk-rebuild
   use the pointer-swap pattern — old R-tree stays queryable
   until the new one is WAL-synced, then an `arc_swap`-style
   replace promotes it.
7. **Crash-recovery test** that mirrors
   `phase6_fulltext-async-writer::fulltext_crash_recovery.rs`:
   journal 5 000 inserts + a partial `RTreeBulkLoadDone`,
   simulate a kill-9, reopen, assert every WAL-committed row
   surfaces.
8. **`spatial.nearest` rewrite**: swap the linear
   `index.entries().iter().sort_by(...)` walk for the
   priority-queue traversal that stops after k results. This
   is the key SLO win.
9. **`USING RTREE` grammar alias** alongside the existing
   `CREATE SPATIAL INDEX ON :Label(prop)` so Neo4j-dialect
   scripts parse without porting.
10. **Deterministic bulk-load test** that hashes the page file
    and asserts byte-identical output across two replicas.
11. **Benchmarks** under `crates/nexus-bench` covering the
    parent-proposal SLOs: 1 M random points,
    `withinDistance` p95 < 3 ms, `nearest` k=10 p95 < 2 ms,
    bulk-load 10 M points < 30 s, sustained 10 k writes/sec.

## Impact

- Affected specs: NEW `docs/specs/rtree-index.md`, MODIFIED
  `docs/specs/knn-integration.md` (spatial section), NEW
  `docs/guides/GEOSPATIAL.md`.
- Affected code: NEW `crates/nexus-core/src/index/rtree/`
  (mod.rs, hilbert.rs, search.rs, page.rs); DELETED
  `crates/nexus-core/src/geospatial/rtree.rs`; MODIFIED
  `crates/nexus-core/src/index/mod.rs`,
  `crates/nexus-core/src/executor/shared.rs`,
  `crates/nexus-core/src/executor/operators/admin.rs`,
  `crates/nexus-core/src/executor/operators/procedures.rs::
  execute_spatial_nearest`, `crates/nexus-core/src/wal/mod.rs`;
  NEW `crates/nexus-core/tests/rtree_crash_recovery.rs`,
  `crates/nexus-bench/benches/rtree_bench.rs`.
- Breaking change: NO. Cypher surface from slice A stays
  identical; the grid backend is replaced in-place and
  `spatial.addPoint` keeps working.
- User benefit: p95 latency for `spatial.nearest` drops from
  `O(N)` to `O(log N + k)`; `withinDistance` queries stop
  enumerating world-sized grids; crash recovery covers
  spatial data end-to-end for the first time.
- Dependencies: requires slice A (merged); unblocks
  `phase6_spatial-planner-seek` (needs a real index to seek)
  and `phase6_spatial-index-autopopulate` (needs the
  `IndexManager::rtree` handle this task introduces).
- Timeline: 2-3 weeks. Complexity medium (R-tree is textbook;
  the hairy part is the MVCC + atomic-rebuild interaction).
  Risk medium — Windows MSVC linker contention is a known
  slowdown; the benchmark suite needs a dedicated Linux runner
  for the p95 SLOs.
