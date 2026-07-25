# Proposal: phase0_perf-ingest-relationship-fsync-per-edge

**Priority: HIGH (performance). Relationship ingest is ~20× slower than Neo4j
and ~3× slower than Nexus's own node ingest, entirely because of a durable
LMDB fsync per relationship.**

## Why

Loading LDBC SF0.1 takes ~480 s on Nexus vs ~30 s on Neo4j. Measurement
(`docs/analysis/ingest-relationship-throughput/`) localised the cost to a fixed
per-relationship cost of ~0.33 ms that batching cannot amortise and graph size
does not affect:

- Node ingest: ~8 500 nodes/s. Relationship ingest: ~3 000 rel/s. Neo4j: ~60 000 rel/s.
- Rate is FLAT across `/ingest` batch sizes (100 → 20 000), so the cost is per
  ROW, below the handler.
- Ruled out (each with a micro-benchmark): O(degree) hub chains (prepend is
  O(1)); graph-size-dependent scans; batch/transaction overhead.

Root cause: `Engine::create_relationship` calls `catalog.increment_rel_count`
for every edge, which does `get_statistics` (LMDB read + deserialize the whole
stats blob) + `update_statistics` (LMDB write + serialize + **commit**). The
catalog env opens with default flags (no `NoSync`), so every commit is a
durable data+metadata fsync — one disk sync PER relationship, just to bump a
counter. Nodes avoid this: `create_node_inner` does not increment per node
(counts are batched via `batch_increment_node_counts`).

This is a KNOWN gap. `engine/mod.rs:481-491` documents that the executor CREATE
operator batches node counts but never added the rel-count equivalent, calling
the fix "a separate follow-up."

Secondary: `relationship_storage` (Phase 8) is `Some` by default, so every edge
is ALSO written into a parallel `RelationshipStorageManager` whose read path is
disabled (traversal now uses the store adjacency index) — dead-on-read work
paid per write.

## What Changes

- Add `Catalog::batch_increment_rel_counts` (twin of the node version: one
  `get_statistics` + one `update_statistics` per batch, not per edge).
- Route the `/ingest` relationship loop and the executor CREATE path through
  it, so a batch of N edges does ONE LMDB commit — and the executor path stops
  under-counting rel totals (closes the `engine/mod.rs:481` caveat).
- Gate/retire the redundant per-row Phase-8 `RelationshipStorageManager` write
  once its isolated cost is measured.

## Impact

- Affected specs: none (internal write-path / catalog change).
- Affected code: `crates/nexus-core/src/catalog/stats.rs` (batch API),
  `crates/nexus-core/src/engine/crud/relationships.rs` (stop per-edge
  increment), `crates/nexus-server/src/api/ingest.rs` (accumulate + flush per
  batch), `crates/nexus-core/src/executor/operators/create.rs` (add rel-count
  column to the existing batch), `executor/shared.rs` (Phase-8 gating).
- Breaking change: NO — same results, same durability for the record stores;
  only the catalog stats-blob update cadence changes from per-edge to
  per-batch.
- User benefit: relationship ingest an order of magnitude faster; LDBC and any
  bulk relationship load stop being fsync-bound; the catalog rel total becomes
  correct through both write paths.
- Related: `phase0_fix-ingest-bulk-path` (made `/ingest` go straight to the
  engine), `phase0_perf-store-reverse-incoming-adjacency-index` (added the
  adjacency index that supersedes the Phase-8 read path).
