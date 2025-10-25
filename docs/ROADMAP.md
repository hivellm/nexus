# Nexus Development Roadmap

This document outlines the phased implementation plan for Nexus graph database.

## Status Legend

- ✅ Completed
- 🚧 In Progress
- 📋 Planned
- 🔮 Future (Post V2)

---

## Phase 0: Foundation (Current)

**Goal**: Architecture documentation and project scaffolding

### Tasks

- ✅ Architecture design documentation
- ✅ Cargo workspace setup (edition 2024, nightly)
- ✅ Core module scaffolding (nexus-core)
- ✅ Server scaffolding (nexus-server)
- ✅ Protocol layer scaffolding (nexus-protocol)
- 📋 Integration tests structure
- 📋 CI/CD workflows (test, lint, coverage)

**Deliverable**: Buildable workspace with comprehensive documentation

---

## Phase 1: MVP - Single Node Engine

**Goal**: Working graph database with basic Cypher and native KNN

**Timeline**: 8-12 weeks

### 1.1 Storage Foundation (Week 1-2) ✅ COMPLETED

- ✅ **Catalog Implementation**
  - LMDB/heed integration (10GB max, 8 databases)
  - Label/Type/Key → ID bidirectional mappings
  - Statistics storage (node counts, relationship counts)
  - Schema metadata (version, epoch, page_size)
  - 21 tests, 98.64% coverage

- ✅ **Record Stores**
  - nodes.store: Fixed 32-byte records with memmap2
  - rels.store: Fixed 48-byte records with linked lists
  - Automatic file growth (1MB → 2x exponential)
  - Label bitmap (64 labels per node)
  - Doubly-linked adjacency lists for O(1) traversal
  - 18 tests, 96.96% coverage

- ✅ **Page Cache**
  - 8KB pages with Clock eviction algorithm
  - Pin/unpin semantics with atomic reference counting
  - Dirty page tracking with HashSet
  - xxHash3 checksums for corruption detection
  - Statistics (hits, misses, evictions, hit rate)
  - 21 tests, 96.15% coverage

**Test Coverage**: ✅ 96%+ for storage layer (exceeds 95% requirement)

### 1.2 Durability & Transactions (Week 3-4) ✅ COMPLETED

- ✅ **Write-Ahead Log (WAL)**
  - Append-only log with 10 entry types
  - CRC32 validation for data integrity
  - fsync on commit for durability
  - Checkpoint mechanism with statistics
  - WAL replay for crash recovery
  - Truncate operation for WAL rotation
  - 16 tests, 96.71% coverage

- ✅ **MVCC Implementation**
  - Epoch-based snapshots for snapshot isolation
  - Single-writer model (parking_lot Mutex)
  - Begin/commit/abort transaction logic
  - Visibility rules (created_epoch <= tx_epoch < deleted_epoch)
  - Transaction statistics tracking
  - 20 tests, 99.02% coverage

- ✅ **Integration Tests**
  - 15 end-to-end tests covering all modules
  - Performance benchmarks (>100K reads/sec, >10K writes/sec)
  - Crash recovery validation
  - MVCC snapshot isolation verification
  - Concurrent access (5 readers + 3 writers)

**Test Coverage**: ✅ 96.06% global coverage (exceeds 95% requirement)
**Total Tests**: ✅ 133 tests (118 unit + 15 integration), 100% passing

### 1.3 Basic Indexes (Week 5)

- 📋 **Label Bitmap Index**
  - RoaringBitmap per label
  - Add/remove node from label
  - Cardinality statistics
  - Bitmap operations (AND, OR, NOT)

- 📋 **KNN Vector Index**
  - HNSW index per label (hnsw_rs)
  - node_id → embedding_idx mapping
  - Cosine similarity support
  - Configurable M, ef_construction parameters

**Test Coverage**: 95%+ with benchmark comparisons

### 1.4 Cypher Executor (Week 6-8)

- 📋 **Parser**
  - Basic pattern syntax: `(n:Label)-[r:TYPE]->(m)`
  - WHERE predicates: `=`, `>`, `<`, `>=`, `<=`, `!=`, `AND`, `OR`
  - RETURN projection
  - ORDER BY + LIMIT
  - Simple aggregations: COUNT, SUM, AVG, MIN, MAX

