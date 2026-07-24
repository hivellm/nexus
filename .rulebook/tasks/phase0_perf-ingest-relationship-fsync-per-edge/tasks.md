# Tasks: phase0_perf-ingest-relationship-fsync-per-edge

Relationship ingest is fsync-bound: `Engine::create_relationship` calls
`catalog.increment_rel_count` per edge, which commits a durable LMDB write
transaction (the catalog env has no `NoSync`) just to bump a counter. Nodes
avoid this via `batch_increment_node_counts`. Full analysis + measurements:
`docs/analysis/ingest-relationship-throughput/`. Order matters: lock in the
baseline and a repeatable micro-benchmark FIRST, so every later step is
measured, not assumed.

## 1. Implementation
- [ ] 1.1 Commit a repeatable relationship-ingest micro-benchmark (extend the
  harness sketched in `docs/analysis/ingest-relationship-throughput/01`) that
  reports rel/s for distinct-source, same-source, and across batch sizes, plus
  an end-to-end SF0.1 relationship-pass wall-clock. Record the baseline
  (~3 000 rel/s; ~480 s SF0.1) so §1.4/§2 can show the delta.
- [ ] 1.2 Add `Catalog::batch_increment_rel_counts(&[(TypeId, u32)])` in
  `catalog/stats.rs` — one `get_statistics` + one `update_statistics` for the
  whole slice, mirroring `batch_increment_node_counts` exactly (same lock
  discipline, same single commit). Unit-test it against N single increments for
  count-equivalence.
- [ ] 1.3 Stop the per-edge increment on the `/ingest` path: remove the
  `self.catalog.increment_rel_count(type_id)?` call from
  `engine/crud/relationships.rs` for the batched case, and have
  `api/ingest.rs` accumulate `(type_id, +1)` per created edge and flush ONCE
  per relationship batch via `batch_increment_rel_counts`. Keep the counter
  correct: the accumulation must cover every edge the batch actually created
  (skip the failed rows, same as `nodes_ingested`).
- [ ] 1.4 Re-measure with §1.1. Gate: relationship ingest ≥ node ingest
  (~8 500 rel/s); record the number. If still short, profile the next per-row
  cost (Phase-8 write, WAL, node reads) before proceeding — do not guess.
- [ ] 1.5 Close the executor CREATE path's rel-count gap (`engine/mod.rs:481`
  caveat): `executor/operators/create.rs` already calls
  `batch_increment_node_counts`; add the rel-type column so `UNWIND … CREATE`
  also records rel counts — through the batched API, so it stays fast. Verify
  the catalog rel total now matches the created edges through BOTH `/ingest`
  AND `UNWIND … CREATE`.
- [ ] 1.6 Measure the isolated cost of the redundant Phase-8 per-edge write
  (`relationship_storage` / `relationship_property_index` in
  `engine/crud/relationships.rs:83-114`; `Some` by default via
  `executor/shared.rs`). Its read path is disabled and traversal now uses the
  store adjacency index. If its cost is material, gate it behind an
  off-by-default flag (or retire it) — evidence-backed, not assumed. Confirm no
  read path still depends on it.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation — refresh
  the `docs/analysis/ingest-relationship-throughput/` numbers with the
  post-fix rates, and correct the `engine/mod.rs:481-491` caveat comment once
  rel counts are batched on both paths.
- [ ] 2.2 Write tests covering the new behavior — `batch_increment_rel_counts`
  count-equivalence; an `/ingest` batch leaves `catalog` rel total == created
  edges (not per-edge, not zero); an `UNWIND … CREATE` batch does likewise;
  the failed-row case does not over-count.
- [ ] 2.3 Run tests and confirm they pass — `cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core -p nexus-server --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green, plus the §1.4 throughput gate.

## Related
- `phase0_fix-ingest-bulk-path` — routed `/ingest` straight to
  `Engine::create_node`/`create_relationship` (the path this optimises).
- `phase0_perf-store-reverse-incoming-adjacency-index` — added the store
  adjacency index that supersedes the Phase-8 relationship-storage read path.
- `phase7_ldbc-snb-benchmark` — the load whose ~16× gap motivated this.
