# 01 — Executive Summary

**Date:** 2026-04-28 · **Baseline:** v1.13.0 core, v1.15.0 SDKs, 2310 workspace tests passing, 300/300 Neo4j diff-suite.

## TL;DR

Nexus is a **production-credible single-node property graph + vector database** with a **functionally complete V2 sharded mode** (Raft per shard, 201 dedicated tests). It outperforms Neo4j Community on **73 of 74 measured Cypher operations** (avg **4.15× faster**, max **42.74× on relationship creation**), ships **6 first-party SDKs** in lockstep at v1.15.0, and is the **only credible open replacement for KuzuDB** (archived Oct-2025) in the embedded-friendly Cypher+vector quadrant. The product is **not yet competitively complete** against Neo4j Aura / Memgraph 3.4 / ArangoDB 3.12 on three axes: distributed transactions, query optimizer sophistication, and ecosystem breadth (no Java/JVM driver, no Bloom-class GUI, no GDS feature parity).

## What's working

- **Storage + MVCC**: solid Neo4j-style 32B/48B fixed records, memmap2, epoch-based snapshot isolation, WAL with CRC32 + checkpointing. 96 %+ test coverage on storage layer.
- **SIMD kernels**: AVX-512 / AVX2 / NEON runtime dispatch with proptest parity. Real, large multipliers — KNN dot @ d=768 **12.7×**, l2_sq @ d=512 **13.5×**, sum_f32 @ 262K rows **15.9×**, popcount @ 4 KiB **≈11×**.
- **Cypher coverage**: 250+ functions, ~100 APOC procedures, 19 GDS procedures, constraints (UNIQUE / NODE KEY / NOT NULL / type-check), Cypher 25 `FOR ... REQUIRE` DDL, savepoints, named graphs. Cypher parser fix: O(N²) → O(N), **~290× on 31.5 KiB queries**.
- **Native vector**: per-label HNSW, cosine/L2/dot, bytes-native on the RPC wire. Hybrid graph-traversal + KNN works in one query.
- **Full-text search**: Tantivy 0.22 with 9 analyzers; **150 µs** single-term @ 100k×1 KB corpus, **60k docs/s** bulk ingest, async writer path optional.
- **Binary RPC default**: 3–10× lower latency, 40–60 % smaller payloads vs HTTP/JSON. SDKs default to `nexus://` URLs.
- **Auth**: API key + JWT + RBAC + per-key rate limiting + 3-layer back-pressure (key → conn → admission queue) + audit log with fail-open metric.
- **V2 cluster (core)**: hash sharding, per-shard Raft (65 unit tests, 3-/5-node scenarios), distributed coordinator (46 tests, scatter-gather + LRU cross-shard cache), `/cluster/*` REST API, multi-host TCP transport.

## What's not working / not finished

| Issue | Impact | File |
|-------|--------|------|
| **Global executor lock** serializing all queries | CPU sits at ~12 % under load (Nexus 202 qps vs Neo4j 507 qps measured pre-fix) — Phase 9 partly addressed but no post-fix multi-writer benchmark in repo | `crates/nexus-core/src/engine/` |
| **JIT compiler disabled** | All execution interpreted; ~80 % of `execution/jit/` is `// TODO` Cranelift codegen | `crates/nexus-core/src/execution/jit/` |
| **No cross-shard 2PC** | Multi-shard mutations fail-atomic but no read-consistency across shards mid-query | `crates/nexus-core/src/coordinator/` |
| **R-tree spatial index** parser-only | `point.withinDistance` / `withinBBox` planner-rewriter shipped (v1.2.0), but full Cypher spatial predicates not yet wired across all clauses | `crates/nexus-core/src/index/rtree/` |
| **Quantified path patterns / `CALL ... IN TRANSACTIONS`** grammar-only | Neo4j 5.9+ syntax parses but doesn't execute | `phase6_opencypher-*` tasks |
| **Planner is heuristic** | No cardinality propagation, no join-DP, no query cache, no prepared statements | `crates/nexus-core/src/executor/planner/` |
| **Replication failover manual** in v1; per-shard Raft is the V2 answer but the v1 replica path still ships | Operator confusion + risk window | `docs/operations/REPLICATION.md` |
| **Version-string drift** README v1.13.0, CHANGELOG v1.2.0, SDK v1.15.0 | Release hygiene signal | repo root |
| **No Java/JVM SDK** | Hard ceiling on enterprise adoption (Neo4j, Spring Data, GraphRAG-on-JVM) | `sdks/` |
| **No GUI / Bloom-equivalent** (Phase 5 GUI archived to v1.1) | Demo + ops gap vs Neo4j | task `phase5_implement-v1-gui` |

