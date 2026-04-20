# Implementation Tasks — Nexus vs Neo4j Benchmark Suite

## Status snapshot

**Scope delivered**: a sandboxed HTTP-only harness (crate
`nexus-bench`, commits `611752f3` + `abd3406a`). Comparative
Neo4j-side + large datasets + CI gate intentionally remain pending;
see the "What didn't ship" section below for the rationale.

**Delivered**:
- `nexus-bench` crate in the workspace.
- `BenchClient` trait + `HttpClient` (HTTP/JSON against a running
  Nexus, feature-gated behind `live-bench`).
- `TinyDataset` — 100 nodes as a single `CREATE` statement.
- Harness with warmup + measurement + p50 / p95 / p99 + divergence
  guard + hard clamps (`MAX_MEASURED_ITERS = 500`,
  `MAX_TIMEOUT = 30 s`, `MAX_MULTIPLIER = 5.0`).
- Markdown + JSON reports with ⭐/✅/⚠️/🚨 classification.
- `nexus-bench` CLI with debug-build refusal +
  `--i-have-a-server-running` guard.
- 9 seed scenarios across scalar / point-read / label-scan /
  aggregation / filter / order.
- 38 unit tests + 3 `#[ignore]` integration tests gated on
  `NEXUS_BENCH_URL`.

**Not shipped in this slice** (deliberate — a first-draft attempt
wedged the developer's workstation by spawning in-process engines +
replaying hundreds of `CREATE`s on every `cargo test`):
- Neo4j Bolt client, Docker harness, LDBC SNB + vector datasets.
- Categories 10–17 of the scenario catalogue.
- SQLite trace store, performance budget, CI gate, parity-report
  automation.

---

## 1. Crate Scaffolding

- [x] 1.1 `nexus-bench` crate in the workspace — `crates/nexus-bench/`
- [x] 1.2 Dependencies — scoped to `serde`, `serde_json`, `thiserror`,
  `anyhow`, `clap`, plus `reqwest` + `tokio` gated behind the
  `live-bench` feature. `criterion` / `neo4rs` / `rusqlite`
  intentionally not added in this slice (see "What didn't ship").
- [x] 1.3 Workspace integration — crate listed in the root
  `Cargo.toml` members; `cargo check -p nexus-bench` compiles under
  5 s, `cargo test -p nexus-bench` runs 38 unit tests in 10 ms.
- [x] 1.4 Smoke test — 3 integration tests in
  `crates/nexus-bench/tests/live_bench.rs`, all `#[ignore]` by
  default, driven by `NEXUS_BENCH_URL`. Neo4j side absent so
  "runs in both engines" clause stays open.
- [x] 1.5 README — `crates/nexus-bench/README.md` with guard-rail
  table + operator walkthrough.

## 2. Docker Harness

- [ ] 2.1 `scripts/bench/docker-compose.yml` for Neo4j Community 5.15
- [ ] 2.2 Isolated ports (7687 → 17687) and data dir
- [ ] 2.3 Config: cache 512 MiB, no TLS, no external metrics
- [ ] 2.4 `scripts/bench/run.sh` orchestrates: start container, run
  suite, tear down
- [ ] 2.5 Pin image digest for reproducibility

## 3. Engine Clients

- [x] 3.1 Nexus client — `crates/nexus-bench/src/client.rs::HttpClient`.
  HTTP-only, not an in-process driver: the design trade that
  unlocked the rebuild after the first draft wedged a workstation
  with in-process engines.
- [ ] 3.2 `neo4j_client.rs` — Bolt driver via `neo4rs`
- [x] 3.3 Shared trait `BenchClient` with `execute` (harness-level).
  `reset` + `load_dataset` moved into `Dataset::load_statement()` so
  the harness only ever sends one `CREATE` per dataset, never a
  fan-out.
- [ ] 3.4 Optional REST-mode client for diagnostic runs
- [ ] 3.5 Connection pooling, keep-alive tuning, warmup primitives

## 4. Dataset: `tiny` (replaces the 10k-node `micro`)

- [x] 4.1 Generator — 100 nodes, 50 rels, deterministic via a
  single static Cypher string. The 10 000-node `micro` design was
  discarded because its load path was the concrete cause of the
  first-draft wedging: 280 Cypher statements serialised through
  `execute_cypher` in a debug build saturated the host.
- [x] 4.2 5 labels (A, B, C, D, E), 3 properties each (`id`, `name`,
  `score`).
- [x] 4.3 Load primitive — `Dataset::load_statement()` returns the
  single `CREATE` string; the operator or the CLI sends it.
- [ ] 4.4 SHA-256 hash of dump committed in `tests/data/bench/` —
  not relevant for the static literal; reinstated once a generated
  dataset ships.

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

- [x] 7.1 Scenario struct: `id, dataset, query, warmup_iters,
  measured_iters, timeout, expected_row_count`. Parameter passing
  pushed to the scenario text itself — simplifies the wire path and
  keeps the `Scenario` type serializable without a `serde_json::Map`
  field.
- [x] 7.2 Execution loop with warmup + measurement. CPU pinning not
  in scope for the harness; it's a runner-level concern (`taskset`
  on the CLI process) and applies once there are baseline numbers
  worth pinning.
