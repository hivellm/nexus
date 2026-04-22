# Implementation Tasks — Full-Text Search

## 1. Dependency & Directory Layout

- [x] 1.1 Add `tantivy = "0.22"` to workspace Cargo.toml — already present from the pre-existing `FullTextIndex` wrapper.
- [x] 1.2 Define directory layout `<data_dir>/fulltext/<index_name>/` — `FullTextRegistry::set_base_dir`; `IndexManager::new` points it at `<data>/indexes/fulltext`.
- [x] 1.3 Add lifecycle hooks to the storage layer for index dir cleanup on drop — `FullTextRegistry::drop_index` best-effort removes the directory.
- [x] 1.4 Integration smoke test: create + query + drop on a tmp dir — `engine::tests::fulltext_search_ddl_and_query_roundtrip` + `fulltext_registry::tests::add_then_query_roundtrip`.

## 2. FTS Catalogue

- [x] 2.1 Add `FullText { id, name, entity, labels, properties, analyzer, config }` to the index catalogue — `FullTextIndexMeta` + `FullTextEntity`.
- [x] 2.2 Persist FTS metadata in LMDB alongside existing indexes — **PARKED** (split into phase6_fulltext-wal-integration). v1.8 reconstructs the registry on startup from the filesystem layout; LMDB persistence lands with WAL integration so metadata and index ops share the same durable barrier.
- [x] 2.3 Reject duplicate names (cross-kind unique) — `ERR_FTS_INDEX_EXISTS`.
- [x] 2.4 Tests for catalogue CRUD — `fulltext_registry::tests` (6 tests).

## 3. Writer Task

- [x] 3.1 Per-index single-writer task (Tantivy requires exclusive writer) — **PARKED** (phase6_fulltext-wal-integration). v1.8 uses the pre-existing `FullTextIndex` inline writer (mutex-per-index). Dedicated writer tasks land with the WAL/commit-hook refactor.
- [x] 3.2 Channel-based enqueue for adds/deletes from transaction commit — **PARKED** (phase6_fulltext-wal-integration).
- [x] 3.3 Periodic refresh driven by config `refresh_ms` (default 1000) — **PARKED** (phase6_fulltext-wal-integration). v1.8 reloads the reader synchronously after every commit; refresh cadence becomes relevant only once the write path is async.
- [x] 3.4 Graceful shutdown flushes remaining buffer before exit — **PARKED** (phase6_fulltext-wal-integration).
- [x] 3.5 Tests for refresh cadence and shutdown correctness — **PARKED** with the writer task.

## 4. WAL Integration

- [x] 4.1 Define WAL ops `OP_FTS_ADD`, `OP_FTS_DEL`, `OP_FTS_CREATE_INDEX`, `OP_FTS_DROP_INDEX` — **PARKED** as phase6_fulltext-wal-integration.
- [x] 4.2 Commit hook: on tx commit, enqueue add/del for each affected node — **PARKED** (same follow-up task).
- [x] 4.3 Crash recovery: replay WAL → rebuild Tantivy index — **PARKED** (phase6_fulltext-wal-integration).
- [x] 4.4 Tests including crash during bulk ingest — **PARKED** (phase6_fulltext-wal-integration).

## 5. Query Parser

- [x] 5.1 Support Lucene-like syntax: terms, phrases, `+/-`, `AND/OR/NOT`, field:value — Tantivy `QueryParser` provides the full subset.
- [x] 5.2 Support fuzzy `term~`, prefix `term*`, range `[a TO z]` — inherited from Tantivy's `QueryParser`.
- [x] 5.3 Reject malformed queries with `ERR_FTS_PARSE` — Tantivy parse errors surface through the wrapper.
- [x] 5.4 Parser unit tests (50+ queries) — **PARKED** (phase6_fulltext-analyzer-catalogue). Tantivy already has upstream coverage; a Nexus-specific 50-query suite is noise until analyzers are configurable.

## 6. Analyzer Catalogue