## Market positioning (mid-2026)

- **The Kuzu vacancy**: KuzuDB was archived **2025-10-10**. Forks (Bighorn, Ladybug, RyuGraph) are early. **Nexus is the most production-shaped Cypher + vector + embeddable-leaning replacement** with active commercial momentum behind it. This window closes within ~12 months as forks mature or FalkorDB consolidates.
- **vs Neo4j**: Nexus wins on raw per-query latency on most operations (4.15× avg, 42.7× peak), wins on price (Apache 2.0 vs GPLv3 Community / commercial Enterprise), and ships native vector at parity-or-better. Loses on ecosystem (no Bloom, no GDS-2.20 algo set, no Spring Data, no JVM driver), enterprise tooling, and managed cloud.
- **vs Memgraph 3.4**: similar Cypher + vector positioning. Memgraph is in-memory (RAM-bounded); Nexus is on-disk + page-cache (no RAM ceiling). Memgraph has a stronger streaming story, MAGE/cuGraph integrations, and a longer commercial track record.
- **vs ArangoDB 3.12.5+** (which now bundles Enterprise features in CE): Arango wins on multi-model breadth + AQL+ArangoSearch+vector hybrid in one query. Nexus wins on graph-native traversal performance and binary RPC.
- **vs FalkorDB**: FalkorDB is Redis-cluster-bound and GraphBLAS-fast on dense aggregates. Nexus is more independent (no Redis dependency), has its own cluster mode, ships its own SDKs.
- **vs pure vector DBs** (Pinecone / Weaviate / Qdrant / Milvus): orthogonal — graph-native traversal is Nexus's moat. Risk: Weaviate / Qdrant ship "graph-lite" features faster than Nexus closes its ANN-at-billion-scale gap.

## Top 5 priorities (next 1–2 quarters)

1. **Kill the global executor lock** for real (multi-reader concurrent execution path; verify with a re-run of the 74-test benchmark on the latest build). Current numbers from Dec-2025 are stale; ship a v1.2 benchmark report.
2. **Finish R-tree + quantified path patterns + `CALL ... IN TRANSACTIONS` execution.** All three are grammar-only blockers for Neo4j 5.9+ feature parity. Effort: ~6 weeks combined.
3. **Cross-shard 2PC or pessimistic locking** for V2 mutations; without this, V2 cluster mode is "single-shard writes only," which is the real gating constraint vs Memgraph / Arango on cluster correctness.
4. **JVM SDK (Java + Kotlin coroutines).** Single biggest enterprise-adoption gate; the Bolt-protocol world lives on the JVM. Effort: ~4 weeks if patterned on the Rust SDK.
5. **Operator GUI (Bloom-class)** — even a thin web UI (schema view, query runner with table+graph rendering, KNN inspector). The currently-archived Vue3 work in `phase5_implement-v1-gui` is the obvious starting point.

## Strategic threats / opportunities

- **+ Kuzu vacancy** is exploitable for ~12 months; needs a clear "migrate from Kuzu" guide + WASM/embedded story (Nexus is single-binary, so this is achievable).
- **+ ArangoDB Apache-2.0 expansion** weaponizes pricing against Neo4j; Nexus should match the messaging ("Apache-2.0 forever, no enterprise gate").
- **− Neo4j Aura Agent / GraphRAG** lands a turnkey copilot story Nexus has no equivalent for; mitigate with a documented LangChain/LlamaIndex integration pack.
- **− Memgraph GraphRAG toolkit** (Nov 2025) targets non-graph users — Nexus's docs assume Cypher fluency; entry-level GraphRAG quickstarts are missing.
- **− Pure-vector DBs adding "metadata filters that look like graphs"** is a slow-erosion threat; the counter is a published "Nexus is faster *and* graph-native" benchmark vs Pinecone/Weaviate at d=768, k=10, with traversal in the same query.

See [10_improvement_roadmap.md](10_improvement_roadmap.md) for the prioritized work list with effort estimates.