- 📋 **Physical Operators**
  - NodeByLabel: Scan label bitmap
  - Filter: Property predicates (vectorized if possible)
  - Expand: Traverse linked lists (OUT, IN, BOTH)
  - Project: Return expressions
  - OrderBy + Limit: Top-K heap
  - Aggregate: Hash aggregation

- 📋 **Query Planner**
  - Heuristic cost-based planning
  - Pattern reordering by selectivity
  - Index selection (label bitmap vs KNN)
  - Filter pushdown

**Test Coverage**: 95%+ with TPC-like graph queries

### 1.5 HTTP API (Week 9)

- 📋 **REST Endpoints**
  - POST /cypher: Execute queries
  - POST /knn_traverse: KNN-seeded traversal
  - POST /ingest: Bulk data loading
  - GET /health: Health check
  - GET /stats: Database statistics

- 📋 **Streaming Support**
  - Server-Sent Events (SSE) for large results
  - Chunked transfer encoding
  - Backpressure handling

**Test Coverage**: 95%+ with integration tests

### 1.6 MVP Integration & Testing (Week 10-12)

- 📋 **End-to-End Tests**
  - Sample graph datasets (social network, knowledge graph)
  - Cypher query test suite
  - KNN + traversal hybrid queries
  - Crash recovery scenarios

- 📋 **Performance Benchmarks**
  - Point reads: 100K+ ops/sec target
  - KNN queries: 10K+ ops/sec target
  - Pattern traversal: 1K-10K ops/sec
  - Bulk ingest: 100K+ nodes/sec

- 📋 **Documentation**
  - User guide with examples
  - API reference (OpenAPI spec)
  - Deployment guide
  - Performance tuning guide

**MVP Deliverable**: Production-ready single-node graph database with native KNN

---

## Phase 2: V1 - Quality & Features

**Goal**: Advanced indexes, constraints, optimization

**Timeline**: 6-8 weeks

### 2.1 Advanced Indexes (Week 13-14)

- 📋 **Property B-tree Index**
  - Composite key: (label_id, key_id, value)
  - Range queries: `WHERE n.age > 18`
  - Unique constraints enforcement
  - Index statistics (NDV, histograms)

- 📋 **Full-Text Search**
  - Tantivy integration per (label, key)
  - BM25 scoring
  - Fuzzy search, phrase queries
  - CALL text.search() procedure

**Test Coverage**: 95%+ with query performance tests

### 2.2 Constraints & Schema (Week 15-16)

- 📋 **Constraint Types**
  - UNIQUE(label, key): Enforce uniqueness
  - NOT NULL(label, key): Required properties
  - CHECK(predicate): Custom validation
  - FOREIGN KEY: Relationship integrity (optional)

- 📋 **Schema Evolution**
  - ADD/DROP constraint migrations
  - Index creation/deletion
  - Online schema changes (background)

**Test Coverage**: 95%+ with constraint violation tests

### 2.3 Query Optimization (Week 17-18)

- 📋 **Advanced Planner**
  - Cost model with actual statistics
  - Join order optimization
  - Index-only scans where possible
  - Common subexpression elimination

- 📋 **Query Cache**
  - Prepared statement caching
  - Result caching for read-only queries
  - Cache invalidation on writes

- 📋 **Execution Optimizations**
  - Vectorized filter evaluation (SIMD)
  - Parallel execution (thread pool)
  - Pipelining operators
  - Lazy materialization

**Test Coverage**: 95%+ with benchmark regressions

### 2.4 Bulk Loader (Week 19)

- 📋 **Direct Store Generation**
  - Bypass WAL for initial load
  - Sort nodes/relationships for locality
  - Build indexes during load
  - Parallel ingestion

- 📋 **Import Formats**
  - CSV import (Neo4j compatible)
  - NDJSON import
  - GraphML import
  - Custom binary format

**Test Coverage**: 95%+ with large dataset tests

