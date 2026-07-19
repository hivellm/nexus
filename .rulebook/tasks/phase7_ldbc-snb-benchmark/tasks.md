# phase7_ldbc-snb-benchmark — tasks

Order matters: dataset → loader → queries validated for correctness → driver → baseline →
report. Do not benchmark before every query returns validated-correct results (a fast wrong
answer is worthless). Engine gaps discovered along the way are FILED against
phase7_opencypher-gap-closure with a repro query — never worked around by simplifying the
benchmark query.

## 1. Implementation
- [x] 1.1 Scaffold `benchmarks/ldbc-snb/` (README with scope statement: Interactive workload, unaudited, REST harness) + `fetch-dataset.sh`/`.ps1` downloading LDBC pre-generated SNB Interactive CSVs for SF0.1 and SF1 with pinned URLs and SHA-256 checksums, cached outside git
      Done: `dataset-manifest.tsv` pins 6 artifacts (SF0.1 + SF1 × dataset/parameters/updates) from
      `datasets.ldbcouncil.org`, serializer CsvCompositeMergeForeign + LongDateFormatter, with SHA-256
      computed locally (LDBC publishes no checksum file for Interactive v1). Both fetch scripts read that
      one manifest; cached archives are always re-hashed — no flag skips verification. `--force` controls
      re-extraction only. zstd via CLI or Python `zstandard` fallback. Cache defaults to `~/.cache/ldbc-snb`,
      overridable via `$LDBC_SNB_CACHE_DIR`; `.gitignore` is a safety net for `--cache` pointed at the repo
      (deliberately not a blanket `*.csv`, so loader test fixtures stay tracked). Verified end-to-end on
      SF0.1 and SF1, both scripts, including idempotent re-runs and `--verify-only`. README records the
      scope statement, measured disk footprint (SF0.1 135 MiB / SF1 1.5 GiB), SF0.1 cardinalities
      (327 588 nodes / 576 896 edge rows) and the merge-foreign FK→edge mapping the loader must synthesize.
- [x] 1.2 Schema prep script: Cypher DDL creating the SNB label indexes + B-tree property indexes (Person.id, Post.id, Comment.id, Forum.id, Place.name, Tag.name, Organisation.id, creationDate fields) — run against a fresh Nexus database before load
      Done: `schema/indexes.cypher` (15 property indexes: 8 id, 3 name incl. TagClass.name for IC12,
      4 creationDate) + `schema/create-schema.sh`. Label indexes are NOT declared — Nexus maintains a
      RoaringBitmap per label automatically, so only property indexes need DDL. All 15 statements verified
      executing against a live server. Three engine gaps found and FILED, not worked around:
      `SHOW INDEXES` unimplemented (gap-closure 4.6), negative numeric literals rejected in CREATE property
      maps (gap-closure 4.7), and REST database routing entirely non-functional — `database` field parsed
      and never read, `PUT /session/database` reports success without switching (new task
      phase19_fix-cypher-database-routing, HIGH: cross-database leakage). Because of the last one the
      script deliberately has NO `--database` flag; the harness assumes one database per server process.
      Coverage is proven via the planner's `UnindexedPropertyAccess` notification since `SHOW INDEXES` is
      absent. First implementation of that check was VACUOUS — the notification needs rows to scan, so an
      empty database reported every index present even with none created. Fixed by creating one throwaway
      node per label, probing, then deleting and asserting removal. Validated with both controls:
      Person.firstName (no index) → MISS, Person.id (indexed) → silent.
- [ ] 1.3 Bulk loader (`benchmarks/ldbc-snb/loader/`, Rust bin or Python script): stream the composite-merged CSVs into Nexus via `/ingest` — all 8 node labels first, then all edge files with date/datetime coercion; verify post-load node/edge counts against the dataset's expected cardinalities and fail loudly on mismatch
- [ ] 1.4 Port short reads IS1–IS7 from `ldbc/ldbc_snb_interactive_impls` (cypher flavor) into `benchmarks/ldbc-snb/queries/`, one file per query with parameter placeholders; smoke-validate each against SF0.1 comparing results with the same query on Neo4j (docker via `scripts/bench/docker-compose.yml`)
- [ ] 1.5 Port complex reads IC1–IC14 the same way, validating each against Neo4j on SF0.1; each query Nexus cannot express or answers differently → file a finding (repro + expected vs actual) in phase7_opencypher-gap-closure and mark the query BLOCKED in the README table
- [ ] 1.6 Port the 8 Interactive updates (INS1–INS8: add person/like/post/comment/forum/membership/friendship/reply) with parameter streams from the dataset's update CSVs; validate side effects (counts before/after) on SF0.1
- [ ] 1.7 Bench driver crate (`benchmarks/ldbc-snb/driver/`, Rust, NOT in the main workspace): loads parameter substitution files, replays the official Interactive operation mix (frequency ratios per query type, configurable client concurrency), measures per-query p50/p95/p99 + overall throughput (ops/s), warm-up phase excluded, emits `results/*.json` + Markdown summary
- [ ] 1.8 Neo4j baseline mode in the driver (Bolt or HTTP endpoint switch) so the identical run executes against the dockerized Neo4j; produce the side-by-side SF1 comparison
- [ ] 1.9 Run SF1 on both engines (3 runs, report median), write `docs/performance/LDBC_SNB_REPORT.md` (setup, hardware, per-query table, throughput, blocked-query list, caveat: unaudited/in-repo harness); add an SF0.1 smoke entry point (`benchmarks/ldbc-snb/smoke.sh`) wired as a manual/nightly CI job, not per-PR

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation (benchmarks/ldbc-snb/README.md how-to-run, docs/performance/LDBC_SNB_REPORT.md, link from docs/performance/PERFORMANCE_V1.md)
- [ ] 2.2 Write tests covering the new behavior (loader unit tests: CSV parsing/coercion/count verification; driver unit tests: mix scheduling, percentile math; query-correctness harness vs Neo4j at SF0.1)
- [ ] 2.3 Run tests and confirm they pass (loader + driver crate tests green, SF0.1 smoke end-to-end green, clippy `-D warnings` on new crates)

## Follow-ups (explicitly out of scope here)
- Official LDBC Java driver connector → blocked on JVM SDK (ecosystem gap, no task yet)
- SNB Business Intelligence workload
- SF10+ runs on dedicated hardware
