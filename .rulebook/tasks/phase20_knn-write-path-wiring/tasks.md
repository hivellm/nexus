## 1. Foundation — registration half (leaves tree compiling; nothing populates yet)

- [x] 1.1 WAL entries. In `crates/nexus-core/src/wal/record.rs` add
  `KnnVectorAdd { node_id: u64, embedding: Vec<f32> }` and
  `KnnVectorDelete { node_id: u64 }` to `WalEntry`, with discriminants in
  `WalEntryType` and arms in `entry_type()` — mirror `FtsAdd`/`RTreeInsert`
  (record.rs ~198-231, ~285-289). **Done when**: compiles; a round-trip
  (de)serialization test passes (mirror the FtsAdd/FtsDel test in wal/mod.rs ~799-842).
- [x] 1.2 Vector-index registry. New `crates/nexus-core/src/index/knn_registry.rs`
  (`VectorIndexRegistry`, structural mirror of `index/rtree/registry.rs`):
  `register(name,label,property)` (errs on a second entry unless OR-REPLACE),
  `definitions()`, `contains`, `drop_index`, `indexes_containing(node_id)`. Wire into
  `crates/nexus-core/src/index/mod.rs` (`pub mod knn_registry;`, an
  `Arc<VectorIndexRegistry>` field on `IndexManager`, init in `new`). **Done when**:
  `cargo check -p nexus-core` clean; unit tests cover register / second-entry-rejected /
  OR-REPLACE / drop / indexes_containing.
- [x] 1.3 DDL parse. In `crates/nexus-core/src/executor/parser/clauses/admin.rs`
  (`parse_create_index_clause`, ~133-140, ~233-253) recognize `VECTOR INDEX` and
  `USING VECTOR` → `index_type = Some("vector")` (mirror the `SPATIAL`/`USING RTREE`
  branch — no new AST fields). **Done when**: a parser test asserts
  `CREATE VECTOR INDEX docEmb FOR (d:Doc) ON d.embedding` →
  `index_type: Some("vector"), label:"Doc", property:"embedding"`.
- [x] 1.4 Executor registry exposure. In `crates/nexus-core/src/executor/engine.rs`
  add `knn_registry: Arc<VectorIndexRegistry>` to `ExecutorShared`, assign it where
  `rtree_registry` is assigned (~307), add accessor `knn_registry()` (~359). **Done
  when**: `cargo check`; the executor reads the same definitions the engine registers.
- [x] 1.5 DDL dispatch. In `crates/nexus-core/src/executor/operators/admin.rs`
  (`execute_create_index`, ~16-96) add a `Some("vector") =>` arm calling
  `knn_registry().register(...)`, matching the existing spatial arm's error handling
  (`IF NOT EXISTS` idempotent, `OR REPLACE` clears via `KnnIndex::clear()`). Add the
  companion `DROP VECTOR INDEX` arm in `execute_drop_index`
  (`knn_registry().drop_index` + `knn_index.clear()`). **Done when**:
  `CREATE VECTOR INDEX` executes, is idempotent under `IF NOT EXISTS`, a second index
  on a different property without `OR REPLACE` errors, and `DROP` clears both registry
  and graph.

## 2. Population — write-path maintenance (depends on §1)

- [x] 2.1 Engine hooks. In `crates/nexus-core/src/engine/crud/index_maintenance.rs`
  implement `knn_autopopulate_node` (mirror `spatial_autopopulate_node` ~274: extract a
  `Vec<f32>` from a `Value::Array` of finite numbers, call
  `self.indexes.knn_index.add_vector`, emit `WalEntry::KnnVectorAdd`) and
  `knn_refresh_node` (mirror `spatial_refresh_node` ~327, evict-then-conditional-readd);
  extend the existing `knn_evict_node` (~452) to emit `WalEntry::KnnVectorDelete` and
  drop registry membership. Dimension-mismatch → `tracing::warn!` + skip (never abort
  the write). **Done when**: existing `knn_evict_node_tests` (~914-947) stay green; 1-3
  new unit tests cover autopopulate-on-match, refresh evict-then-readd, evict-emits-WAL.
