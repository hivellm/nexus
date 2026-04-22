# Proposal: phase2b_consolidate-oncelock-schema-stats-knn

## Why

Second decomposition slice of the old `phase2_consolidate-oncelock-into-app-state`.
`api/schema.rs` has `CATALOG`, `api/stats.rs` has `CATALOG` + `LABEL_INDEX` +
`KNN_INDEX` + `ENGINE`, and `api/knn.rs` has `EXECUTOR`. All six OnceLocks
duplicate data that lives on `NexusServer` — the catalog is reachable via
`server.engine.catalog`, the label/knn indexes live on
`server.engine.indexes.label_index` / `.knn_index`, and the executor is
`server.executor`. Same test-isolation problem as phase2a.

Depends on: phase2a (so the `NexusServer` extractor pattern is already
landed in cypher/data and there is a template to copy).

## What Changes

- Migrate every handler in `api/schema.rs`, `api/stats.rs`, `api/knn.rs`
  to take `State(server): State<Arc<NexusServer>>` and read
  `server.engine` / `server.executor` directly.
- Delete the six OnceLock statics plus every `init_*` / `get_*` helper
  in those three modules.
- Drop the corresponding `api::stats::init_engine(...)` and
  `api::knn::init_executor(...)` calls from `nexus-server/src/main.rs`.
- Update the per-module tests to build an `Arc<NexusServer>` instead of
  initialising a global.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-server/src/api/schema.rs`
  - `nexus-server/src/api/stats.rs`
  - `nexus-server/src/api/knn.rs`
  - `nexus-server/src/main.rs` (init calls)
  - Any integration test that imports the `init_*` helpers of those modules
- Breaking change: NO — routing and HTTP payload shapes unchanged.
- User benefit: same as phase2a — parallel-safe tests, one state handle
  to reason about.
