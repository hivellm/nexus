# Implementation Tasks — Nexus vs Neo4j Benchmark Suite

## 1. Crate Scaffolding

- [ ] 1.1 Create `nexus-bench` crate in workspace
- [ ] 1.2 Add dependencies: `criterion`, `neo4rs`, `rusqlite`, `serde_json`
- [ ] 1.3 Workspace integration: `cargo bench --package nexus-bench` entrypoint
- [ ] 1.4 Smoke test: empty scenario runs end-to-end in both engines
- [ ] 1.5 README with quickstart

## 2. Docker Harness

- [ ] 2.1 `scripts/bench/docker-compose.yml` for Neo4j Community 5.15
- [ ] 2.2 Isolated ports (7687 → 17687) and data dir
- [ ] 2.3 Config: cache 512 MiB, no TLS, no external metrics
- [ ] 2.4 `scripts/bench/run.sh` orchestrates: start container, run suite, tear down
- [ ] 2.5 Pin image digest for reproducibility

## 3. Engine Clients

- [ ] 3.1 `nexus_client.rs` — in-process embedded engine driver
- [ ] 3.2 `neo4j_client.rs` — Bolt driver via `neo4rs`
- [ ] 3.3 Shared trait `BenchClient` with `execute`, `reset`, `load_dataset`
- [ ] 3.4 Optional REST-mode client for diagnostic runs
- [ ] 3.5 Connection pooling, keep-alive tuning, warmup primitives

## 4. Dataset: `micro`

- [ ] 4.1 Generator — 10k nodes, 50k rels, deterministic seed
- [ ] 4.2 5 labels (A, B, C, D, E), 3 properties each
- [ ] 4.3 Load primitives for both engines
- [ ] 4.4 SHA-256 hash of dump committed in `tests/data/bench/`

## 5. Dataset: `social` (LDBC SNB)

- [ ] 5.1 Integrate `ldbc-snb-datagen` via Docker for sf=0.1
- [ ] 5.2 Cache generated dump under `tests/data/bench/social/`
- [ ] 5.3 Load primitives (LOAD CSV for Neo4j, direct import for Nexus)
- [ ] 5.4 Sanity check: row counts match across engines

## 6. Dataset: `vector`

- [ ] 6.1 Generate 100k nodes with 384-dim random embeddings
- [ ] 6.2 10 labels, random assignment
- [ ] 6.3 Companion full-text corpus (Wikipedia lead paragraphs)
- [ ] 6.4 Load primitives for both engines

## 7. Scenario Harness

- [ ] 7.1 Scenario struct: `id, dataset, query, params, warmup_iters, measured_iters, timeout_ms`
- [ ] 7.2 Execution loop with warmup, measurement, CPU-pinned worker
- [ ] 7.3 Row-count parity check: divergence raises `ERR_BENCH_OUTPUT_DIVERGENCE`
- [ ] 7.4 Metrics collection: p50, p95, p99, throughput, RSS delta
- [ ] 7.5 Timeout handling with graceful engine recovery

## 8. Scenarios — Scalar Functions

- [ ] 8.1 One scenario per numeric function (abs, ceil, floor, round, sqrt, pow, trig, etc.)
- [ ] 8.2 One per string function (substring, replace, toUpper, split, regex)
- [ ] 8.3 One per temporal function (date parsing, arithmetic, truncation)
- [ ] 8.4 One per type-check and type-conversion function
- [ ] 8.5 One per `bytes.*` function (once advanced-types ships)

## 9. Scenarios — Aggregations & Point Reads

- [ ] 9.1 COUNT, SUM, AVG, MIN, MAX, COLLECT across small and large windows
- [ ] 9.2 percentileCont, percentileDisc, stDev, stDevP
- [ ] 9.3 Point read by node id
- [ ] 9.4 Point read by indexed property (B-tree)
- [ ] 9.5 Label scan with and without property filter

## 10. Scenarios — Traversals

- [ ] 10.1 1-hop neighbour lookup, fixed label
- [ ] 10.2 2-hop friend-of-friend
- [ ] 10.3 Variable-length path `*1..3`
- [ ] 10.4 Quantified path pattern `{1,5}`
- [ ] 10.5 shortestPath and allShortestPaths
- [ ] 10.6 BFS vs DFS expansion via `apoc.path.expand`
- [ ] 10.7 MATCH with multiple patterns and cartesian join

## 11. Scenarios — Writes

- [ ] 11.1 Single-node CREATE
- [ ] 11.2 Batched CREATE via UNWIND (1k, 10k rows)
- [ ] 11.3 MERGE with and without existing match
- [ ] 11.4 SET property and `SET +=` map merge
- [ ] 11.5 DELETE and DETACH DELETE
- [ ] 11.6 Bulk ingest via `CALL {} IN TRANSACTIONS OF 10000 ROWS`
- [ ] 11.7 Concurrent writes (baseline under 4 parallel clients)

## 12. Scenarios — Indexes

