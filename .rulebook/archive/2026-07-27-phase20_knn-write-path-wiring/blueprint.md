# KNN write-path wiring — implementation blueprint (v1)

Goal: make native HNSW vector search work end-to-end for a user. Engine already has a
working `KnnIndex` (crates/nexus-core/src/index/knn_index.rs: `add_vector`:149,
`search_knn`:225, cosine). Query surfaces exist (RPC `KNN_SEARCH`, RESP3 `KNN.SEARCH`)
but the index is NEVER populated by any user write path. Mirror the FTS/spatial pattern.

## Design decisions (fixed — do not re-litigate)
- **A. Explicit DDL registration**, not a magic property name. `CREATE VECTOR INDEX name FOR (n:Label) ON n.prop`.
  Reuse `parse_create_index_clause` `index_type: Option<String>` with value `"vector"` (mirror `"spatial"`),
  recognized by `VECTOR INDEX` and `USING VECTOR`. No new AST fields.
- **B. Dimension fixed at DEFAULT_VECTORIZER_DIMENSION (128)** for v1. No OPTIONS parsing. Dim mismatch →
  `tracing::warn!` + skip (same error-containment as FTS/spatial), never abort the CREATE.
- **C. Single GLOBAL HNSW graph** for v1 (not per-label). Add a `VectorIndexRegistry` that stores only
  definitions `(name,label,property)`. Only ONE active vector index allowed (register errs on second unless
  OR REPLACE → `KnnIndex::clear()`). BUT fix the ignored `_label`: `Engine::knn_search` must resolve
  label→label_id and post-filter HNSW hits against `label_index.get_nodes(label_id)`.
- **D. WAL entries** `KnnVectorAdd{node_id,embedding}` / `KnnVectorDelete{node_id}` emitted by ENGINE-level
  hooks only (never executor-level — same split as spatial: executor crate has no WAL handle).

## Call sites (mirror these exactly)
- Engine CREATE: crates/nexus-core/src/engine/crud/nodes.rs:256 (`fts_autopopulate_node`), :260 (`spatial_autopopulate_node`) inside `create_node_inner`.
- Engine SET/update: crates/nexus-core/src/engine/crud/lookup.rs:95 (`fts_refresh_node`), :98 (`spatial_refresh_node`) inside `persist_node_state`.
- Engine DELETE: crates/nexus-core/src/engine/crud/nodes.rs:474 (`fts_evict_node`), :477 (`spatial_evict_node`) inside `delete_node`.
- Engine hook impls: crates/nexus-core/src/engine/crud/index_maintenance.rs — `fts_autopopulate_node`:190, `spatial_autopopulate_node`:274, `fts_refresh_node`:23, `spatial_refresh_node`:327, `fts_evict_node`:414, `spatial_evict_node`:394. `knn_evict_node` ALREADY EXISTS at :452 (unwired). Docstring gap note at :439-446.
- Executor CREATE (Cypher): crates/nexus-core/src/executor/operators/create.rs — node create sites at :308/:311, :426/:431, :847/:848. Executor hook impls: executor/operators/procedures/fts.rs:37, spatial_procs.rs:29 (NO WAL — comment spatial_procs.rs:55-59).
- Executor shares the SAME KnnIndex Arc (engine/mod.rs:355/562/823-827 pass `&indexes.knn_index` clone; KnnIndex is Arc<RwLock>). Accessors executor/engine.rs:388-395 (`knn_index()`/`knn_index_mut()`). rtree_registry pattern: assigned at engine.rs:307, accessor:359.
- Query: Engine::knn_search engine/maintenance.rs:21 → IndexManager::knn_search index/mod.rs:89 (ignores `_label`) → KnnIndex::search_knn knn_index.rs:225.

## Ordered sub-tasks (1-2 files each, build/test after each)
1. WAL entries — wal/record.rs: add `KnnVectorAdd{node_id:u64,embedding:Vec<f32>}`, `KnnVectorDelete{node_id:u64}` to `WalEntry`, `WalEntryType`, `entry_type()` (mirror FtsAdd/RTreeInsert). Round-trip test.
2. Registry — NEW index/knn_registry.rs (`VectorIndexRegistry`, mirror index/rtree/registry.rs): register (rejects 2nd unless OR-REPLACE), definitions, contains, drop_index, indexes_containing. Wire into IndexManager (index/mod.rs: `pub mod knn_registry;`, field `knn_registry: Arc<VectorIndexRegistry>`, init in `new`). Unit tests.
3. DDL parse — executor/parser/clauses/admin.rs (:133-140,:233-253): recognize `VECTOR`/`USING VECTOR` → `index_type=Some("vector")`. Parser test.
4. DDL dispatch — executor/operators/admin.rs `execute_create_index`(:16-96): `Some("vector")=>` branch → `knn_registry().register(...)`.
5. Executor registry exposure — executor/engine.rs: `knn_registry: Arc<VectorIndexRegistry>` in ExecutorShared, assign at ~:307, accessor `knn_registry()` ~:359.
6. Engine hooks — engine/crud/index_maintenance.rs: `knn_autopopulate_node` (mirror spatial:274, extract Vec<f32> from Value::Array of finite numbers, `knn_index.add_vector`, emit KnnVectorAdd), `knn_refresh_node` (mirror spatial:327), extend `knn_evict_node`:452 to emit KnnVectorDelete + drop registry membership. Keep existing knn_evict_node_tests (:914-947) green.
7. Engine wiring — engine/crud/nodes.rs: `knn_autopopulate_node` after :260; `knn_evict_node` after :477.
8. Engine SET wiring — engine/crud/lookup.rs: `knn_refresh_node` after :98.
9. Executor hooks — NEW executor/operators/procedures/knn_procs.rs (mirror spatial_procs.rs:29, NO WAL) using knn_registry().definitions() + knn_index().
10. Executor wiring — executor/operators/create.rs: `knn_autopopulate_node` after each spatial call (:311,:431,:848).
11. Label-aware search — engine/maintenance.rs knn_search(:20-23): resolve label→label_id, post-filter hits vs label_index.get_nodes(label_id).
12. DROP VECTOR INDEX — admin.rs execute_drop_index companion: knn_registry().drop_index + knn_index.clear().

## Tests
- NEW crates/nexus-core/tests/knn/main.rs group (mirror tests/spatial/main.rs): knn_write_path_test.rs — CREATE→autopopulate→knn_search, SET→refresh, DELETE→evict, label filter, DDL round-trip (CREATE/IF NOT EXISTS/2nd-rejected/OR REPLACE/DROP).
- NEW crates/nexus-server/tests/knn_write_path_test.rs: RPC CREATE_NODE+KNN_SEARCH, RESP3 KNN.SEARCH, REST /cypher end-to-end.

## Open risk (verify)
Blueprint found NO generic boot replay loop consuming FtsAdd/RTreeInsert back into registries — FTS/spatial
crash recovery may be a pre-existing gap. So after implementing, TEST index survival across restart. If the
HNSW graph does NOT survive restart, add a boot-time rebuild that re-scans nodes carrying the registered
(label,property) and re-adds vectors. (Embeddings survive as properties regardless — they live in the property store.)
