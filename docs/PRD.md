# Nexus — Product Requirements Document (PRD)

> **Version**: 1.0.0-dev
> **Status**: Release candidate (branch `release/v1.0.0`)
> **Owner**: HiveLLM Nexus Team
> **Last updated**: 2026-04-18
> **Related**: [ARCHITECTURE.md](../ARCHITECTURE.md) · [ROADMAP.md](../ROADMAP.md) · [DAG.md](../DAG.md)

---

## 1. Executive Summary

**Nexus** is a high-performance property graph database with native vector
search, engineered for **read-heavy, embedding-aware workloads** — in
particular Retrieval-Augmented Generation (RAG), hybrid semantic+graph
retrieval, recommendation systems, and static code intelligence.

The system implements a Neo4j-inspired storage architecture (fixed-size
record stores, doubly-linked adjacency lists, epoch-based MVCC) and fuses
first-class HNSW KNN indexes into the same transactional substrate, so a
single query can combine vector similarity with multi-hop graph traversal
without round-tripping between two separate systems.

v1.0.0 ships the single-node engine with ~55% openCypher coverage, 300/300
Neo4j-compatibility tests (array row format), six official SDKs, SIMD
kernels (AVX-512 / AVX2 / NEON) runtime-dispatched, and production
hardening (auth, audit, replication primitives, Prometheus metrics).

---

## 2. Problem Statement

Modern AI applications need to combine two forms of retrieval that, today,
live in different systems:

