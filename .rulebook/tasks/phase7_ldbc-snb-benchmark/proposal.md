# Proposal: phase7_ldbc-snb-benchmark

## Why
Nexus's performance claims (100K+ point reads/sec, <1ms p95, "vs Neo4j" numbers) come from
ad-hoc in-house scripts (`scripts/benchmark/benchmark-nexus-vs-neo4j*.ps1`). The industry
standard for graph-database performance is the **LDBC Social Network Benchmark (SNB)
Interactive workload** — a defined schema, generated datasets at known scale factors, a fixed
operation mix (14 complex reads IC1–IC14, 7 short reads IS1–IS7, 8 updates), and standardized
throughput/latency reporting. Neo4j and Memgraph both publish SNB numbers; without it Nexus
performance is not externally comparable. SNB is also an excellent openCypher stress test — its
queries exercise multi-hop traversal, shortestPath, ordering with ties, aggregation, and
concurrent updates in patterns our unit suites do not.

Scope decision (stated assumption): implement the **Interactive workload** (not the BI
workload) with an **in-repo Rust harness driving the REST API**, at SF0.1 (CI smoke) and SF1
(regular runs), SF10 manual. An LDBC-*audited* run requires the official Java driver, which is
blocked on the missing JVM SDK (tracked as an ecosystem gap); this task produces
LDBC-*compatible* (unaudited) results and leaves the official-driver connector as explicit
follow-up.

## What Changes
- New `benchmarks/ldbc-snb/` directory: dataset fetch/generation scripts, Nexus bulk loader,
  the 29 Interactive queries as Cypher files, a Rust bench-driver crate, and a results report.
- Dataset: use LDBC's pre-generated SNB Interactive datasets (CSV, composite-merged layout) for
  SF0.1/SF1 instead of running the Spark Datagen; a download script pins URLs + checksums.
- Loader: stream the CSVs into Nexus via the existing `/ingest` bulk endpoint (nodes:
  Person/Forum/Post/Comment/Place/Tag/TagClass/Organisation; edges: KNOWS/HAS_MEMBER/LIKES/
  REPLY_OF/etc.), creating the B-tree indexes the queries need before measurement.
- Queries: port the Cypher reference implementations from `ldbc/ldbc_snb_interactive_impls`
  (Neo4j flavor) to Nexus's Cypher subset; any query that cannot be expressed becomes a
  documented compatibility finding feeding phase7_opencypher-gap-closure (never silently
  dropped or simplified).
- Driver: small Rust harness (workspace-external crate under `benchmarks/ldbc-snb/driver/`)
  that replays the official operation mix with substitution parameters from the dataset,
  measures per-query p50/p95/p99 latency + aggregate throughput, and emits JSON + Markdown.
- Baseline: same dataset + reference queries against Neo4j via the existing
  `scripts/bench/docker-compose.yml`, producing a side-by-side table.
- Report: `docs/performance/LDBC_SNB_REPORT.md` (+ CI smoke job at SF0.1 gated behind a
  manual/nightly trigger, not per-PR).

## Impact
- Affected specs: none existing; adds `benchmarks/ldbc-snb/README.md` as the canonical how-to
- Affected code: new code only — `benchmarks/ldbc-snb/` (loader, queries, driver crate, scripts);
  touches `scripts/bench/docker-compose.yml` reuse for the Neo4j baseline; no engine changes
  (engine gaps found by SNB queries are filed against phase7_opencypher-gap-closure, not hacked
  around here)
- Breaking change: NO
- User benefit: externally comparable, reproducible performance numbers on the industry-standard
  graph benchmark; a realistic mixed read/write stress suite that guards against performance and
  compatibility regressions
