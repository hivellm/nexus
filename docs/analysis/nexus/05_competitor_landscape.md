# 05 — Competitor Landscape (2025–2026)

Snapshot taken 2026-04-28. Sources cited inline. Focus: graph databases that compete on Cypher / vector / hybrid-RAG; pure vector DBs evaluated for the vector axis only.

## 1. Neo4j (2025.x / Aura)

- **Position:** tier-1 incumbent, ~44 % graph DBMS market share; **>$200M ARR (early 2025)**, ~$2B valuation, $50M raise from Noteus Partners; total funding ~$581M; >1,000 enterprise customers (75 % of Fortune 100).
- **Query / ecosystem:** Cypher 25, GDS 2.20 (Maximum Flow + Min-Cost Max Flow GA in 2025.07), Bloom GUI, broad LangChain/LlamaIndex integration, Aura Agent (GraphRAG copilot, 2025).
- **Vector:** Native `VECTOR` property type since **2025.10**; vector-3.0 provider since 2025.09; lower-memory ANN search in Aura April 2025 release.
- **Distributed/HA:** Causal cluster (Enterprise) + Aura managed multi-region. Bulk-import path bypasses tx overhead in Aura 2025.
- **License:** Community = GPLv3 (single-instance only); Enterprise commercial; Aura usage-based.
- **Vs Nexus:** Nexus is faster on 73 / 74 measured Cypher operations (avg 4.15×) and ships native vector at parity. Neo4j wins on ecosystem depth (Bloom, GDS algo coverage, Aura Agent), Java/Spring Data driver mindshare, and managed cloud.

## 2. Memgraph (3.4)

- **Position:** niche-but-growing in-memory Cypher peer; total funding ~$14.2M (M12 seed 2021); positioned as "real-time / low-latency Neo4j peer."
- **Query / ecosystem:** Bolt + Cypher; **MAGE** extensions (40+ algos in C++/Python/CUDA, including cuGraph GPU-accelerated images). MAGE merged into main repo early 2026.
- **Vector:** Vector indexes since 3.x; **edge vector indexes + scalar quantization** in **3.4 (2025)**; vector data stored only in the index (no property-store dup).
- **Distributed/HA:** HA replication in Enterprise; in-memory model bounds dataset to RAM.
- **License:** Community under **BSL** (BSL-restricted free); Enterprise commercial.
- **Recent:** Nov-2025 GraphRAG toolkit launch for "non-graph users."
- **Vs Nexus:** Both ship Cypher + vector. Nexus is on-disk + page-cache (no RAM ceiling); Memgraph is RAM-bound but lower latency for hot working sets. Memgraph has a stronger streaming / ETL story and longer commercial track record.

## 3. ArangoDB (3.12.5+)

- **Position:** strongest "single-store multi-model" pitch (document + graph + KV + search + vector); mid-tier in pure-graph mindshare.
- **Query / ecosystem:** AQL, ArangoSearch (FTS), Foxx microservices, Pregel.
- **Vector:** New vector index in **3.12** built on **FAISS**, callable from AQL alongside graph traversal + ArangoSearch ("HybridGraphRAG"). WAND optimisations for SORT+LIMIT.
- **Distributed/HA:** Native sharding + smart joins on graphs (Enterprise); multi-DC.
- **License:** Apache 2.0. **From 3.12.5, all Enterprise features ship in Community Edition** — aggressive open-core reversal.
- **Vs Nexus:** Arango wins on multi-model breadth and Apache-2.0-with-Enterprise-bundled positioning. Nexus wins on graph-native traversal performance and binary RPC. Arango is the **biggest pricing-positioning threat**.

## 4. TigerGraph (4.2)

- **Position:** established MPP graph vendor under stress: ~30 % layoffs (~90 staff) in 2024–2025, new CEO, **Series D-II from Cuadrilla Capital (Jul 2025)** keeping it alive.
- **Query / ecosystem:** GSQL (proprietary, MPP-native, parameterized). Smaller community than Cypher.
- **Vector:** **TigerVector** integrated into 4.2 (Dec 2024); SIGMOD'25 paper. Hybrid search added Mar 2025; **Community Edition** introduced same month.
- **Distributed/HA:** strong native MPP/distributed pedigree; "Savanna" cloud-native (separated compute/storage).
- **License:** Free Community Edition (size-limited), Enterprise commercial, Savanna SaaS.
- **Vs Nexus:** TigerGraph targets analytical / batch graph workloads at petabyte scale. Nexus is OLTP-leaning + vector-first. Limited overlap; TigerGraph is wounded but still the choice for >1B-node analytics.

