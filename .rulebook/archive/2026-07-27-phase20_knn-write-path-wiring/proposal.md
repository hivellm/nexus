# Proposal: phase20_knn-write-path-wiring

## Why

Native vector search (KNN/HNSW) is the project's headline feature ("first-class
HNSW KNN indexes", RAG use cases), but it is **not functional end-to-end**: no
user write path populates the HNSW index. The insertion primitive
`KnnIndex::add_vector` (crates/nexus-core/src/index/knn_index.rs:149) has **no
production caller** — only benchmarks and unit tests. The code itself documents
the gap at `crates/nexus-core/src/engine/crud/index_maintenance.rs:439-446`
("no CREATE/SET path maintains the KNN index"), and `knn_evict_node`
(index_maintenance.rs:452) exists but is wired into nothing.

Consequences observed on a running 3.0.0 server:
- HTTP `/knn_traverse` (crates/nexus-server/src/api/knn.rs) is an MVP **stub**:
  the `vector` field is `#[allow(dead_code)]`, never used; it runs
  `MATCH (n:Label) RETURN n` and **fabricates** scores
  (`1.0 - i*0.1`, knn.rs:115) — no HNSW involvement.
- RPC `KNN_SEARCH` and RESP3 `KNN.SEARCH` query the real HNSW correctly, but the
  index is always empty, so they always return `[]`.

Net: a user cannot run a real vector search that returns ranked results by any
API. The engine *has* a working HNSW (cosine, `search_knn`) — only the write-path
wiring is missing.

## What Changes

Mirror the already-working FTS and spatial auto-populate pattern to maintain the
HNSW index from the write path, plus an explicit DDL registration and a
label-aware query:

1. Explicit DDL: `CREATE VECTOR INDEX <name> FOR (n:Label) ON n.prop`
   (mirror of the existing `SPATIAL`/`USING RTREE` handling — no magic property
   name, consistent with FTS/spatial which are registry-driven).
2. Write-path hooks (`knn_autopopulate_node` / `knn_refresh_node` /
   `knn_evict_node`) wired into CREATE / SET / DELETE at both the engine level
   (WAL-emitting) and the executor level (in-memory only), exactly as FTS/spatial
   split responsibilities today.
3. A `VectorIndexRegistry` (definitions only) mirroring `RTreeRegistry`;
   single global HNSW graph for v1, one active vector index at a time.
4. Fix the ignored `label` in `Engine::knn_search` — post-filter HNSW hits
   against the label bitmap so `KNN_SEARCH(label, ...)` is label-scoped.
5. Fix the HTTP `/knn_traverse` stub to consult the real index (or route it
   through the real query path) so no public surface fabricates scores.
6. Restart-durability check: verify the HNSW graph survives a restart; if the
   index has no boot replay/rebuild (a pre-existing gap suspected for FTS/spatial
   too), add a boot-time rebuild that re-adds vectors from the registered
   (label, property).

Design decisions (dimension fixed at 128 for v1, single global graph, WAL
entries) are recorded in the blueprint referenced in tasks.md task 1.

## Impact
- Affected specs: docs/specs/knn-integration.md (query + write path + DDL),
  docs/specs/cypher-subset.md (CREATE VECTOR INDEX), CLAUDE.md / README vector
  claims.
- Affected code: crates/nexus-core/src/{wal/record.rs, index/knn_registry.rs (new),
  index/mod.rs, executor/parser/clauses/admin.rs, executor/operators/admin.rs,
  executor/engine.rs, engine/crud/index_maintenance.rs, engine/crud/nodes.rs,
  engine/crud/lookup.rs, executor/operators/procedures/knn_procs.rs (new),
  executor/operators/create.rs, engine/maintenance.rs};
  crates/nexus-server/src/api/knn.rs (fix stub).
- Breaking change: NO (new DDL + additive write-path maintenance; existing
  queries unchanged). Behavioral: KNN queries start returning real results.
- User benefit: native vector search actually works end-to-end — the RAG/vector
  headline becomes true.
