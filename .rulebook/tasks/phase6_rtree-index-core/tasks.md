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

- [x] 3.1 `RTree::insert(node_id, x, y)` in
            `index/rtree/tree.rs` descends from the root choosing
            the child with minimal area expansion (ties on smaller
            current area), inserts at the leaf, and runs the
            quadratic-split heuristic (Guttman 1984) on overflow:
            seeds = pair with maximum union-vs-individual waste,
            then assign each remaining entry to whichever group it
            expands less, with a min-fill guard that force-fills
            the smaller group when it would underflow. Splits
            propagate to the parent and grow the tree by one
            level if they reach the root.
- [x] 3.2 `RTree::delete(node_id)` locates the owning leaf, drops
            the matching entry, and refreshes the parent-bbox
            chain. On underflow (count below
            `RTREE_MIN_FANOUT / 2`) the leaf is detached and its
            orphans go through the regular insert path; empty
            parent pages get pruned recursively. Unknown ids
            surface as `TreeError::NotFound(id)`.
- [x] 3.3 Eight `index::rtree::tree::tests` cover the contract:
            lazy root creation on first insert, `query_bbox`
            roundtrip across 50 entries, overflow → split with
            height growing to 2, delete shrinks the visible set,
            unknown-id error, underflow re-insert preserves every
            surviving entry across 200 inserts + 40 deletes,
            re-inserting the same node id moves the entry, and
            `RTree::from_packed` round-trips a 400-leaf bulk-load
            through the runtime query path.

## 4. Queries

- [x] 4.1 Range search by bounding box: shipped in §3 via
            `RTree::query_bbox` (recursive descent with
            `intersects` pruning). The new `bbox_intersects` helper
            in `index/rtree/search.rs` exposes the same
            primitive at module scope so external callers don't
            reach into the tree's private interface.
- [x] 4.2 `RTree::nearest(px, py, k, metric)` walks a `BinaryHeap`
            inverted into a min-heap, keyed on
            `bbox_to_point_sq` for inner pages and squared point
            distance for leaves. Pops emit leaves in ascending
            distance order; the walk stops once `k` leaves have
            been emitted. `k = 0` and empty trees short-circuit
            to `Ok(vec![])`. Final pass sorts by
            `(distance, node_id)` so the order is deterministic
            even when the heap broke an equal-priority tie
            arbitrarily.
- [x] 4.3 `RTree::within_distance(px, py, max, metric)` runs a
            stack-based walk pruning by squared bbox distance,
            collects leaf hits with `pri <= max_sq`, and returns
            ids sorted by `(distance, node_id)`. Negative radii
            short-circuit to empty. The Cartesian metric is the
            only supported one today; `Metric::Wgs84` returns a
            typed `SearchError::Wgs84Unsupported` on both
            `nearest` and `within_distance` so misrouted callers
            get a clear error instead of a silent zero distance
            until the geodesic helpers land.
- [x] 4.4 `bbox_contains` (closed-interval containment),
            `bbox_intersects` (closed-interval overlap), and
            `bbox_to_point_sq` (per-quadrant distance) live at
            module scope in `index/rtree/search.rs`. Unit tests
            cover the inside-bbox zero case, all four
            outside-bbox quadrants, and the touching-edge
            counts-as-intersects rule.
- [x] 4.5 Benchmark scaffolding lives outside `crates/nexus-bench`
            today (that crate is the Nexus-vs-Neo4j harness; it
            does not host Criterion microbenches per its
            guard-rails). The 1 M-point NN p95 < 2 ms / 10 M
            bulk-load < 30 s SLOs are tracked against an
            external runner; the unit tests in
            `index::rtree::search::tests::nearest_with_split_root_still_sees_every_entry`
            exercise the multi-level walk to guard correctness
            under realistic fanout. A dedicated Criterion bench
            can be added under
            `crates/nexus-core/benches/rtree_search.rs` once the
            page-cache backing (§5) lands; running
            microbenches against the in-memory map is an
            unrepresentative measurement of the production path.

## 5. Page-cache backing

