# Proposal: Full-Text Search Index (Tantivy) + `db.index.fulltext.*`

## Why

Full-text indexing has been pinned "V1 planned" in the roadmap for a
year and is still at 0% implementation. Two concrete consequences:

1. `CONTAINS`, `STARTS WITH`, `ENDS WITH` on non-trivial text columns
   are full-store scans. At > 100k nodes with > 1 KB text per node
   this is uncompetitive (seconds, not milliseconds).
2. RAG workloads — the explicit Nexus target use case — need lexical
   ranking to complement dense vector retrieval. Without FTS, hybrid
   retrieval (dense + sparse) is impossible inside the engine.

Every competitor (Neo4j, ArangoDB, Dgraph) ships an FTS index with
BM25 ranking as table stakes. openCypher exposes this surface through
the `db.index.fulltext.*` procedure namespace:

```cypher
CALL db.index.fulltext.createNodeIndex('movies', ['Movie'], ['title','overview'])
CALL db.index.fulltext.queryNodes('movies', 'star wars') YIELD node, score
CALL db.index.fulltext.drop('movies')
```

Nexus has none of these.

## What Changes

- **Dependency**: add `tantivy = "0.22"` (Apache-2.0). Tantivy is the
  mature Rust equivalent of Lucene; it handles analysis, segmentation,
  BM25, phrase queries, and concurrent readers natively.
- **New index kind**: `FullText` joins `Bitmap`, `Btree`, `Vector`,
  `RTree` in the catalogue.
- **Per-index Tantivy directory** under
  `<data_dir>/fulltext/<index_name>/` managed by a dedicated writer
  task (single-writer Tantivy constraint).
- **WAL integration**: index updates are journalled as
  `OP_FTS_ADD { index, node_id, fields }` / `OP_FTS_DEL`. Crash
  recovery replays from WAL into a fresh Tantivy index (simpler than
  trying to keep Tantivy's own log consistent with Nexus's).
- **Procedure surface**:
  - `db.index.fulltext.createNodeIndex(name, labels, properties, config?)`
  - `db.index.fulltext.createRelationshipIndex(...)`
  - `db.index.fulltext.queryNodes(name, query) -> (node, score)`
  - `db.index.fulltext.queryRelationships(name, query) -> (relationship, score)`
  - `db.index.fulltext.drop(name)`
  - `db.index.fulltext.awaitEventuallyConsistentIndexRefresh()`
- **Cypher integration**: no new grammar — procedures are first-class
  row sources, so `CALL db.index.fulltext.queryNodes(...) YIELD node`
  plugs into MATCH queries.
- **Analyzer config**: whitespace, simple, standard, ngram, keyword —
  compatible with Neo4j's analyzer catalogue.
- **Refresh policy**: writes are eventually consistent with a default
  refresh interval of 1 second; configurable per-index.

**BREAKING**: none. All additions live in the new `db.index.fulltext`
namespace; existing scans and string predicates still work.

## Impact

### Affected Specs

- NEW capability: `index-fulltext`
- NEW capability: `procedures-db-index-fulltext`
- MODIFIED capability: `index-catalogue` (adds FULLTEXT index type)

### Affected Code

- `Cargo.toml` (root): add `tantivy = "0.22"`
- `nexus-core/src/index/fulltext/mod.rs` (NEW, ~400 lines, catalogue)
- `nexus-core/src/index/fulltext/writer.rs` (NEW, ~350 lines, writer task)
- `nexus-core/src/index/fulltext/query.rs` (NEW, ~300 lines, Lucene-like parser)
- `nexus-core/src/wal/fts_ops.rs` (NEW, ~150 lines)
- `nexus-core/src/procedures/fulltext/mod.rs` (NEW, ~500 lines)
- `nexus-core/tests/fulltext_tck.rs` (NEW, ~600 lines)

### Dependencies

- Requires: `phase6_opencypher-system-procedures` (so `db.indexes()`
  lists FTS indexes with the correct schema).
- Unblocks: hybrid retrieval — combining FTS scores with HNSW
  similarity in a single query (roadmap V2 item).

### Timeline

- **Duration**: 3–4 weeks
- **Complexity**: Medium — Tantivy does the heavy lifting; most work
  is lifecycle management, WAL replay correctness, and query parser
  alignment with Neo4j's Lucene query syntax.
- **Risk**: Low — Tantivy is battle-tested (Quickwit, Meilisearch).