## 5. Dgraph (Hypermode → Istari Digital)

- **Position:** **declining / in transition.** Dgraph Labs → Hypermode (2023) → **acquired by Istari Digital (Oct 2025)** for AI/engineering data fabric. Repo: `hypermodeinc/dgraph`.
- **Query / ecosystem:** DQL (GraphQL-derived) + native GraphQL. Smaller than Cypher.
- **Vector:** Native vector indexing; hybrid GraphRAG positioning under Hypermode.
- **Distributed/HA:** native distributed (Raft per group, sharded predicates) — historically the differentiator.
- **License:** Apache 2.0 core; Enterprise (ACLs, encryption) commercial.
- **Recent:** acquisition implies enterprise integration vs independent product growth; community uncertainty post-rebrand.
- **Vs Nexus:** distributed-first vs Nexus's single-node-first heritage. Dgraph has stronger sharding pedigree but unclear product future.

## 6. JanusGraph

- **Position:** mature open-source (Linux Foundation), no commercial vendor; rides Cassandra / HBase / ScyllaDB / BerkeleyDB / Bigtable as storage.
- **Query / ecosystem:** TinkerPop **Gremlin** — broad polyglot but harder to learn than Cypher; ecosystem = Apache TinkerPop world.
- **Vector:** No first-class; FTS via Elasticsearch / Solr.
- **Distributed/HA:** inherited from storage backend; well-suited for petabyte-scale Cassandra deployments. Microsoft markets the JanusGraph + Azure Managed Cassandra combo (Dec 2025 blog).
- **License:** Apache 2.0; cost = the underlying storage.
- **Vs Nexus:** JanusGraph is the choice when "we already have Cassandra, just put a graph layer on top." Nexus is the choice when "we want a single binary."

## 7. NebulaGraph (5.2)

- **Position:** strong in China, modest internationally. Vesoft-developed, distributed shared-nothing.
- **Query / ecosystem:** nGQL (SQL-like, NOT openCypher; partial Cypher support exists). Isolated ecosystem.
- **Vector:** **Native vector search added in Enterprise 5.1 (2025); 5.2 added native full-text** for hybrid search inside one query.
- **Distributed/HA:** shard + Raft from the start; one of the better horizontally scalable property graph stores.
- **License:** Apache 2.0 core; Enterprise commercial (vector + FTS are Enterprise-only).
- **Vs Nexus:** NebulaGraph is more proven at horizontal scale; Nexus is more open (FTS / vector are free in Nexus) and has Cypher in CE. NebulaGraph's traction outside China is limited.

## 8. KuzuDB — **archived 2025-10-10**

- **Position:** until October was the leading "DuckDB-for-graphs" embedded analytics engine; closest peer to Nexus on per-node performance claims.
- **Query / ecosystem:** Cypher (subset), Python / Node / Rust / Java embedded bindings. v0.8.0 (early 2025) added **Kuzu-WASM** in-browser, **FTS (BM25)**, parallel hash aggregation; v0.8.2 added Delta Lake / GCS scans.
- **Vector:** Native vector + FTS; popular for GraphRAG.
- **Distributed/HA:** none (embedded / single-node by design).
- **Recent:** Kùzu Inc. archived the GitHub repo on **2025-10-10**. Multiple forks now: **Bighorn (Kineviz), Ladybug (Arun Sharma), RyuGraph (Predictable Labs)**, Vela Partners' multi-writer fork. FalkorDB published a Kuzu→FalkorDB migration guide. The category is now fragmented.
- **License:** MIT, free; no commercial vendor.
- **Vs Nexus — strategic:** **the Kuzu vacancy is Nexus's biggest near-term opportunity**. Forks are early. Nexus is single-binary, Cypher-first, vector-native, and has commercial momentum. Window to capture displaced Kuzu users: ~12 months before forks mature or FalkorDB consolidates.

## 9. FalkorDB