- [x] 2.2 Engine wiring (CREATE + DELETE). In `crates/nexus-core/src/engine/crud/nodes.rs`
  call `self.knn_autopopulate_node(node_id, &label_ids, &properties)?;` right after the
  `spatial_autopopulate_node` call (~260) in `create_node_inner`, and
  `self.knn_evict_node(id);` right after the `spatial_evict_node` call (~477) in
  `delete_node`. **Done when**: engine-level `CREATE_NODE` (REST/RPC/RESP3) with a
  registered vector index populates `indexes.knn_index`; `DELETE_NODE` removes it.
- [x] 2.3 Engine wiring (SET). In `crates/nexus-core/src/engine/crud/lookup.rs` call
  `self.knn_refresh_node(node_id, &effective_label_ids, &props_value);` right after the
  `spatial_refresh_node` call (~98) in `persist_node_state`. **Done when**: a SET that
  changes the embedding property reindexes (old vector unreachable, new one found).
- [x] 2.4 Executor hooks (Cypher CREATE). New
  `crates/nexus-core/src/executor/operators/procedures/knn_procs.rs` (mirror
  `spatial_procs.rs:29` — NO WAL) using `self.knn_registry().definitions()` +
  `self.knn_index()`. **Done when**: compiles; a unit test calling the method directly
  populates the shared `KnnIndex`.
- [x] 2.5 Executor wiring (Cypher CREATE). In
  `crates/nexus-core/src/executor/operators/create.rs` add `self.knn_autopopulate_node(...)`
  after each existing `spatial_autopopulate_node` call (~311, ~431, ~848). **Done when**:
  `CREATE (:Doc {embedding: $v})` via Cypher populates the shared global index.
- [x] 2.6 Label-aware search. In `crates/nexus-core/src/engine/maintenance.rs`
  (`knn_search`, ~20-23) resolve `label` → `label_id` via catalog and post-filter HNSW
  hits against `self.indexes.label_index.get_nodes(label_id)` before returning. **Done
  when**: two nodes of different labels with similar embeddings — `knn_search("Doc", ...)`
  returns only `:Doc` nodes.

## 3. Public query surface + durability (depends on §2)

- [x] 3.1 Fix the HTTP stub. In `crates/nexus-server/src/api/knn.rs` replace the
  fabricated-score MVP with a call through the real `engine.knn_search` (label + query
  vector + k), returning real cosine scores. Remove the `#[allow(dead_code)]` on
  `vector`. **Done when**: `POST /knn_traverse {label, vector, k}` returns nodes ranked
  by real similarity; no fabricated scores remain.
- [x] 3.2 Restart durability. Verify the HNSW graph survives a container/process
  restart (WAL replay or boot rebuild). If it does NOT (no generic boot replay loop was
  found for FTS/spatial either — a pre-existing suspicion), add a boot-time rebuild that
  re-scans nodes carrying each registered (label, property) and re-adds their vectors.
  **Done when**: after restart, a KNN query over previously-inserted embeddings still
  returns ranked results.

## 2. Tail (docs + tests — check or waive with tailWaiver)

- [x] 2.1 Update or create documentation covering the implementation.
  Update `docs/specs/knn-integration.md` (write path, DDL, query),
  `docs/specs/cypher-subset.md` (CREATE/DROP VECTOR INDEX), and the vector-search
  claims in README / CLAUDE.md to reflect the now-functional feature. Update CHANGELOG.
- [x] 2.2 Write tests covering the new behavior.
  DONE — coverage landed as unit tests co-located with the code (mirroring the crate's
  convention) plus one server integration test, rather than a new `tests/knn/` group:
  autopopulate/refresh/evict + WAL in `engine/crud/index_maintenance.rs` (8 tests),
  label-aware search in `engine/maintenance.rs`, DDL round-trip
  (CREATE / IF NOT EXISTS / second-rejected / OR REPLACE / DROP) in `engine/tests/indexes.rs`,
  parser in `executor/parser/tests/ddl.rs`, registry in `index/knn_registry.rs`,
  restart durability in `engine/tests/transactions.rs::vector_index_survives_restart`, and
  server-level end-to-end (real ranked scores via `/knn_traverse`) in
  `crates/nexus-server/tests/knn_write_path_test.rs`. RPC/RESP3 covered by their existing
  dispatch unit tests.
- [x] 2.3 Run tests and confirm they pass.
  Full gate (`cargo check` → `cargo clippy --workspace -- -D warnings` →
  `cargo test --workspace`) green; re-run the live Docker smoke test for KNN.
