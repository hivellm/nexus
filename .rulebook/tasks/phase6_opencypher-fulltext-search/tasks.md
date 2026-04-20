# Implementation Tasks — Full-Text Search

## 1. Dependency & Directory Layout

- [ ] 1.1 Add `tantivy = "0.22"` to workspace Cargo.toml
- [ ] 1.2 Define directory layout `<data_dir>/fulltext/<index_name>/`
- [ ] 1.3 Add lifecycle hooks to the storage layer for index dir cleanup on drop
- [ ] 1.4 Integration smoke test: create + query + drop on a tmp dir

## 2. FTS Catalogue

- [ ] 2.1 Add `FullText { id, name, entity, labels, properties, analyzer, config }` to the index catalogue
- [ ] 2.2 Persist FTS metadata in LMDB alongside existing indexes
- [ ] 2.3 Reject duplicate names (cross-kind unique)
- [ ] 2.4 Tests for catalogue CRUD

## 3. Writer Task

- [ ] 3.1 Per-index single-writer task (Tantivy requires exclusive writer)
- [ ] 3.2 Channel-based enqueue for adds/deletes from transaction commit
- [ ] 3.3 Periodic refresh driven by config `refresh_ms` (default 1000)
- [ ] 3.4 Graceful shutdown flushes remaining buffer before exit
- [ ] 3.5 Tests for refresh cadence and shutdown correctness

## 4. WAL Integration

- [ ] 4.1 Define WAL ops `OP_FTS_ADD`, `OP_FTS_DEL`, `OP_FTS_CREATE_INDEX`, `OP_FTS_DROP_INDEX`
- [ ] 4.2 Commit hook: on tx commit, enqueue add/del for each affected node
- [ ] 4.3 Crash recovery: replay WAL → rebuild Tantivy index
- [ ] 4.4 Tests including crash during bulk ingest

## 5. Query Parser

- [ ] 5.1 Support Lucene-like syntax: terms, phrases, `+/-`, `AND/OR/NOT`, field:value
- [ ] 5.2 Support fuzzy `term~`, prefix `term*`, range `[a TO z]`
- [ ] 5.3 Reject malformed queries with `ERR_FTS_PARSE`
- [ ] 5.4 Parser unit tests (50+ queries)

## 6. Analyzer Catalogue

- [ ] 6.1 Register analyzers: `whitespace`, `simple`, `standard`, `keyword`, `ngram`
- [ ] 6.2 Multi-language tokenisers for `standard` (via Tantivy's `StopWordFilter` per language)
- [ ] 6.3 `ngram` with configurable min/max gram size
- [ ] 6.4 Tests for tokenisation of each analyzer

## 7. Procedure Surface

- [ ] 7.1 `db.index.fulltext.createNodeIndex(name, labels, properties, config?)`
- [ ] 7.2 `db.index.fulltext.createRelationshipIndex(...)`
- [ ] 7.3 `db.index.fulltext.queryNodes(name, query)` returning `(node, score)`
- [ ] 7.4 `db.index.fulltext.queryRelationships(name, query)` returning `(rel, score)`
- [ ] 7.5 `db.index.fulltext.drop(name)`
- [ ] 7.6 `db.index.fulltext.awaitEventuallyConsistentIndexRefresh()`
- [ ] 7.7 Tests for each

## 8. Ranking & Scoring

- [ ] 8.1 Default scorer: BM25 (Tantivy default)
- [ ] 8.2 Expose `top_k` via config, default 100
- [ ] 8.3 Tie-break by node id ascending
- [ ] 8.4 Ranking regression tests against a known corpus (MS MARCO sample)

## 9. Integration with `db.indexes()`

- [ ] 9.1 FTS indexes appear in `db.indexes()` output
- [ ] 9.2 Columns: `type = "FULLTEXT"`, `indexProvider = "tantivy-0.22"`
- [ ] 9.3 Regression tests through the system procedures harness

## 10. Benchmarks

- [ ] 10.1 Bench: 100k documents × 1 KB each, single-term query < 5 ms p95
- [ ] 10.2 Bench: phrase query < 20 ms p95 on same corpus
- [ ] 10.3 Bench: indexing throughput > 5k docs/sec sustained
- [ ] 10.4 Bench harness committed and runnable via `cargo bench`

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 11.1 Add `docs/guides/FULL_TEXT_SEARCH.md`
- [ ] 11.2 Update `docs/specs/knn-integration.md` (hybrid retrieval section)
- [ ] 11.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 11.4 Add CHANGELOG entry "Added full-text search (Tantivy)"
- [ ] 11.5 Update or create documentation covering the implementation
- [ ] 11.6 Write tests covering the new behavior
- [ ] 11.7 Run tests and confirm they pass
- [ ] 11.8 Quality pipeline: fmt + clippy + ≥95% coverage