- **Position:** successor to **RedisGraph (EOL Jan 31, 2025)**; GraphRAG-focused, sparse-matrix engine.
- **Query / ecosystem:** openCypher (GraphBLAS-backed), Redis client compatibility (Bolt-on protocol over Redis), strong text-to-Cypher tooling, "QueryWeaver" text-to-SQL.
- **Vector:** native vector indexes on nodes + relationships; Cypher-extended similarity functions.
- **Distributed/HA:** inherits Redis cluster sharding + replication (pragmatic but not graph-native).
- **License:** source-available (SSPL-style) core; embedded `falkordblite` for Python; commercial cloud.
- **Funding:** $3M seed Jun-2024 (Angular Ventures, ex-Redis founders, Google/Firebolt angels).
- **Vs Nexus:** FalkorDB is Redis-bound (operational lock-in); Nexus is independent. Both target GraphRAG. FalkorDB's GraphBLAS is fast on dense aggregates; Nexus is faster on traversal (linked-list O(1)).

## 10. Pure vector DBs (vector axis only)

| DB | Funding | Position |
|----|---------|----------|
| **Pinecone** | ~$138M, $2.75B valuation | Tier-1 commercial managed; expensive at scale, zero ops; closed source |
| **Weaviate** | ~$67.6M, est. $500M+ valuation | OSS BSD-3 + Cloud; strongest hybrid (BM25 + vector) + multimodal modules |
| **Qdrant** | ~$28M | OSS Apache 2.0, Rust; best price/perf for low-latency, high-throughput |
| **Milvus / Zilliz** | ~$113–115M Series B-II (2022) | OSS Apache 2.0; **40 K+ GitHub stars in 2025**; billion-scale workloads, heavy ops |

None offers true property-graph traversal; their "graph" stories are at best metadata filtering. **Hybrid graph + vector is the moat for graph-native vendors.** Threat: Weaviate / Qdrant ship "graph-lite" features (cross-references, link traversal) faster than Nexus closes its ANN-at-billion-scale gap.

## Comparative table

| DB | Query lang | License | Native vector | Distributed | Embedded | Perf claim | Target use |
|----|-----------|---------|---------------|-------------|----------|-----------|------------|
| **Nexus 1.13** | Cypher (~55 % openCypher, 300/300 diff) | Apache 2.0 | Yes (HNSW per-label) | V2 sharding + Raft (single-shard writes) | Single binary; not yet WASM | 4.15× avg vs Neo4j on 74-test | Graph + vector RAG, mid-scale OLTP |
| **Neo4j 2025.x** | Cypher 25 | GPLv3 / commercial | Yes (VECTOR type 2025.10) | Causal cluster (Ent) / Aura | No | mature, balanced OLTP+graph | Enterprise knowledge graphs, GraphRAG |
| **Memgraph 3.4** | Cypher | BSL / commercial | Yes (nodes + edges, quantized) | HA replication | No | in-memory low-latency | Streaming graphs, real-time RAG |
| **ArangoDB 3.12.5+** | AQL | Apache 2.0 (Ent bundled) | Yes (FAISS) | Native sharding | No | multi-model balance | Hybrid doc+graph+vector |
| **TigerGraph 4.2** | GSQL | Free CE / commercial | Yes (TigerVector) | MPP native | No | high-parallel huge graphs | Analytical graph + RAG |
| **Dgraph (Istari)** | DQL/GraphQL | Apache 2.0 core | Yes | Native (Raft groups) | No | distributed-first | AI agent memory, GraphQL apps |
| **JanusGraph** | Gremlin | Apache 2.0 | No (FT via ES/Solr) | via Cassandra/HBase/Scylla | No | petabyte via backend | Hyperscale on existing infra |
| **NebulaGraph 5.2** | nGQL | Apache 2.0 / Ent | Yes (Ent only) | Shard + Raft | No | horizontal scale | Large-scale graph (CN-heavy) |
| **Kuzu (archived) / forks** | Cypher | MIT | Yes (+FTS) | No (embedded) | **Yes (in-proc, WASM)** | fastest single-node OLAP graph | Embedded analytics, RAG pipelines |
| **FalkorDB** | openCypher | SSPL-ish / commercial | Yes (nodes + rels) | Redis cluster | Yes (`falkordblite`) | GraphBLAS sparse-matrix | LLM knowledge graphs |
| Pinecone / Weaviate / Qdrant / Milvus | proprietary REST | mixed | Yes (core) | native | mostly no | billion-scale ANN | pure vector / RAG retrieval |

