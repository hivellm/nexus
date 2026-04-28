# 10 — Improvement Roadmap

Prioritized recommendations, with rough engineering effort estimates. Three horizons: **0–4 weeks (P0 — quick wins / unblockers)**, **1–3 months (P1 — competitive parity)**, **3–9 months (P2 — strategic positioning)**.

## P0 — quick wins / unblockers (0–4 weeks)

| # | Item | Why | Effort | Owner |
|---|------|-----|--------|-------|
| P0.1 | **Re-run 74-test vs Neo4j on post-Phase-9 build** and publish v1.2 perf doc | Current numbers are pre-fix (Dec-2025). Concurrency fix changed the picture. Marketing-critical. | 1 wk | perf |
| P0.2 | **Reconcile version strings** — README v1.13.0 / CHANGELOG v1.2.0 / SDKs v1.15.0 | Release hygiene; signals "internal incoherence" to outsiders | 1 day | release |
| P0.3 | **Resolve JIT module** — finish (~3 wk) OR delete (1 day). Disabled-with-TODOs violates project Tier-1 ("no shortcuts"). | Self-imposed quality rule | 1 day OR 3 wk | core |
| P0.4 | **Fix the 2 ignored tests** in `engine/tests.rs` (use `tempfile::tempdir()` per test) | Restores parallel test execution | 1 day | core |
| P0.5 | **Page-cache property-index eviction TODO** in `cache/mod.rs` | Memory predictability under high-cardinality property indexes | 3–5 days | core |
| P0.6 | **Honor `USING INDEX` planner hints** | Feature parity, low effort, immediate DBA win | 1 wk | planner |
| P0.7 | **`CALL { ... } IN TRANSACTIONS` executor batching** (grammar already shipped) | Cypher 25 ETL parity | 1 wk | executor |
| P0.8 | **Cross-test row-count parity passes** (close 22-test diff in 74-bench) | Cleaner narrative; takes compatibility 70 % → 95 % | 1 wk | executor |
| P0.9 | **Migration guide FROM Kuzu** | Ride the Kuzu vacancy (Oct-2025). FalkorDB already published one. | 3 days | docs |
| P0.10 | **GraphRAG / LangChain / LlamaIndex integration pack** (Python + TS adapter, cookbook) | Biggest 2025–2026 narrative axis; competitors all shipped | 2–3 wk | SDK |
| P0.11 | **Recall@k benchmark** vs ground-truth on SIFT1M / GloVe | KNN credibility; vector-DB convention | 1 wk | perf |
| P0.12 | **K8s Helm chart + Docker Compose example pack** | Cloud-native deployment gate | 2 wk | devops |

**P0 total: ~10–14 engineer-weeks** (parallelizable across 3–4 streams).

## P1 — competitive parity (1–3 months)

| # | Item | Why | Effort |
|---|------|-----|--------|
| P1.1 | **Quantified path patterns execution** (`phase6_opencypher-quantified-path-patterns`) — `()-[]->{1,5}()` operator | Cypher 25 / Neo4j 5.9+ parity | 2–3 wk |
| P1.2 | **R-tree spatial — full Cypher predicate wiring** across all clauses (`phase6_opencypher-geospatial-predicates`) | Geospatial RAG, logistics use cases | 3–4 wk |
| P1.3 | **Function-style `point.nearest(<var>.<prop>, <k>)`** in RETURN/WITH/WHERE | Spatial completeness | 1–2 wk |
| P1.4 | **JVM SDK (Java + Kotlin coroutines)** | Single biggest enterprise gate; Bolt-driver world is JVM | 4 wk |
| P1.5 | **Bolt-protocol shim** (separate port, translates Bolt frames to Nexus executor) | Drop-in compatibility with all Neo4j drivers | 3–4 wk |
| P1.6 | **Cross-shard 2PC or pessimistic per-shard locking** | Removes the "single-shard writes only" V2 ceiling | 4–6 wk |
| P1.7 | **Read-consistency contract across shards** (define semantics, pin with tests) | Cluster correctness story | 2 wk |
| P1.8 | **Plan cache / prepared statements** | High-volume RAG endpoints | 2 wk |
| P1.9 | **Histograms in stats catalog** (not just NDV) | Better planner cost estimates | 2 wk |
| P1.10 | **Operator GUI / Bloom-class** — finish the archived Vue3 work in `phase5_implement-v1-gui` | Demo + ops + on-call defect; competitors all ship | 6–10 wk |
| P1.11 | **Encryption at rest** (LMDB + record-store + WAL hooks) | FedRAMP / SOC2 compliance gate | 2–3 wk |
| P1.12 | **SSO / OIDC / SAML adapters** | Same compliance gate | 2 wk |
| P1.13 | **Automatic failover** for v1 master-replica path | Single-node + replica deployment story | 2 wk |
| P1.14 | **Cluster perf benchmarks** + chaos / failure injection for V2 Raft | Production-readiness gating | 2 wk |
| P1.15 | **Cypher LSP** (VS Code + IntelliJ extensions) | Developer onboarding | 1–2 wk |
| P1.16 | **Migration guides FROM Neo4j and Memgraph** | Mirror competitive positioning | 1 wk each |

