# 06 — Feature Gaps vs Market

Gaps grouped by competitive urgency. Each row: feature, who has it, Nexus status, impact, effort.

## Tier A — gaps that block enterprise adoption

| Feature | Who has it | Nexus | Impact | Effort |
|---------|-----------|-------|--------|--------|
| **Cross-shard 2PC / distributed write transactions** | Neo4j Causal Cluster, Memgraph HA, ArangoDB cluster, Dgraph, NebulaGraph | V2 supports **single-shard writes only**; multi-shard mutations unsupported | **High** — V2 cluster mode advertised but write-correctness gated | 4–6 weeks |
| **JVM SDK (Java + Kotlin)** | Neo4j (canonical), Memgraph (Bolt), Arango (Java driver), TigerGraph (TgJDBC) | None. 6 SDKs ship for Rust/Python/TS/Go/C#/PHP at v1.15.0 | **High** — JVM is the Bolt-driver world; missing this caps adoption | ~4 weeks (port from Rust SDK) |
| **Bolt-protocol shim** | every Neo4j-compatible engine that wants existing-driver reuse | None — Nexus uses bespoke MessagePack RPC | **High** — turns "we have to rewrite client code" into "switch the URL" | 3–4 weeks |
| **Operator GUI / Bloom-class** | Neo4j Bloom, Memgraph Lab, ArangoDB Web UI, NebulaGraph Studio | Vue3 work archived in `phase5_implement-v1-gui` (deferred to v1.1) | **High** — demo + ops + on-call defect | 6–10 weeks |
| **Encryption at rest** | Neo4j Enterprise, Memgraph Enterprise, Arango, NebulaGraph Ent | Not in engine; defers to OS/disk-level | **Medium-high** — required for FedRAMP / SOC2 customer asks | 2–3 weeks (LMDB + record-store + WAL hooks) |
| **SSO / OIDC / SAML** | Neo4j Enterprise, Aura, ArangoDB Ent, TigerGraph | API key + JWT only | **Medium-high** — same compliance gate | 2 weeks |

## Tier B — gaps that block competitive narratives

| Feature | Who has it | Nexus | Impact | Effort |
|---------|-----------|-------|--------|--------|
| **GraphRAG copilot / LLM integration pack** | Neo4j Aura Agent, Memgraph GraphRAG toolkit (Nov 2025) | None packaged | **Medium-high** — biggest 2025–2026 narrative axis | 3–4 weeks (LangChain/LlamaIndex integrations + cookbook) |
| **Quantified path patterns execution** (Neo4j 5.9+) | Neo4j 5.9+, Memgraph (partial) | Grammar + AST shipped; executor pending | **Medium** — Cypher 25 feature parity | 2–3 weeks |
| **`CALL { ... } IN TRANSACTIONS`** | Neo4j 5.x, Memgraph | Grammar shipped; executor batching pending | **Medium** — large-ETL story | 1 week |
| **Full geospatial Cypher predicates** | Neo4j (Spatial), Memgraph, ArangoDB Geo, NebulaGraph | Seek shapes shipped (v1.2.0); full predicate wiring across all clauses pending | **Medium** — geospatial RAG, logistics | 3–4 weeks |
| **`USING INDEX/SCAN/JOIN` planner hints honored** | Neo4j, Memgraph | Parsed but ignored | **Medium** — DBA tunability | 1 week |
| **Online re-sharding** | Arango, NebulaGraph, Dgraph | Iterative `RebalancePlan` exists but not exercised end-to-end on running clusters | **Medium** — cluster ops gating | 4–6 weeks (incl. tests) |
| **Automatic failover** for v1 master-replica path | Neo4j Causal, Aura | Manual (`POST /replication/promote`); V2 Raft is the answer for shard data | **Medium** — single-node + replica deployments | 2 weeks |
| **Bolt v5 driver compatibility** | every Neo4j 5.x driver | n/a | **Medium** — overlaps with Bolt shim above | covered by shim |
| **WASM / embedded mode** | Kuzu (and forks), FalkorDB `falkordblite` | n/a — Nexus is single binary, not embeddable | **Medium** — captures the Kuzu-vacancy embedded story | 4–6 weeks (compile core to WASM, expose JS bindings) |

## Tier C — competitive parity polish