## Strategic read for Nexus

1. **The Kuzu archival (Oct 2025) leaves a clear vacancy in the embedded-friendly Cypher + vector + fast quadrant — exactly Nexus's positioning.** Forks are early; FalkorDB is the most aggressive incumbent claimant of displaced users. **Window: ~12 months**.
2. **On the cloud / distributed side, Neo4j + Aura, ArangoDB-with-Enterprise-in-CE, and NebulaGraph 5.2 are the structural threats**; TigerGraph and Dgraph are wounded incumbents.
3. **ArangoDB 3.12.5's "all Enterprise features in CE" move weaponizes pricing** against Neo4j. Nexus should match the messaging: "Apache-2.0 forever, no enterprise gate, all features in OSS."
4. **Neo4j Aura Agent (GraphRAG copilot) and Memgraph's Nov-2025 GraphRAG toolkit** are the new "narrative" battleground. Nexus has no equivalent — even a thin LangChain/LlamaIndex integration pack closes the gap.
5. **Pure-vector DBs adding metadata-filter "graph-lite"** is a slow-erosion threat. Counter: publish a graph + vector benchmark vs Pinecone/Weaviate at d=768, k=10, with traversal in the same query. This is where Nexus's binary RPC + bytes-native embeddings + linked-list traversal compounds.

## Sources

- Neo4j 2025: https://neo4j.com/blog/news/2025-ai-scalability/ ; https://neo4j.com/press-releases/neo4j-revenue-milestone-2024/ ; vector docs https://neo4j.com/docs/cypher-manual/current/indexes/semantic-indexes/vector-indexes/
- Memgraph 3.4: https://memgraph.com/blog/memgraph-3-4-release-announcement ; vector docs https://memgraph.com/docs/querying/vector-search ; GraphRAG toolkit https://www.businesswire.com/news/home/20251111832729/en/
- ArangoDB 3.12: https://docs.arangodb.com/3.12/release-notes/version-3.12/whats-new-in-3-12/ ; vector blog https://arango.ai/blog/vector-search-in-arangodb-practical-insights-and-hands-on-examples/
- TigerVector / TigerGraph 4.2: https://arxiv.org/abs/2501.11216 ; layoffs/Series D-II: https://www.trueup.io/co/tigergraph/layoffs ; https://siliconangle.com/2025/03/04/tigergraph-adds-hybrid-search-capability-graph-database-releases-free-edition/
- Istari Digital + Dgraph: https://www.prnewswire.com/news-releases/istari-digital-acquires-dgraph-to-strengthen-data-foundation-for-ai-and-engineering-302593246.html
- JanusGraph + Azure Cassandra: https://devblogs.microsoft.com/cosmosdb/janusgraph-azure-cassandra-graph-databases/
- NebulaGraph 2025: https://www.nebula-graph.io/posts/NebulaGraph_2025_Year_in_Review
- Kuzu archival: https://www.theregister.com/2025/10/14/kuzudb_abandoned/ ; forks https://gdotv.com/blog/weekly-edge-kuzu-forks-duckdb-graph-cypher-24-october-2025/ ; FalkorDB migration https://www.falkordb.com/blog/kuzudb-to-falkordb-migration/
- FalkorDB seed: https://www.crunchbase.com/funding_round/falkordb-seed--663ce4f6 ; RedisGraph EOL https://www.falkordb.com/blog/redisgraph-eol-migration-guide/
- Vector DB landscape: https://tensorblue.com/blog/vector-database-comparison-pinecone-weaviate-qdrant-milvus-2025 ; Pinecone valuation https://www.secondtalent.com/resources/pinecone-vs-weaviate-vs-qdrant-vs-pgvector/ ; Weaviate funding https://wellfound.com/company/weaviate/funding ; Zilliz https://techcrunch.com/2022/08/24/zilliz-the-startup-behind-the-milvus-open-source-vector-database-for-ai-applications-raises-60m-and-relocates-to-sf/ ; Milvus 40K stars https://finance.yahoo.com/news/milvus-surpasses-40-000-github-010000562.html
