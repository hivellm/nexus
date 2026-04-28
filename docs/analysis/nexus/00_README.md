# Nexus — System Analysis

**Date:** 2026-04-28
**Baseline:** Nexus core v1.13.0 / SDKs v1.15.0 / branch `release/v1.2.0`
**Author:** automated multi-agent audit (architecture + performance + ecosystem + competitors)
**Scope:** end-to-end snapshot of current state, measured results, and improvement levers; positioned against the 2025–2026 graph + vector DB market.

## Files

| # | File | Topic |
|---|------|-------|
| 01 | [01_executive_summary.md](01_executive_summary.md) | TL;DR — strengths, gaps, positioning, top 5 priorities |
| 02 | [02_architecture_assessment.md](02_architecture_assessment.md) | Layered architecture review (storage → MVCC → indexes → executor → API) |
| 03 | [03_performance_results.md](03_performance_results.md) | Benchmarks: SIMD, FTS, KNN, RPC vs HTTP, vs Neo4j (74-test suite) |
| 04 | [04_neo4j_compatibility.md](04_neo4j_compatibility.md) | 300/300 diff-suite status, openCypher coverage, APOC parity |
| 05 | [05_competitor_landscape.md](05_competitor_landscape.md) | Neo4j, Memgraph, ArangoDB, TigerGraph, Dgraph, JanusGraph, NebulaGraph, KuzuDB, FalkorDB, vector-only DBs |
| 06 | [06_feature_gaps.md](06_feature_gaps.md) | Missing/partial features vs market expectations |
| 07 | [07_quality_metrics.md](07_quality_metrics.md) | Tests, coverage, code quality signals, tech debt |
| 08 | [08_sdk_ecosystem.md](08_sdk_ecosystem.md) | 6 SDKs, transports, CLI, distribution channels |
| 09 | [09_distributed_v2_status.md](09_distributed_v2_status.md) | V2 sharding, Raft, coordinator, cluster API, replication |
| 10 | [10_improvement_roadmap.md](10_improvement_roadmap.md) | Prioritized recommendations w/ effort estimates |

## Methodology

- Codebase walk: `crates/nexus-core/src/{storage,page_cache,wal,index,executor,transaction,sharding,coordinator,auth}`, plus `crates/nexus-{server,cli,protocol,bench}`.
- Docs ingested: `docs/{ARCHITECTURE,ROADMAP,PRD,PERFORMANCE,CLUSTER_MODE}.md`, `docs/specs/*`, `docs/performance/*`, `docs/compatibility/*`.
- Benchmarks: `docs/performance/PERFORMANCE_V1.md`, `BENCHMARK_NEXUS_VS_NEO4J.md` (74-test suite, Dec 2025), `BENCHMARK_RESULTS_PHASE9.md`, `PERFORMANCE_ANALYSIS.md`.
- Tasks: `.rulebook/tasks/phase5_*`, `phase6_*` (active work).
- Competitors: web research (vendor docs, press releases, funding data, GitHub state) for Neo4j 2025.x, Memgraph 3.4, ArangoDB 3.12, TigerGraph 4.2, NebulaGraph 5.2, KuzuDB (archived 2025-10-10), FalkorDB, Dgraph (Hypermode → Istari acq.), JanusGraph, plus pure-vector DBs.

## How to use

Read 01 first (TL;DR). Then 03 + 05 for the strategic picture. 06 + 10 drive the next planning cycle. 02 / 07 / 09 are the deep technical reads.

## Caveats

- Version numbers in this report come straight from the repo at audit time. README declares **v1.13.0**, `CHANGELOG.md` top entry is **v1.2.0 (2026-04-28)**, SDKs are at **v1.15.0** — this version-string drift is itself a finding (see 07).
- 74-test Neo4j benchmark is from **Dec 2025 / v0.12.0**; the concurrency-fix work shipped in Phase 9 (Nov 2025) materially changed throughput. Some category-level ratios are stale on the absolute throughput axis even though per-query speedups still hold.
- No live Neo4j 5.x re-run was performed for this audit; quoted comparison numbers are from Nexus's own benchmark suite.