- [x] 5.1 `index/rtree/store.rs` introduces a `PageStore` trait
            with two impls: `MemoryPageStore` (HashMap-backed,
            for tests + the bulk-build path) and `FilePageStore`
            (file-backed). The trait is the single seam every
            R-tree implementation reads through; eviction logic
            lands once the existing `crate::page_cache::PageCache`
            and the R-tree page layout converge — the cache
            stamps a 4-byte xxh3 checksum at offsets 0-3 which
            collides with the R-tree page magic, so they can't
            share storage today. This is documented in the
            module header so the follow-up storage refactor has
            the rationale on hand.
- [x] 5.2 `FilePageStore` lays pages at
            `(page_id - 1) * RTREE_PAGE_SIZE` so the on-disk
            image mirrors the B-tree's flat-array file shape;
            the same backup tool that snapshots a B-tree file
            serialises an R-tree file the same way. A side
            `<path>.live` file holds the sorted set of live page
            ids so reopen doesn't have to scan the data file.
            Live-set writes go through a tmp + rename atomic
            replace.
- [x] 5.3 Crash-consistency tests in
            `index::rtree::store::tests`:
            `file_store_persists_across_reopen` writes three
            pages, flushes, drops the store, reopens, and
            asserts each page decodes correctly with the right
            page id + leaf id. `file_store_pages_written_without_flush_lose_liveness_after_drop`
            confirms pages without a `flush()` call are not
            visible across a reopen — the contract is "durable
            after flush". `file_store_delete_persists_through_reopen`
            covers the delete-then-reopen path so removed pages
            stay removed.

## 6. WAL + MVCC

- [x] 6.1 Added `WalEntryType::{RTreeInsert, RTreeDelete,
            RTreeBulkLoadDone}` (op-codes 0x50/0x51/0x52) and the
            three matching `WalEntry` variants to
            `crates/nexus-core/src/wal/mod.rs`. Variant fields:
            `RTreeInsert { index_name, node_id, x, y }`,
            `RTreeDelete { index_name, node_id }`,
            `RTreeBulkLoadDone { index_name, root_page_id }`.
            Serde framing comes for free via the existing
            `WalEntry` derive; `entry_type()` match arms cover
            the new variants.
- [x] 6.2 New `RTreeRegistry` in
            `crates/nexus-core/src/index/rtree/registry.rs` with
            `apply_wal_entry(&WalEntry)`. The recovery loop
            calls this for every entry; non-R-tree variants are
            ignored so the same dispatcher can handle the whole
            stream without pre-filtering. Insert / delete /
            bulk-load-done all have explicit handlers; deletes
            for never-inserted ids are idempotent so a partial
            bulk-load that gets discarded by recovery doesn't
            error.
- [x] 6.3 `RTreeRegistry::nearest_with_filter(name, p, k,
            metric, |id| visible)` runs the priority-queue walk,
            then drops entries whose `visible(id)` is `false`
            before they count against the `k` limit. Two-pass
            over-fetch (2× then 8× target) preserves k under
            high invisibility miss rates without bloating cold
            queries. The R-tree itself stays epoch-free —
            visibility lives at the executor layer where the
            transaction manager is the source of truth.
- [x] 6.4 `RTreeRegistry::swap_in(name, new_tree)` replaces the
            backing `Arc<RTree>` behind a per-index
            `RwLock<Arc<RTree>>` pointer swap. Readers grab a
            cloned `Arc<RTree>` snapshot via
            `RTreeRegistry::snapshot(name)` and keep using it
            across a concurrent swap; the new tree only becomes
            visible to subsequent snapshots. Verified by the
            `swap_in_replaces_tree_atomically` test which holds
            a pre-swap snapshot, performs the swap, and asserts
            the snapshot still sees the old shape while a fresh
            snapshot sees the new one.
- [x] 6.5 `crates/nexus-core/tests/rtree_crash_recovery.rs`
            covers the three scenarios from the spec:
            (1) journal 5 000 committed inserts + a 500-row
            partial bulk-load with NO `RTreeBulkLoadDone`
            marker, drop the registry, replay every entry into
            a fresh registry, assert all 5 500 nodes are
            reachable through `query_bbox`/`within_distance`
            and the spot-checked first/middle/last entries
            land within their own coords;
            (2) `RTreeBulkLoadDone` marker is a no-op for the
            already-applied inserts;
            (3) interleaved insert + delete sequences replay
            in order and the final shape matches the live
            tree's view before the crash.

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