### 2.5 Authentication & Security (Week 20)

- 📋 **API Key Authentication**
  - API key generation and management
  - Argon2 hashing for security
  - Storage in catalog (LMDB)
  - Disabled by default, required for 0.0.0.0 binding

- 📋 **Rate Limiting**
  - Per-API-key limits (1000/min, 10000/hour)
  - X-RateLimit-* headers
  - 429 Too Many Requests responses

- 📋 **RBAC (Role-Based Access Control)**
  - Permissions: READ, WRITE, ADMIN, SUPER
  - User → Roles → Permissions
  - JWT token support
  - Audit logging

**Test Coverage**: 95%+ with security scenario tests

### 2.6 Replication System (Week 21-22)

- 📋 **Master-Replica Architecture**
  - Async replication (default)
  - Sync replication (optional quorum)
  - Read-only replicas
  - WAL streaming to replicas

- 📋 **Replication Protocol**
  - Full sync via snapshot transfer
  - Incremental sync via WAL stream
  - Circular replication log (1M operations)
  - Auto-reconnect with exponential backoff

- 📋 **Failover Support**
  - Health monitoring (heartbeat)
  - Manual promotion (POST /replication/promote)
  - Automatic failover (V2)
  - Replication lag monitoring

**Test Coverage**: 95%+ with failover tests

### 2.7 Desktop GUI (Electron) (Week 23-25)

- 📋 **GUI Foundation**
  - Electron app structure
  - Vue 3 + TailwindCSS
  - IPC communication with server
  - Auto-updater integration

- 📋 **Graph Visualization**
  - Force-directed graph layout (D3.js/Cytoscape.js)
  - Node/relationship filtering
  - Property inspector
  - Interactive zoom/pan

- 📋 **Query Interface**
  - Cypher editor (CodeMirror with syntax highlighting)
  - Query execution
  - Result table/graph view toggle
  - Query history and saved queries

- 📋 **Management Features**
  - Schema browser
  - Index management
  - Backup/restore UI
  - Replication monitoring
  - Performance dashboard (Chart.js)

- 📋 **KNN Search UI**
  - Text input with embedding generation
  - Visual similarity results
  - Hybrid query builder

**Test Coverage**: 95%+ with E2E GUI tests

### 2.8 Monitoring & Operations (Week 26)

- 📋 **Metrics Exposure**
  - Prometheus metrics endpoint
  - Query latency histograms
  - Cache hit rates
  - WAL size, checkpoint frequency
  - Replication lag metrics

- 📋 **Operational Tools**
  - Backup/restore utilities
  - Point-in-time snapshots
  - Database compaction
  - Index rebuild

**Test Coverage**: 95%+ with ops scenario tests

**V1 Deliverable**: Production-grade single-node database with replication, GUI, and advanced features

---

## Phase 3: V2 - Distributed Graph

**Goal**: Horizontal scalability via sharding and replication

**Timeline**: 12-16 weeks

### 3.1 Sharding Architecture (Week 21-24)

- 🔮 **Shard Management**
  - Hash(node_id) → shard_id assignment
  - Relationships reside with source node
  - Cross-shard edge pointers
  - Shard metadata catalog

- 🔮 **Data Partitioning**
  - Balanced hash partitioning
  - Range partitioning (optional)
  - Rebalancing strategies
  - Hot shard detection

**Test Coverage**: 95%+ with multi-shard scenarios

### 3.2 Replication (Week 25-28)

- 🔮 **Raft Consensus**
  - openraft integration per shard
  - Leader election
  - Log replication
  - Snapshot transfer

- 🔮 **Read Replicas**
  - Followers serve read-only queries
  - WAL streaming to replicas
  - Causal consistency guarantees
  - Replica lag monitoring

**Test Coverage**: 95%+ with failover tests

### 3.3 Distributed Queries (Week 29-32)

- 🔮 **Query Coordinator**
  - Plan decomposition
  - Shard-aware planning
  - Pushdown optimization (filters, limits)
  - Cross-shard joins

- 🔮 **Execution Runtime**
  - Scatter/gather pattern
  - Streaming results aggregation
  - Partial failure handling
  - Timeout management