1. **Semantic retrieval** over dense vector embeddings (RAG, "find me the
   10 chunks most similar to this question").
2. **Structural retrieval** over typed relationships ("find me the authors
   of those chunks and the papers they cite").

The status-quo solution bolts together a vector database (Pinecone, Qdrant,
Weaviate) and a graph database (Neo4j, TigerGraph), with application code
shuttling IDs between them. This introduces:

- **Consistency drift** — embeddings and graph state evolve independently.
- **Latency tax** — every hybrid query pays two network round-trips.
- **Operational overhead** — two clusters, two monitoring stacks, two
  backup procedures, two auth models.
- **Semantic impedance** — vector IDs ≠ graph node IDs; mapping layers rot.

Nexus removes the split: vectors are just another property stored on
graph nodes, KNN is a native operator in Cypher, and a single query plan
can start from vector similarity and expand through graph relationships
— atomically, under one transaction, one auth policy, one backup.

---

## 3. Vision & Goals

### Vision
> *"One datastore for both meanings and connections — fast enough to put
> on the hot path of an LLM application."*

### Strategic goals (v1.0)

| # | Goal | Signal of success |
|---|------|-------------------|
| G1 | **Hybrid-query performance** — make `KNN + graph expand` competitive with or faster than dedicated vector DBs for ≤10M-node corpora | ≥10K KNN queries/sec @ dim=768, p95 < 2 ms |
| G2 | **Neo4j compatibility** — let Neo4j users migrate without rewriting queries or clients | 100% of the 300-query compatibility suite passes |
| G3 | **Developer ergonomics** — 5-minute "pip install → running query" on every major language | 6 official SDKs, each with ≥30-test comprehensive suite passing |
| G4 | **Production readiness** — authentication, audit, metrics, replication, Docker-ready deploys | Auth, RBAC, Prometheus metrics, replication (async/sync), Helm/Docker docs, SECURITY_AUDIT signed off |
| G5 | **Ops predictability** — no surprise latency spikes from GC pauses or unbounded caches | Input hardcaps, capped page cache, bounded WAL, documented memory tuning guide |

### Non-goals (v1.0)

- **Write-heavy OLTP.** Single-writer by design; we optimize for mixed read + bulk ingest, not per-row TPS.
- **Full openCypher parity.** We target the ~55% subset that serves graph-plus-vector workloads; `CALL {}` subqueries, advanced constraints, and the full procedure catalog stay in V2.
- **Distributed writes / sharding.** Horizontal scaling is V2 (Phase 3). v1.0 is a replicated single master.
- **Simple key-value storage.** Use Redis/RocksDB; Nexus is overkill.
- **General-purpose analytics engine.** We expose a graph-algorithm catalog (GDS-style), but OLAP scan performance is not a v1.0 commitment.

---

## 4. Target Users & Personas

### Persona A — "RAG Engineer" (primary)
- Builds LLM-backed applications with a retrieval step.
- Current stack: Postgres + Pinecone/Qdrant, glue code in Python/TS.
- Pain: keeping embeddings in sync with entity data; multi-hop retrieval
  requires N+1 vector lookups.
- Needs from Nexus: Cypher + `knn()` in one query, Python & TS SDKs, MCP
  integration for Claude/Cursor tooling.

### Persona B — "Neo4j Refugee"
- Already runs Neo4j in production, hitting licence cost or performance
  wall, wants a drop-in with native vector support.
- Pain: rewriting Cypher to another dialect; migrating drivers.
- Needs: Bolt-like wire compat (Cypher over REST/RESP3), array row format,
  `MATCH / WHERE / RETURN / CREATE / MERGE / UNWIND / WITH` coverage,
  procedure catalog for core GDS.

### Persona C — "Recommendation / Similarity Platform Engineer"
- Runs user-item or entity-similarity workloads.
- Pain: join-heavy queries with per-user vector embeddings.
- Needs: high-throughput KNN at low dim (32-256), parameterized Cypher,
  hybrid "similar AND connected-via-type" queries.

### Persona D — "Dev-tool / Code-intel Builder"
- Builds static analysis, LLM-assisted coding, or architecture-diagram
  tools that need call graphs, dep graphs, data-flow graphs.
- Needs: Graph correlation API (call/dep/dataflow/component graphs),
  visualization exports (SVG, GraphML), MCP tools.

### Persona E — "Platform Ops / SRE"
- Deploys and operates Nexus for the personas above.
- Needs: container images, Prometheus metrics, replication, rotatable API
  keys, audit log, memory tuning guide, backup/restore procedure.

---

## 5. Use Cases / Top User Stories

| ID | As a… | I want to… | So that… |
|----|-------|------------|----------|
| US-1 | RAG engineer | issue one Cypher query that `MATCH`es nodes by KNN-similarity and expands through `CITES` relationships | my retrieval prompt includes semantically-similar docs **and** their graph-neighbors in a single round-trip |
| US-2 | Neo4j user | reuse my existing Cypher scripts against a Nexus endpoint | my migration is a config change, not a rewrite |
| US-3 | Platform engineer | isolate multiple tenants in separate databases on one Nexus server | I don't pay for N clusters and can enforce per-tenant quotas |
| US-4 | Dev-tool builder | call MCP tools (`graph_correlation_generate`, `graph_correlation_analyze`) from Claude/Cursor | my AI assistant can see the codebase as a graph |
| US-5 | SRE | scrape a Prometheus `/prometheus` endpoint for cache hit rate, WAL fsync latency, audit-log failure count | I can page on the things that cause incidents |
| US-6 | Security engineer | require an API key, rotate it, audit who read what | I meet compliance without modifying application code |
| US-7 | Data engineer | bulk-ingest a 100M-row JSON/CSV file | initial corpus load completes in one maintenance window |
| US-8 | SRE | promote a replica to master with bounded data loss | a single-node failure doesn't require a cold restore from backup |

---

## 6. Functional Requirements

### 6.1 Query language — Cypher subset
- **MUST** support `MATCH / OPTIONAL MATCH / WHERE / RETURN / ORDER BY / SKIP / LIMIT`.
- **MUST** support `CREATE / MERGE / DELETE / DETACH DELETE / SET / REMOVE`.
- **MUST** support `UNWIND / WITH / UNION / UNION ALL / CALL <procedure>`.
- **MUST** support parameterized queries via `$name` binding.
- **MUST** expose ~60 functions across string, numeric, list, map, date/time, spatial, and aggregation categories.
- **MUST** pass 300/300 Neo4j-compatibility tests (array row format).
- **SHOULD** expose an `EXPLAIN` / query-plan inspection path.
- Detailed spec: [docs/specs/cypher-subset.md](../specs/cypher-subset.md).

### 6.2 Vector search
- **MUST** store vectors as node properties (`f32` and `f64`).
- **MUST** provide per-label HNSW indexes via `hnsw_rs`.
- **MUST** expose `knn(node, k)` and `knn_traverse(start, vector, k, depth)` primitives.
- **MUST** support cosine, dot, and L2 distance metrics.
- **MUST** dispatch SIMD kernels at runtime (AVX-512, AVX2, SSE4.2, NEON) with a scalar fallback proven by property-based tests.
- Detailed spec: [docs/specs/knn-integration.md](../specs/knn-integration.md) · [docs/specs/simd-dispatch.md](../specs/simd-dispatch.md).

### 6.3 Storage & transactions
- **MUST** persist graph state in fixed-size record stores (node=32 B, rel=48 B) backed by `memmap2`.
- **MUST** journal every mutation via append-only WAL with CRC32 validation and `fsync` on commit.
- **MUST** provide epoch-based MVCC snapshot isolation, single-writer per partition.
- **MUST** survive `kill -9` with zero data loss on committed transactions.
- **MUST** support database-level isolation — separate catalog (LMDB), stores, indexes, and WAL per logical DB.
- Detailed spec: [docs/specs/storage-format.md](../specs/storage-format.md) · [docs/specs/wal-mvcc.md](../specs/wal-mvcc.md) · [docs/specs/page-cache.md](../specs/page-cache.md).

### 6.4 Indexes
- **MUST** provide:
  - Label bitmap index (RoaringBitmap).
  - B-tree property index.
  - Full-text index (Tantivy).
  - HNSW KNN index (per label, per vector property).
- **SHOULD** provide composite and spatial (R-tree) indexes.

### 6.5 APIs & protocols
- **MUST** expose a REST API on a configurable port (default `15474`).
- **MUST** expose: `POST /cypher`, `POST /knn_traverse`, `POST /ingest`, `GET /health`, `GET /stats`, `GET /prometheus`, database-management endpoints.
- **MUST** expose an MCP server for integration with Claude Desktop / Cursor.
- **SHOULD** expose a RESP3 debug listener (loopback-only, opt-in).
- **SHOULD** expose a UMICP channel for HiveLLM-internal agents.
- Detailed spec: [docs/specs/api-protocols.md](../specs/api-protocols.md) · [docs/specs/rpc-wire-format.md](../specs/rpc-wire-format.md) · [docs/specs/resp3-nexus-commands.md](../specs/resp3-nexus-commands.md).

### 6.6 Clients (SDKs)
- **MUST** ship official clients for: Rust, Python, TypeScript, Go, C#, PHP.
- **MUST** share a common row format (Neo4j-compatible `[[value1, value2]]`) across all SDKs.
- **MUST** each ship a ≥30-test comprehensive suite covering CRUD, aggregation, parameterized queries, NULL handling, CASE, UNION, MERGE, database management.

### 6.7 Security
- **MUST** disable authentication by default when binding to loopback; **require** it when binding to public interfaces.
- **MUST** implement API-key auth with Argon2 hashing and revocation.
- **MUST** provide RBAC with per-database scoping.
- **MUST** rate-limit per API key (defaults: 1000/min, 10000/h).
- **MUST** implement Cypher-injection validation (parameter-binding enforcement, identifier allow-list).
- **MUST** emit auditable events for authentication outcomes and write operations; failures are counted via `nexus_audit_log_failures_total` (fail-open: original response is preserved, but the miss is observable).
- Detailed doc: [docs/security/AUTHENTICATION.md](../security/AUTHENTICATION.md) · [docs/security/SECURITY_AUDIT.md](../security/SECURITY_AUDIT.md).

### 6.8 Operations
- **MUST** expose Prometheus-format metrics (latency histograms, cache hit rate, WAL size/fsync, audit failures, active connections).
- **MUST** ship Docker/Dockerfile with a multi-stage build and health-check-friendly entrypoint.
- **MUST** provide deployment, memory-tuning, and replication docs.
- **SHOULD** provide backup/restore utilities and point-in-time snapshots.
- **SHOULD** provide async + sync replication with configurable quorum.
- Detailed docs: [docs/guides/DEPLOYMENT_GUIDE.md](../guides/DEPLOYMENT_GUIDE.md) · [docs/performance/MEMORY_TUNING.md](../performance/MEMORY_TUNING.md) · [docs/operations/REPLICATION.md](../operations/REPLICATION.md).

### 6.9 Graph analytics
- **SHOULD** expose ≥20 GDS-compatible procedures: PageRank, Louvain, WCC/SCC, Betweenness/Closeness/Degree/Eigenvector centrality, Dijkstra / A* / Yen's K-shortest, triangle count, clustering coefficient.
- **SHOULD** expose graph-correlation APIs for call/dependency/data-flow/component graphs, with pattern detection.
- Detailed spec: [docs/specs/graph-correlation-analysis.md](../specs/graph-correlation-analysis.md).

---

## 7. Non-Functional Requirements

### 7.1 Performance targets (single-node, Ryzen 9 7950X3D reference)

| Dimension | Target | Measured (v1.0-dev) |
|-----------|--------|---------------------|
| Point read (by ID) | ≥100K ops/sec | 100K+ ✅ |
| KNN query @ dim=768 | ≥10K q/sec, p95 < 2 ms | 34.5 ns/dot AVX-512, **12.7×** scalar ✅ |
| Write throughput | ≥10K ops/sec | 603+ q/sec end-to-end, 15% faster than Neo4j ✅ |
| Cypher parse @ 32 KiB query | O(N) | ~290× faster than prior O(N²) ✅ |
| Cache hit rate (steady) | ≥90% for hot working set | tracked via `/prometheus` |

### 7.2 Reliability
- **Durability**: committed transactions survive `kill -9`; WAL replay covers crash boundary.
- **Availability**: async replication keeps replicas ≤1 WAL segment behind master under nominal load.
- **Integrity**: CRC32 on every WAL entry; xxHash3 on every page; corrupt-page detection refuses to serve poisoned data.

### 7.3 Scalability (v1.0)
- Vertical scalability: up to 64 GB RAM page cache, billions of nodes per database, multiple databases per instance.
- Horizontal read-scale via replicas (v1.0).
- Horizontal write-scale via shards (V2, not v1.0).

### 7.4 Compatibility
- **Cypher**: 300/300 Neo4j compatibility tests (openCypher subset).
- **Wire**: Neo4j-shaped JSON response `{"columns": [...], "rows": [[v1, v2]], "execution_time_ms": N, "stats": {...}}`.
- **Server format is frozen**: any per-SDK ergonomic transformation lives in the SDK (e.g. Go's `RowsAsMap()`), never in the server.

### 7.5 Quality gates
- 95%+ unit/integration test coverage for new code.
- `cargo +nightly fmt --all` clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — zero warnings.
- `cargo test --workspace` — 100% passing.
- Pre-commit hooks enforce the gates; `--no-verify` is forbidden.

### 7.6 Observability
- Structured logs via `tracing`.
- Prometheus metrics with HELP/TYPE metadata for every counter.
- Health endpoint for load balancers and orchestrators.

### 7.7 Documentation
- All documentation in English.
- Per-persona guides (user, ops, SDK).
- Every public API documented with doc-comments or OpenAPI spec.
- Root `/docs/` holds only `ARCHITECTURE.md`, `ROADMAP.md`, `DAG.md`; everything else is categorized under `/docs/<topic>/`.

### 7.8 Security
- Default-deny when bound to public interfaces.
- No network-reachable unauthenticated admin surface.
- All credentials hashed with Argon2; no plaintext secrets in logs, backups, or error responses.

---

## 8. Technical Architecture (summary)

Nexus is a Rust workspace of five crates:

| Crate | Role |
|-------|------|
| `nexus-core` | Graph engine: catalog, stores, page cache, WAL, MVCC, executor, indexes, SIMD, auth |
| `nexus-server` | Axum-based HTTP/REST façade, middleware, admin endpoints, Prometheus exporter |
| `nexus-protocol` | REST / MCP / UMICP client bindings |
| `nexus-cli` | Interactive CLI over the server |
| `sdks/*` | Language SDKs (Rust, Python, TS, Go, C#, PHP) |

Full detail: [ARCHITECTURE.md](../ARCHITECTURE.md). Module dependency ordering: [DAG.md](../DAG.md).

---

## 9. Release Plan

### v1.0.0 (current release candidate) — Single-node production

- Full MVP (Phase 1) + V1 hardening (Phase 2 subset): advanced indexes, replication primitives, query optimizer, bulk loader, Prometheus, auth, audit, all six SDKs.
- **Exit criteria**: all targets in §7.1 met on the reference hardware; 300/300 Neo4j tests; zero critical bugs open; SECURITY_AUDIT signed off; deployment + memory-tuning + replication docs published.

### v1.1 (post-1.0, candidate features)

- Full-text index operational coverage, point/spatial indexes, expanded GDS catalog, GUI improvements.
- Backup/restore CLI, point-in-time snapshots.

### v2.0 — Distributed graph

- Sharding (hash on node_id), Raft per shard (openraft), distributed query coordinator, multi-region replication, chaos-engineering suite.
- Timeline: 12–16 weeks post-1.0.

Details: [ROADMAP.md](../ROADMAP.md).

---

## 10. Success Metrics (KPIs)

### Product KPIs
- **Time-to-first-query** — developer installs SDK → runs first `MATCH` in ≤5 minutes.
- **Migration friction** — Neo4j user reuses ≥95% of their Cypher unchanged.
- **Hybrid-query win rate** — for a RAG benchmark with `KNN + 2-hop expand`, Nexus is within 2× of the best single-purpose vector DB **and** within 2× of Neo4j on pure graph traversal.

### Engineering KPIs
- 100% passing test suite on every merged commit.
- <0.1% of releases require a hotfix patch.
- Zero critical (CVSS ≥ 9.0) security vulnerabilities open at any time.

### Operational KPIs (reported by deployed instances)
- p95 query latency published per database.
- Replication lag < 1 s at p99 under steady workload.
- Audit-log failure rate = 0 at steady state.

---

## 11. Risks & Mitigations

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| R1 | HNSW recall degrades at high update rate | Med | Med | Periodic index rebuild; expose recall metrics; document tuning |
| R2 | Single-writer becomes the bottleneck before sharding ships | Med | High | Batch-write API, bulk loader, asynchronous indexing, document hot-path tuning |
| R3 | Server row format change breaks SDKs | Low | High | **Frozen contract**; all per-SDK transformation lives in SDKs; CI checks format stability |
| R4 | Audit-log IO pressure causes request storm | Low | Med | Fail-open policy + `nexus_audit_log_failures_total` alert (documented in SECURITY_AUDIT §5) |
| R5 | Cypher planner picks bad plan on large graphs | Med | Med | Statistics collection; `EXPLAIN` surface; A/B plan-evaluation harness |
| R6 | Page-cache memory blowup under adversarial input | Low | High | Input hardcaps (spec: §7.8); capped page cache; `MEMORY_TUNING.md` guide |
| R7 | Distributed-systems timeline slips (V2) | High | Med | V2 explicitly out of v1.0 scope; replication primitives in v1.0 derisk V2 |
| R8 | openCypher subset leaves critical features out | Med | Med | Triage against the 300-compat suite; capture gaps in ROADMAP; pre-commit to v1.1 patch releases |

---

## 12. Open Questions

1. **GUI** — is a bundled desktop GUI part of v1.0 or deferred to v1.1? (Currently marked planned in Phase 2.7.)
2. **Geospatial scope for v1.0** — Point type + distance are shipped; is full R-tree expected at GA?
3. **Backup tooling** — do we commit to a PITR CLI for v1.0 or ship only offline dump/restore?
4. **Cluster mode** — internal task exists (`phase5_implement-cluster-mode`); is any slice of it merged into v1.0 or fully deferred to V2?

These are tracked in the corresponding Rulebook tasks under `.rulebook/tasks/`.

---

## 13. Appendix — Glossary

- **Cypher**: query language for property graphs, introduced by Neo4j and standardized as openCypher.
- **HNSW**: Hierarchical Navigable Small World — a graph-based approximate-nearest-neighbor index.
- **MVCC**: Multi-Version Concurrency Control — readers see a snapshot, writers never block readers.
- **WAL**: Write-Ahead Log — append-only journal used for durability and crash recovery.
- **RAG**: Retrieval-Augmented Generation — LLM pattern where prompts are enriched with retrieved context.
- **MCP**: Model Context Protocol — standard for tool servers consumed by LLM hosts.
- **UMICP**: Unified Multi-agent Inter-Component Protocol — internal HiveLLM transport.
- **RESP3**: Redis Serialization Protocol v3 — used by `redis-cli` and compatible clients.
- **GDS**: Graph Data Science — Neo4j's procedure catalog for graph algorithms; Nexus exposes a compatible subset.