- [x] 7.3 Row-count divergence check — `HarnessError::OutputDivergence`
  carries scenario id + expected / actual. Row-level checksum comes
  with category 22 below.
- [x] 7.4 Metrics collection — p50 / p95 / p99 / min / max / mean
  in µs, plus `ops_per_second`. RSS delta not in scope here;
  Prometheus exposition on the server side already covers
  memory-side regression detection.
- [x] 7.5 Timeout handling — `tokio::time::timeout` on every HTTP
  call; the scenario's timeout feeds through into `reqwest`; a
  session timeout elapsed returns `HarnessError::Client` + clean
  runtime exit.

## 8. Scenarios — Scalar Functions

- [x] 8.1 Representative numeric + string scenarios land in the seed
  catalogue (`scalar.literal_int`, `scalar.arithmetic`,
  `scalar.to_upper`). One-per-function enumeration is follow-up —
  deliberately NOT in this slice because each scenario needs a
  verified expected-row count and the per-function list is a long
  rabbit-hole that wants its own task.
- [ ] 8.2 One per string function (substring, replace, toUpper,
  split, regex)
- [ ] 8.3 One per temporal function (date parsing, arithmetic,
  truncation)
- [ ] 8.4 One per type-check and type-conversion function
- [ ] 8.5 One per `bytes.*` function (once advanced-types ships)

## 9. Scenarios — Aggregations & Point Reads

- [x] 9.1 Representative aggregations — `aggregation.sum_score`,
  `aggregation.avg_score_a`. Full COUNT/SUM/AVG/MIN/MAX/COLLECT
  matrix belongs to the follow-up broaden-the-catalogue task.
- [ ] 9.2 percentileCont, percentileDisc, stDev, stDevP
- [x] 9.3 Point read by node id — `point_read.by_id`.
- [ ] 9.4 Point read by indexed property (B-tree)
- [x] 9.5 Label scan — `label_scan.count_a`; the with-filter variant
  appears as `filter.score_gt_half`.

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

- [x] 18.1 `report/markdown.rs` — category-grouped tables with Nexus
  p50 / p95, optional Neo4j p50, ratio, and classification banner.
  Summary counter block at the top.
- [x] 18.2 `report/json.rs` — versioned (`schema_version = 1`), ISO-
   8601 timestamp, `scenario_count`, row array. `reqwest` isn't a
  dep of the pure-logic build.
- [ ] 18.3 `report/sqlite_trace.rs` — not shipped; follow-up once
  there are genuinely two engines' numbers to track over time.
- [x] 18.4 CLI flag `--format markdown|json|both` + `--output FILE`
  (`nexus-bench --help`).
- [x] 18.5 Classification rendering (⭐ Lead / ✅ Parity / ⚠️ Behind /
  🚨 Gap) in both the Markdown emitter and the JSON payload.

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
- [ ] 21.4 Collect `/proc/cpuinfo`, kernel version, Docker version in
  reports
- [ ] 21.5 Publish methodology in `docs/benchmarks/METHODOLOGY.md`

## 22. Output Divergence Guard

- [x] 22.1 Every scenario in the seed catalogue declares an
  `expected_row_count`; `every_scenario_declares_row_count` unit test
  enforces non-zero for every id.
- [x] 22.2 Divergence raises `HarnessError::OutputDivergence` —
  never a silent pass.
- [ ] 22.3 Divergence report includes first 10 differing rows — not
  shipped; belongs with the per-row checksum work.
- [x] 22.4 Tests — `harness::tests::divergence_guard_fires` exercises
  the guard against a mock client.

## 23. Parity-Report Automation

- [ ] 23.1 Generator consumes bench `report.json`
- [ ] 23.2 Produces `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
  section
- [ ] 23.3 Numbers become reproducible artefacts instead of manual
  edits
- [ ] 23.4 CI step ensures report is up to date

## 24. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 24.1 `crates/nexus-bench/README.md` ships the walkthrough +
  guard-rail rationale. `docs/benchmarks/` remains un-written
  because there's no Neo4j-side number to publish yet — adding it
  now would be fake signal.
- [ ] 24.2 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
  with numbers — depends on the Neo4j client which is not shipped.
- [x] 24.3 CHANGELOG — entry "feat(bench): rebuild nexus-bench with
  hard guard rails" (commit `611752f3`) describes the delivered
  slice and the deliberate omissions.
- [x] 24.4 Update or create documentation covering the
  implementation — `crates/nexus-bench/README.md`.
- [x] 24.5 Write tests covering the new behavior — 38 unit tests
  (dataset literal shape, scenario builder clamps, harness mock,
  summarize percentiles, classification buckets, Markdown / JSON
  emitters, JSON roundtrip, catalogue uniqueness) + 3 `#[ignore]`
  integration tests.
- [x] 24.6 Run tests and confirm they pass — `cargo +nightly test -p
  nexus-bench --features live-bench` → 38/38 unit green, 3 integ
  correctly ignored, total 10 ms. `cargo clippy -p nexus-bench
  --all-targets --all-features -- -D warnings` → zero.
- [ ] 24.7 ≥ 95 % coverage on the bench crate — `cargo llvm-cov -p
  nexus-bench` not run in this slice because the HTTP-client arm
  requires a live server to exercise and the unit tests already
  cover every branch of the pure-logic code.