**Test Coverage**: 95%+ with distributed query tests

### 3.4 Cluster Operations (Week 33-36)

- 🔮 **Cluster Management**
  - Node discovery (gossip or static config)
  - Health checking
  - Rolling upgrades
  - Shard migration

- 🔮 **Disaster Recovery**
  - Multi-region replication
  - Cross-datacenter latency handling
  - Backup coordination
  - Restore across cluster

**Test Coverage**: 95%+ with chaos engineering tests

**V2 Deliverable**: Distributed graph database with multi-node scalability

---

## Phase 4: Future Enhancements (Post V2)

### Graph Algorithms

- 🔮 Shortest path (Dijkstra, A*)
- 🔮 PageRank, centrality measures
- 🔮 Community detection (Louvain)
- 🔮 Graph neural network integration

### Advanced Features

- 🔮 Temporal graph (valid-time versioning)
- 🔮 Geospatial indexes and queries
- 🔮 Graph streaming (Kafka/Pulsar ingestion)
- 🔮 Multi-tenancy and access control
- 🔮 Encryption at rest and in transit

### Analytics Integration

- 🔮 Apache Arrow integration
- 🔮 OLAP-style aggregations
- 🔮 Data export to data lakes
- 🔮 BI tool connectors

### AI/ML Integration

- 🔮 Graph embeddings (Node2Vec, GraphSAGE)
- 🔮 Link prediction models
- 🔮 Anomaly detection
- 🔮 Knowledge graph completion

---

## Success Metrics

### MVP Success Criteria

- ✅ 95%+ test coverage across all modules
- ✅ Zero known critical bugs
- ✅ 100K+ point reads/sec (single node)
- ✅ 10K+ KNN queries/sec
- ✅ Complete API documentation
- ✅ Sample applications (RAG, recommendation)

### V1 Success Criteria

- ✅ All advanced indexes operational
- ✅ Query optimization reduces latency by 50%+
- ✅ Bulk loader achieves 500K+ nodes/sec
- ✅ Production deployments (internal)

### V2 Success Criteria

- ✅ Linear read scalability (add nodes → proportional throughput)
- ✅ < 5% overhead from distribution
- ✅ Automatic failover in < 30 seconds
- ✅ Production deployments (external customers)

---

## Resource Requirements

### MVP (Phase 1)

- **Team**: 1-2 engineers
- **Duration**: 8-12 weeks
- **Hardware**: Development machines (8+ cores, 16GB+ RAM)

### V1 (Phase 2)

- **Team**: 2-3 engineers
- **Duration**: 6-8 weeks
- **Hardware**: Same as MVP + test clusters

### V2 (Phase 3)

- **Team**: 3-4 engineers
- **Duration**: 12-16 weeks
- **Hardware**: Multi-node test clusters (3-5 nodes per cluster)

---

## Risk Management

### Technical Risks

| Risk | Mitigation |
|------|------------|
| Page cache complexity | Start with simple Clock algorithm, optimize later |
| MVCC performance overhead | Benchmark early, tune epoch GC frequency |
| Distributed consensus latency | Careful shard sizing, batch replication |
| Query planner accuracy | Collect statistics, A/B test plans |

### Schedule Risks

| Risk | Mitigation |
|------|------------|
| Feature creep | Strict adherence to MVP scope |
| Testing bottleneck | Automated testing from day 1 |
| Integration delays | Mock external dependencies early |

---

## Dependencies

### External Projects

- **Vectorizer**: KNN embeddings may come from Vectorizer MCP
- **Synap**: Optional pub/sub for graph change events
- **UMICP**: Protocol integration for multi-service graphs

### Internal Milestones

- MVP completion required before V1 start
- V1 production usage required before V2 start
- Each phase gated on 95%+ test coverage

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2024-10-24 | Initial roadmap, Phase 0 scaffolding complete |

---

## Next Steps

1. Complete Phase 0 (scaffolding + docs) ✅
2. Begin Phase 1.1 (Storage Foundation)
3. Weekly sync meetings to track progress
4. Update this roadmap as phases complete