- [ ] 12.1 Bitmap label scan vs full scan
- [ ] 12.2 B-tree equality and range seeks
- [ ] 12.3 Composite B-tree prefix seek
- [ ] 12.4 HNSW KNN queries (k=1, k=10, k=100)
- [ ] 12.5 R-tree `withinBBox`, `withinDistance`, nearest-neighbour
- [ ] 12.6 Full-text single-term, phrase, fuzzy queries
- [ ] 12.7 Index build times (cold and warm)

## 13. Scenarios — Constraints

- [ ] 13.1 UNIQUE check overhead on insert
- [ ] 13.2 NOT NULL check overhead on insert and SET
- [ ] 13.3 NODE KEY composite check
- [ ] 13.4 Property-type check
- [ ] 13.5 Backfill validator throughput (rows/sec)

## 14. Scenarios — Subqueries

- [ ] 14.1 `EXISTS { }` predicate
- [ ] 14.2 `COUNT { }` subquery
- [ ] 14.3 `COLLECT { }` subquery
- [ ] 14.4 Nested `CALL { }` 3-deep
- [ ] 14.5 `CALL { } IN TRANSACTIONS` throughput

## 15. Scenarios — Procedures

- [ ] 15.1 `db.labels`, `db.indexes`, `db.constraints` latency
- [ ] 15.2 `dbms.procedures`, `dbms.components` latency
- [ ] 15.3 `apoc.coll.*` representative set (union, sort, flatten)
- [ ] 15.4 `apoc.map.*` merge, groupBy
- [ ] 15.5 `apoc.text.*` fuzzy similarity, regex
- [ ] 15.6 `apoc.path.expand` vs native variable-length match
- [ ] 15.7 `apoc.periodic.iterate` bulk throughput
- [ ] 15.8 `gds.*` pageRank, centrality, pathfinding

## 16. Scenarios — Temporal & Spatial

- [ ] 16.1 `date.format`, `duration.between`, `date.truncate`
- [ ] 16.2 `point.distance` WGS-84 and Cartesian
- [ ] 16.3 Spatial `withinDistance` with and without R-tree
- [ ] 16.4 Full-text multi-language tokenisation

## 17. Scenarios — Hybrid / RAG

- [ ] 17.1 Vector KNN + graph traversal combined
- [ ] 17.2 Full-text + vector re-ranking
- [ ] 17.3 Graph + spatial + temporal combined (geofencing over time)
- [ ] 17.4 Neo4j parity: same queries running against both engines

## 18. Reporting

- [ ] 18.1 `report/markdown.rs` emits grouped tables with latency + ratio
- [ ] 18.2 `report/json.rs` machine-readable output
- [ ] 18.3 `report/sqlite_trace.rs` append-only history store
- [ ] 18.4 CLI flag `--output <markdown|json|sqlite|all>`
- [ ] 18.5 Classification rendering (⭐/✅/⚠️/🚨) in the markdown report

## 19. Performance Budget

- [ ] 19.1 `bench/budget.toml` schema with per-scenario limits
- [ ] 19.2 `nexus-bench check-budget` command compares report to budget
- [ ] 19.3 Exit code 1 on violation, 0 on pass
- [ ] 19.4 Override mechanism: label `perf-exception` in CI gate
- [ ] 19.5 Budget unit tests

## 20. CI Gate

- [ ] 20.1 `.github/workflows/bench.yml` runs on `perf-check` label
- [ ] 20.2 Baseline comparison from `bench/baselines/v<N>.json`
- [ ] 20.3 Fail if > 5% scenarios regress ≥ 20% vs baseline
- [ ] 20.4 Publish report as PR comment via `gh` CLI
- [ ] 20.5 Weekly scheduled run on main branch updates baseline

## 21. Methodology & Reproducibility

- [ ] 21.1 CPU pinning via `taskset` on Linux runners
- [ ] 21.2 Warmup iterations tuned so variance < 5% in CI
- [ ] 21.3 Disable ASLR, turbo boost, hyperthreading where possible
- [ ] 21.4 Collect `/proc/cpuinfo`, kernel version, Docker version in reports
- [ ] 21.5 Publish methodology in `docs/benchmarks/METHODOLOGY.md`

## 22. Output Divergence Guard

- [ ] 22.1 Every scenario declares expected row count or checksum
- [ ] 22.2 Divergence raises explicit error, not silent pass
- [ ] 22.3 Divergence report includes first 10 differing rows
- [ ] 22.4 Tests for the divergence guard itself

## 23. Parity-Report Automation

- [ ] 23.1 Generator consumes bench `report.json`
- [ ] 23.2 Produces `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` section
- [ ] 23.3 Numbers become reproducible artefacts instead of manual edits
- [ ] 23.4 CI step ensures report is up to date

## 24. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 24.1 Write `docs/benchmarks/README.md` and `METHODOLOGY.md`
- [ ] 24.2 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with numbers
- [ ] 24.3 Add CHANGELOG entry "Added Nexus vs Neo4j benchmark suite"
- [ ] 24.4 Update or create documentation covering the implementation
- [ ] 24.5 Write tests covering the new behavior
- [ ] 24.6 Run tests and confirm they pass
- [ ] 24.7 Quality pipeline: fmt + clippy + ≥95% coverage on the bench crate itself
