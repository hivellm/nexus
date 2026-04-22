# Implementation Tasks — R-tree Index Core

## 1. Page layout + codec

- [ ] 1.1 Define `RTreePageHeader` (32 B: magic, version, level, flags, count) and `ChildRef` (64 B: bbox [f64; 4], child_ptr u64, pad u32) in `crates/nexus-core/src/index/rtree/page.rs`.
- [ ] 1.2 Zero-copy page encode / decode against 8 KB page buffers — asserts fanout in `[1, 127]` and header magic on read.
- [ ] 1.3 Unit tests: round-trip empty, single-entry, full-capacity pages; reject pages with wrong magic or fanout > 127.

## 2. Bulk-load via Hilbert curve

- [ ] 2.1 `hilbert_index_2d(x, y, precision)` and `hilbert_index_3d(x, y, z, precision)` helpers with 48 bits per dimension.
- [ ] 2.2 Sort every `(node_id, point)` pair by Hilbert index; assert the sort is stable so ties break on `node_id` ascending.
- [ ] 2.3 Bottom-up pack: 127 entries per leaf, parent pages hold child bbox unions, recurse to root.
- [ ] 2.4 Byte-identical-output test across two replicas on the same input.

## 3. Incremental insert / delete

- [ ] 3.1 `RTree::insert(node_id, point)` — tree-descend choosing the child whose bbox expansion is minimal; split on overflow via quadratic split.
- [ ] 3.2 `RTree::delete(node_id)` — locate + remove; on leaf underflow re-insert orphaned entries (simpler than merging, competitive for read-heavy).
- [ ] 3.3 Insert / delete unit tests: insert-then-query roundtrip, delete removes from query results, underflow re-insert preserves all surviving entries.

## 4. Queries

- [ ] 4.1 Range search by bounding box: recursive descent pruning on bbox intersection.
- [ ] 4.2 Nearest-neighbour priority queue (incremental k-NN) with min-heap ordered by bbox-to-point distance; stops once `k` leaves have been popped.
- [ ] 4.3 Within-distance: expand into great-circle distance (WGS-84) or Euclidean (Cartesian) based on CRS of the query point.
- [ ] 4.4 Contains / intersects helpers for bbox geometry.
- [ ] 4.5 Benchmark in `crates/nexus-bench/benches/rtree_bench.rs`: 1 M random points, NN p95 < 2 ms; `withinDistance` p95 < 3 ms; bulk-load 10 M points < 30 s.

## 5. Page-cache backing

- [ ] 5.1 `RTreePageStore` built on `crate::page_cache::PageCache` with 8 KB pages, Clock / 2Q / TinyLFU eviction matching the B-tree surface.
- [ ] 5.2 Memory-mapped file layout mirroring `index/btree.rs` so the same backup tool serialises both.
- [ ] 5.3 Crash consistency test: write N pages, kill mid-sync, reopen, assert torn pages are detected and dropped.

## 6. WAL + MVCC

- [ ] 6.1 Add `WalEntry::RTreeInsert { index_id, node_id, bbox }`, `WalEntry::RTreeDelete { index_id, node_id }`, `WalEntry::RTreeBulkLoadDone { index_id, root_page }` to `crates/nexus-core/src/wal/mod.rs`.
- [ ] 6.2 Replay dispatcher calls `RTreeRegistry::apply_wal_entry` the way FTS already works.
- [ ] 6.3 Snapshot-aware read filter: after a seek, drop entries whose owning node is invisible at the reader's epoch.
- [ ] 6.4 Atomic rebuild via `arc_swap`: old tree stays queryable until the new one is WAL-synced, then a single swap promotes it.
- [ ] 6.5 Crash-recovery integration test at `crates/nexus-core/tests/rtree_crash_recovery.rs` — journal 5 000 inserts + a partial bulk-load-done, simulate kill-9, reopen, assert every WAL-committed row is visible.

## 7. Registry + executor integration

- [ ] 7.1 Add `IndexManager::rtree: RTreeRegistry` paralleling `IndexManager::fulltext`; keyed by `"{label}.{prop}"`.
- [ ] 7.2 Move the ad-hoc `ExecutorShared::spatial_indexes` map to source from `IndexManager::rtree` instead (paves the way for auto-populate).
- [ ] 7.3 Re-point the `SpatialIndex` type alias at the new R-tree; port the two grid-specific tests in `geospatial_integration_test.rs`.
- [ ] 7.4 Swap `execute_spatial_nearest` from linear `entries()` scan to the priority-queue walk.
- [ ] 7.5 Parser: `CREATE INDEX ... FOR (n:Label) ON (n.prop) USING RTREE` as a grammar alias for `CREATE SPATIAL INDEX`.

## 8. Docs + telemetry

- [ ] 8.1 New `docs/specs/rtree-index.md`: page layout, bulk-load algorithm, MVCC rules, WAL framing.
- [ ] 8.2 New `docs/guides/GEOSPATIAL.md`: end-user guide covering all `point.*` predicates, `spatial.*` procedures, CREATE INDEX, tuning knobs.
- [ ] 8.3 Update `docs/specs/knn-integration.md` so the spatial section points at the new backend alongside HNSW.

## 9. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 9.1 Update or create documentation covering the implementation
- [ ] 9.2 Write tests covering the new behavior
- [ ] 9.3 Run tests and confirm they pass
- [ ] 9.4 Quality pipeline: `cargo +nightly fmt --all` + `cargo +nightly clippy -p nexus-core --all-targets --all-features -- -D warnings` + coverage >= 95% on new code.
- [ ] 9.5 CHANGELOG entry "Added packed Hilbert R-tree backend for spatial indexes".