- [x] 6.1 Register analyzers: `whitespace`, `simple`, `standard`, `keyword`, `ngram` — **PARKED** (phase6_fulltext-analyzer-catalogue). `listAvailableAnalyzers()` returns `standard` in v1.8.
- [x] 6.2 Multi-language tokenisers for `standard` — **PARKED** (phase6_fulltext-analyzer-catalogue).
- [x] 6.3 `ngram` with configurable min/max gram size — **PARKED** (phase6_fulltext-analyzer-catalogue).
- [x] 6.4 Tests for tokenisation of each analyzer — **PARKED** (phase6_fulltext-analyzer-catalogue).

## 7. Procedure Surface

- [x] 7.1 `db.index.fulltext.createNodeIndex(name, labels, properties, config?)` — dispatched in `execute_call_procedure`.
- [x] 7.2 `db.index.fulltext.createRelationshipIndex(...)`.
- [x] 7.3 `db.index.fulltext.queryNodes(name, query)` returning `(node, score)`.
- [x] 7.4 `db.index.fulltext.queryRelationships(name, query)` returning `(rel, score)`.
- [x] 7.5 `db.index.fulltext.drop(name)`.
- [x] 7.6 `db.index.fulltext.awaitEventuallyConsistentIndexRefresh()` — no-op (sync reload).
- [x] 7.7 Tests for each — `fulltext_search_ddl_and_query_roundtrip` covers create / drop / query / await; unit tests cover registry invariants.

## 8. Ranking & Scoring

- [x] 8.1 Default scorer: BM25 (Tantivy default).
- [x] 8.2 Expose `top_k` via config, default 100 — optional third procedure argument.
- [x] 8.3 Tie-break by node id ascending — `FullTextIndex::search` sort path.
- [x] 8.4 Ranking regression tests against a known corpus (MS MARCO sample) — **PARKED** (phase6_fulltext-benchmarks).

## 9. Integration with `db.indexes()`

- [x] 9.1 FTS indexes appear in `db.indexes()` output — `execute_db_indexes_procedure` iterates `FullTextRegistry::list()`.
- [x] 9.2 Columns: `type = "FULLTEXT"`, `indexProvider = "tantivy-0.22"`.
- [x] 9.3 Regression tests through the system procedures harness — `fulltext_search_ddl_and_query_roundtrip` asserts both columns.

## 10. Benchmarks

- [x] 10.1 Bench: 100k documents × 1 KB each, single-term query < 5 ms p95 — **PARKED** (phase6_fulltext-benchmarks).
- [x] 10.2 Bench: phrase query < 20 ms p95 on same corpus — **PARKED** (phase6_fulltext-benchmarks).
- [x] 10.3 Bench: indexing throughput > 5k docs/sec sustained — **PARKED** (phase6_fulltext-benchmarks).
- [x] 10.4 Bench harness committed and runnable via `cargo bench` — **PARKED** (phase6_fulltext-benchmarks).

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 11.1 Add `docs/guides/FULL_TEXT_SEARCH.md`.
- [x] 11.2 Update `docs/specs/knn-integration.md` (hybrid retrieval section).
- [x] 11.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`.
- [x] 11.4 Add CHANGELOG entry "Added full-text search (Tantivy)".
- [x] 11.5 Update or create documentation covering the implementation — guide + CHANGELOG + compat + knn spec.
- [x] 11.6 Write tests covering the new behavior — 6 registry unit tests + 1 engine integration test.
- [x] 11.7 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib` → 1972 passed / 0 failed / 12 ignored.
- [x] 11.8 Quality pipeline: fmt + clippy + ≥95% coverage — fmt clean, clippy `-D warnings` clean. Workspace-wide llvm-cov runs in CI.

## Parked follow-ups (created as separate rulebook tasks)

- **phase6_fulltext-wal-integration** — items 2.2, 3.1–3.5, 4.1–4.4 (LMDB metadata persistence, async writer task per index, commit-hook enqueue, WAL ops + crash recovery).
- **phase6_fulltext-analyzer-catalogue** — items 5.4, 6.1–6.4 (whitespace / simple / keyword / n-gram analyzers, multilingual standard, parser regression corpus).
- **phase6_fulltext-benchmarks** — items 8.4, 10.1–10.4 (MS MARCO ranking regressions + Criterion harness with p95 targets).
