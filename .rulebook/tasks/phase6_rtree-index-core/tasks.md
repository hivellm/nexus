# Implementation Tasks — R-tree Index Core

## 1. Page layout + codec

- [x] 1.1 Defined `RTreePageHeader { level, flags, count, page_id }`
            (32 B header) and `ChildRef { bbox: [f64; 4],
            child_ptr: u64, extra: u64 }` (64 B entry) in
            `crates/nexus-core/src/index/rtree/page.rs`. Module
            constants for magic/version/page-size/fanout live in
            `index/rtree/mod.rs`.
- [x] 1.2 `encode_page` / `decode_page` operate on the 8 KB
            `RTREE_PAGE_SIZE` buffer; asserts `count <= 127`,
            validates `RTREE_PAGE_MAGIC`, the page version, and
            zero-reserved bytes on decode. Decode returns a typed
            `PageDecodeError` for every failure mode.
- [x] 1.3 Twelve `index::rtree::page::tests` unit tests cover
            round-trip (empty / single-entry / full-capacity /
            inner-page / 3-D z-coord), determinism, and the four
            decode-error paths (bad length, bad magic, bad version,
            fanout overflow, non-zero reserved bytes).

## 2. Bulk-load via Hilbert curve

- [x] 2.1 `hilbert_index_2d` (48 bits/dim, total ≤ 96 bits in
            `u128`) + `hilbert_index_3d` (32 bits/dim, total
            96 bits) implemented via Lam-Shapiro 2-D and
            Skilling 3-D iterations; `normalise_2d` /
            `normalise_3d` map real-valued coords onto the
            discrete Hilbert grid with bbox-driven scaling and
            input clamping.
- [x] 2.2 `sort_by_hilbert_2d` / `sort_by_hilbert_3d` use
            `sort_by_cached_key` keyed on `(hilbert_index, node_id)`
            so ties break stably on node id ascending; eleven
            `index::rtree::hilbert::tests` cover the d=1 / d=2
            bijection, high-precision overflow safety, clamping,
            collapsed-axis handling, locality of sorted
            neighbours on a 4×4 grid, stability on duplicate
            coords, and run-to-run determinism.
- [x] 2.3 `bulk_pack` in `index/rtree/packer.rs` chunks the
            Hilbert-sorted leaves into 127-entry pages, computes
            per-chunk bounding-box unions for the parent level,
            and recurses level-by-level until a single root
            remains. Page ids are assigned monotonically in pack
            order; the root id is the last one allocated. Returns
            `PackedTree { pages, root_page_id, height }` so callers
            can stream the encoded buffers straight into the page
            store.
- [x] 2.4 `pack_is_byte_identical_across_runs` packs a 500-entry
            input twice and asserts every page byte plus the root
            id and height match. Combined with the deterministic
            encoder (§1) and stable Hilbert sort (§2.2), two
            replicas given the same input produce byte-identical
            page files. Eight more `index::rtree::packer::tests`
            cover empty input, single-leaf, below-fanout
            single-page, full-capacity-no-promote, overflow → two
            levels, three-level overflow at fanout², parent-bbox
            covers every leaf, and leaf 3-D z-coord round-tripping
            through the pack.

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