| Feature | Who has it | Nexus | Impact | Effort |
|---------|-----------|-------|--------|--------|
| **GDS algorithm count** | Neo4j GDS 2.20 (~70 algos incl. Maximum Flow GA 2025.07) | 19 algos shipped | **Medium** — analytical workload story | 1–2 weeks per algo family |
| **Histograms in stats catalog** (not just NDV) | Neo4j, Arango, TigerGraph | NDV + cardinality only | **Medium** — better planner cost estimates | 2 weeks |
| **Plan cache / prepared statements** | Neo4j, Memgraph, Arango | Every query re-parses + re-plans | **Medium** — high-volume RAG endpoints | 2 weeks |
| **JIT / compiled query path** | Neo4j Cypher Runtime (Slotted/Pipelined) | `execution/jit/` Cranelift codegen disabled | **Medium** — 1.5–2× hot-path | 2–3 weeks (or replace with bytecode VM) |
| **Cypher row-count parity on OPTIONAL/WITH/Write** | Neo4j (canonical) | 22 of 74 cross-bench tests differ on row shape | **Medium** — narrative cleanup | 1 week |
| **Recall@k benchmarks for KNN** | Pinecone, Weaviate, Qdrant publish | Latency-only | **Medium** — vector-DB credibility | 1 week (corpus + ground-truth + report) |
| **Variable-length path perf doc** | Neo4j benchmarks | Not benchmarked | **Low-medium** | 3 days |
| **Streaming ingest** (Kafka / Pulsar source) | Memgraph (strong), TigerGraph, Dgraph | Bulk `/ingest` only | **Low-medium** | 2–3 weeks |
| **Apache Arrow integration** | TigerGraph (Arrow Flight), Kuzu | n/a | **Low** — analytical interop | 2–3 weeks |
| **Multi-region / geo-replication** | Neo4j Aura, Arango DC2DC | Roadmapped V2.1 | **Low (today), High (12 mo)** | 8–12 weeks |
| **Encryption keys via KMS / HSM** | Neo4j Ent, Aura | n/a | **Low (today), Medium (compliance)** | 2 weeks |
| **Backup coordination across shards** | Neo4j, Arango | Per-node manual | **Medium** for V2 production deployment | 2–3 weeks |

## Tier D — cosmetic / ecosystem

| Feature | Who has it | Nexus | Impact | Effort |
|---------|-----------|-------|--------|--------|
| **Helm chart / Kubernetes operator** | Neo4j, Memgraph, NebulaGraph all ship K8s operators | unknown — no `deploy/k8s/` evident | **Medium** — cloud-native gate | 2 weeks |
| **Prometheus / OpenTelemetry exporter** | universal in 2025+ | partial (audit log metric, `/stats`) | **Low-medium** | 1 week |
| **Grafana dashboards** | Neo4j, Memgraph ship templates | n/a | **Low** | 2 days |
| **Migration guides FROM Neo4j / Memgraph / Kuzu** | FalkorDB published Kuzu→FalkorDB | none | **Medium** — ride the Kuzu vacancy | 1 week per source |
| **Spring Data Nexus** | Spring Data Neo4j is the canonical JVM ORM-ish | n/a | **Medium** — Java enterprise | 2–3 weeks (after JVM SDK lands) |
| **Cypher LSP / IDE plugins** | Neo4j (vscode + IntelliJ), Memgraph Lab | n/a | **Low** | 1–2 weeks |

## What Nexus has that competitors don't (differentiation worth marketing)

| Feature | Where |
|---------|-------|
| **Binary RPC default** with bytes-native KNN embeddings (no base64 tax) | unique among Cypher peers; 3–10× lower latency than HTTP/JSON |
| **Three-layer back-pressure** (per-key + per-conn + global admission queue with `503 Retry-After`) | rare in graph DBs; usually bolted on by ops |
| **Audit log fail-open metric** (`nexus_audit_log_failures_total`) | most engines either drop audit silently or fail-closed (500s); Nexus is the only one that exposes both |
| **Documented SIMD honesty** (negative results: CRC32C HW slower, simd-json slower on `Value`, columnar 1.13× not 16×) | almost no other DB ships this | use as trust-signal in marketing |
| **6 first-party SDKs at lockstep version (1.15.0)** | exceeds Neo4j's 1st-party language coverage at this maturity tier; Memgraph ships fewer |
| **300/300 Neo4j diff-suite pass rate** | publicly verifiable; reproducible via PowerShell script |
| **Apache 2.0 with no enterprise gate** | matches ArangoDB 3.12.5+ posture, beats Neo4j Community GPLv3 / Memgraph BSL / FalkorDB SSPL |

## Summary

The Tier-A list (cross-shard 2PC, JVM SDK, Bolt shim, GUI, encryption-at-rest, SSO) is the **enterprise readiness package**. Tier B is the **Cypher-25 + GraphRAG narrative package**. Tier C is **planner / runtime polish**. Tier D is **distribution and DX**.

Total estimated effort to close Tier A: **~18–24 engineer-weeks**. Closing Tier A + B: **~32–40 engineer-weeks**. See [10_improvement_roadmap.md](10_improvement_roadmap.md) for prioritization.