**P1 total: ~38–50 engineer-weeks.**

## P2 — strategic positioning (3–9 months)

| # | Item | Why | Effort |
|---|------|-----|--------|
| P2.1 | **WASM build of `nexus-core` + JS bindings** | Capture the Kuzu-WASM vacancy; embedded RAG pipelines | 4–6 wk |
| P2.2 | **Spring Data Nexus** | JVM enterprise ORM-ish layer | 2–3 wk (after P1.4) |
| P2.3 | **Online re-sharding** under load with integration tests | Cluster ops gating | 4–6 wk |
| P2.4 | **Multi-region / geo-replication** (V2.1 roadmap) | vs Neo4j Aura, Arango DC2DC | 8–12 wk |
| P2.5 | **GDS algorithm expansion** (target 40+ algos, match Memgraph MAGE / Neo4j GDS 2.20) | Analytical workload story | 1–2 wk per algo family, 6–10 wk for parity tier |
| P2.6 | **Streaming ingest** (Kafka / Pulsar source connectors) | vs Memgraph streaming story | 2–3 wk |
| P2.7 | **Apache Arrow integration** (Arrow Flight, OLAP aggregations) | Analytical interop, Kuzu-fork bait | 2–3 wk |
| P2.8 | **Knowledge-graph embeddings** (Node2Vec, GraphSAGE) | GraphRAG + GNN narrative | 3–4 wk |
| P2.9 | **Temporal graph** (valid-time versioning) | Niche but distinguishing | 4–6 wk |
| P2.10 | **Apache 2.0 forever — public commitment** + matching ArangoDB-3.12.5 messaging ("all features in OSS, no enterprise gate") | Pricing posture vs Neo4j Enterprise + Memgraph BSL | 1 day messaging |
| P2.11 | **Aura-Agent equivalent** — turnkey GraphRAG copilot or LLM-as-DBA wrapper | Match Neo4j Aura Agent, Memgraph GraphRAG toolkit | 6–10 wk |
| P2.12 | **Billion-row KNN benchmark** + comparison vs Pinecone / Weaviate at d=768, k=10, with traversal | Hybrid graph + vector moat marketing | 2 wk |
| P2.13 | **Backup coordination across shards** | V2 production-readiness | 2–3 wk |
| P2.14 | **Cypher row-count semantics + Bolt return-shape conformance** at full Neo4j parity | Last 10 % of compatibility | 2 wk |
| P2.15 | **HiveHub SDK integration unblock** (when external SDK ships) | Multi-tenant cloud story | depends |

**P2 total: ~50–80 engineer-weeks.** Should be tackled in phased waves driven by GTM signals (which customer asks for which).

## Sequencing recommendations

If forced to pick a single 3-month sequence:

1. **Weeks 1–4 (P0 wave)** — re-run vs-Neo4j, fix versions, resolve JIT, fix ignored tests, page-cache TODO, USING INDEX hints, IN TRANSACTIONS, row-count parity, GraphRAG pack, Helm chart. Parallel across 3–4 streams. **Outcome:** clean release-ready v1.3 with up-to-date marketing numbers + integration story.
2. **Weeks 5–10 (P1 wave A — Cypher 25 + ecosystem)** — quantified paths, R-tree predicates, point.nearest, Bolt shim, JVM SDK, plan cache. **Outcome:** v1.4 closes the "Cypher 25 + driver compat" gap; existing Neo4j Java apps can connect.
3. **Weeks 11–14 (P1 wave B — V2 correctness)** — cross-shard 2PC, read-consistency contract, V2 chaos tests + benchmarks. **Outcome:** V2 advertisable as production cluster mode.
4. **Weeks 15–20 (P1 wave C — enterprise gates)** — encryption at rest, SSO/OIDC, automatic failover, GUI MVP. **Outcome:** SOC2/FedRAMP-tier customer asks unblocked.
5. **Weeks 21+ — P2 strategic** — WASM, multi-region, GDS expansion, billion-row benchmarks.

## Deferred / not recommended

- **Replacing the bespoke Raft with openraft** — openraft is still 0.10-alpha; Nexus's own impl has 65 unit tests + 3 scenarios + commit-from-current-term + conflict truncate. Ship-of-Theseus the Rust ecosystem catches up; don't rip-and-replace pre-emptively.
- **Replacing `crc32fast` with hardware CRC32C** — already proven 2× *slower* on Zen 4. Keep dual-format infrastructure; flip default only when AVX-512 VPCLMULQDQ becomes universally available *and* an interop need surfaces.
- **Replacing `serde_json` with `simd-json` for `/ingest`** — 1.32–1.48× *slower* on `Value` targets. Keep as-is.
- **Adding a second column-store engine** — the row-vs-columnar 1.13× ratio at 100 K rows shows materialisation cost dominates SIMD-saturated kernels. Spend the engineering on planner / cardinality propagation instead.
- **A "graph algorithms in CUDA" track** — Memgraph already has cuGraph / MAGE; competing on CUDA is expensive. Better to lean into the binary RPC + linked-list traversal differentiation.

## What success looks like at +3 months

- Re-published vs-Neo4j benchmark **with current build + concurrent workload**. Headline still ≥ 4× avg.
- Cypher 25 parity (quantified paths, IN TRANSACTIONS, full geospatial). 300/300 diff-suite stays at 100 %; openCypher coverage ≥ 80 %.
- Bolt shim shipping. JVM SDK shipping. **Existing Neo4j apps can connect with a config change**.
- V2 cluster mode advertisable for production multi-shard writes.
- GraphRAG cookbook + LangChain integration pack. Migration guides from Neo4j + Kuzu + Memgraph.
- v1.5 release train with single coherent version across server + SDK + CLI.

## What success looks like at +9 months

- WASM / embedded build. Spring Data Nexus. K8s operator.
- Multi-region replication. Online re-sharding. Encrypted at rest. SSO/OIDC.
- 40+ GDS algorithms. Streaming connectors. Arrow Flight.
- Aura-Agent-equivalent or partner-led GraphRAG copilot.
- **Public positioning:** "the open-source, Apache-2.0, single-binary, Cypher + native vector graph DB that beats Neo4j on per-query latency on most workloads, scales to multi-shard writes, embeds in browsers via WASM, and ships first-party SDKs in 7 languages."

## Risk register for the roadmap itself

| Risk | Mitigation |
|------|-----------|
| Spreading thin across 3 horizons simultaneously | Single-team, single-phase discipline; keep `team-lead` orchestration; only spawn parallel streams where genuinely independent |
| GraphRAG narrative ages out before Nexus ships its pack | P0.10 first, even ahead of Cypher 25 polish |
| Bolt shim turns into a quagmire (Neo4j Bolt v5 spec is large) | Scope to read-only point + traversal queries first; iterate |
| Cross-shard 2PC bug costs months | Take the pessimistic-locking path first; 2PC as a v2 of the same feature |
| Kuzu forks consolidate before Nexus captures users | P0.9 + P2.1 (WASM) accelerated; ship migration scripts early |
| Maintainer bandwidth on 6 SDKs at lockstep version | Consider freezing PHP & Go to a slower cadence; keep Rust + TS + Python on the fast train |
