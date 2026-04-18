# Nexus Implementation Tasks - Complete Roadmap

This document consolidates all OpenSpec implementation tasks organized by phase.

**Total: 345+ tasks across 3 phases (MVP, V1, V2)**

---

## 📋 **MVP (Phase 1)** - 180+ tasks, 8-12 weeks

### **Proposal 1: implement-mvp-storage** (70 tasks, 2-3 weeks)

**Scope**: Storage layer, catalog, transaction manager

#### 1. Catalog Implementation (9 tasks)
- Setup heed (LMDB) with database environment
- Create bidirectional mappings (label/type/key ↔ ID)
- Add statistics storage and metadata persistence
- Implement concurrent access handling
- Add comprehensive tests (95%+ coverage)

#### 2. Record Stores (11 tasks)
- Implement fixed-size records (nodes: 32B, rels: 48B)
- Setup memory-mapped files (memmap2)
- Implement doubly-linked adjacency lists
- Property chains with overflow
- Strings dictionary with CRC32
- File growth strategy (2x expansion)
- Comprehensive tests

#### 3. Page Cache (11 tasks)
- 8KB pages with header + body
- Clock eviction algorithm
- Pin/unpin semantics with reference counting
- Dirty page tracking and flush
- xxHash3 checksum validation
- Concurrency tests

#### 4. Write-Ahead Log (11 tasks)
- WAL binary format (epoch, tx_id, payload, CRC32)
- Append operation with fsync
- Checkpoint mechanism
- Crash recovery (replay entries)
- WAL archiving
- Corruption detection tests

#### 5. Transaction Manager (12 tasks)
- Epoch-based MVCC
- Single-writer locking (parking_lot)
- Begin/commit/abort operations
- Snapshot isolation
- Version visibility rules
- Garbage collection
- Timeout handling

#### 6. Integration & Testing (10 tasks)
- E2E tests (create, read, update, traverse)
- Crash recovery tests
- Concurrency tests
- Performance benchmarks (100K+ reads/sec)

#### 7. Documentation (5 tasks)
- Update ROADMAP, ARCHITECTURE
- Add usage examples
- Update CHANGELOG

#### 8. Quality Gates (8 tasks)
- fmt, clippy, test, coverage, codespell
- Release build, performance validation

---

### **Proposal 2: implement-mvp-indexes** (30 tasks, 1 week)

**Scope**: Label bitmap, KNN vector index

#### 1. Label Bitmap Index (9 tasks)
- RoaringBitmap per label
- Add/remove node operations
- Bitmap operations (AND, OR, NOT)
- Cardinality estimation
- Persistence
- Tests

#### 2. KNN Vector Index (10 tasks)
- hnsw_rs integration (M, ef_construction)
- Add vector with normalization
- Search KNN (k, ef_search)
- Node ID ↔ embedding index mapping
- Distance metrics (cosine, euclidean)
- Index persistence (custom binary format)
- Recall@k benchmarks
- Performance tests (10K+ queries/sec)

#### 3. Index Statistics (6 tasks)
- Track counts per label/type
- Track NDV per property
- Statistics persistence
- Tests

#### 4. Integration & Testing (5 tasks)
- Integration with storage layer
- Multi-label queries
- Performance benchmarks
- Coverage validation

---

### **Proposal 3: implement-mvp-executor** (50 tasks, 2-3 weeks)

**Scope**: Cypher parser, planner, operators

#### 1. Cypher Parser (10 tasks)
- AST definition
- MATCH, WHERE, RETURN, ORDER BY, LIMIT parsing
- Parameter substitution
- Aggregation functions
- Syntax error reporting
- Tests (95%+ coverage)

#### 2. Query Planner (8 tasks)
- Cost model with statistics
- Pattern reordering (selectivity)
- Index selection
- Filter pushdown
- Limit pushdown
- Plan visualization (EXPLAIN)
- Tests

#### 3. Physical Operators (9 tasks)
- NodeByLabel, Filter, Expand
- Project, OrderBy, Limit
- Aggregate (hash aggregation)
- Operator pipelining
- Tests per operator

#### 4. Aggregation Functions (8 tasks)
- COUNT(*), COUNT(expr)
- SUM, AVG, MIN, MAX
- GROUP BY logic
- Tests

#### 5. Integration & Testing (8 tasks)
- E2E query tests (MATCH, WHERE, aggregation)
- Pattern traversal (2-hop, 3-hop)
- Performance tests (<10ms latency, 1K+ queries/sec)
- Coverage validation

#### 6. Documentation & Quality (4 tasks)
- Update ROADMAP, README
- Add query examples
- CHANGELOG, quality checks

---

### **Proposal 4: implement-mvp-api** (30 tasks, 1 week)

**Scope**: REST endpoints, streaming, error handling

#### 1. Cypher Endpoint (6 tasks)
- Connect to executor
- Parameter validation
- Timeout handling
- Error formatting
- Response formatting
- Tests

#### 2. KNN Traverse Endpoint (7 tasks)
- Vector dimension validation
- KNN search execution
- Graph expansion
- WHERE filters
- Execution time breakdown
- Tests

#### 3. Ingest Endpoint (6 tasks)
- Parse bulk request
- Batch operations
- Partial failure handling
- Throughput metrics
- Tests

#### 4. Streaming Support (5 tasks)
- Server-Sent Events (SSE)
- Chunked transfer encoding
- Backpressure handling
- Timeout
- Tests

#### 5. Integration & Testing (6 tasks)
- API tests (cypher, knn, ingest)
- Error handling tests (400, 408, 500)
- Performance tests (API throughput)
- Coverage validation

---

## 🎯 **V1 (Phase 2)** - 120+ tasks, 6-8 weeks

### **Proposal 5: implement-v1-authentication** (35 tasks, 1 week)

#### 1. API Key Management (8 tasks)
- ApiKey struct, Argon2 hashing
- API key generation (32-char)
- Storage in catalog
- POST /auth/keys, GET /auth/keys, DELETE /auth/keys
- Tests

#### 2. Authentication Middleware (6 tasks)
- Bearer token extraction
- API key validation
- Require auth for 0.0.0.0 binding
- 401 Unauthorized responses
- Tests

#### 3. RBAC (4 tasks)
- Permission enum (READ, WRITE, ADMIN, SUPER)
- Permission checking per endpoint
- 403 Forbidden responses
- Tests

#### 4. Rate Limiting (5 tasks)
- Token bucket algorithm
- Track per minute/hour per API key
- 429 Too Many Requests
- X-RateLimit-* headers
- Tests

#### 5. JWT Support (5 tasks)
- JWT generation and validation
- POST /auth/login
- Token expiration
- Tests

#### 6. Audit Logging (5 tasks)
- Log write operations
- Persist to file
- Log rotation
- Tests

#### 7. Documentation & Quality (4 tasks)

---

### **Proposal 6: implement-v1-replication** (35 tasks, 2 weeks)

#### 1. Master Node (7 tasks)
- WAL streaming to replicas
- Track connected replicas
- Async/sync replication modes
- Circular replication log
- Replica health monitoring
- Tests

#### 2. Replica Node (7 tasks)
- Connect to master via TCP
- Receive and apply WAL entries
- CRC32 validation
- Send ACK (sync mode)
- Auto-reconnect (exponential backoff)
- Replication lag tracking
- Tests

#### 3. Full Sync (7 tasks)
- Create snapshot (tar.zst)
- CRC32 checksum
- Transfer to replica
- Verify and load
- Switch to incremental sync
- Tests

#### 4. Failover Support (6 tasks)
- Health check endpoint
- Heartbeat monitoring
- Failure detection
- Replica promotion (POST /replication/promote)
- Update catalog role
- Failover tests

#### 5. Replication API (6 tasks)
- GET /replication/status
- POST /replication/promote
- POST /replication/pause/resume
- GET /replication/lag
- API tests

#### 6. Documentation & Quality (4 tasks)

---

### **Proposal 7: implement-v1-gui** (50 tasks, 3 weeks)

#### 1. Electron Setup (6 tasks)
- Initialize project, Vue 3 + Vite
- TailwindCSS, IPC communication
- Auto-updater, build scripts

#### 2. Graph Visualization (8 tasks)
- Cytoscape.js force-directed layout
- Node/relationship rendering
- Zoom/pan, selection
- Property inspector, filtering
- Tests

#### 3. Query Editor (8 tasks)
- CodeMirror with Cypher syntax
- Query execution
- Table/graph view toggle
- Query history, saved queries
- Export (JSON, CSV)
- Tests

#### 4. KNN Search Interface (6 tasks)
- Text input, Vectorizer integration
- Visual similarity results
- Hybrid query builder
- Vector index management
- Tests

#### 5. Monitoring Dashboard (7 tasks)
- Chart.js setup
- Metrics charts (throughput, cache, WAL, lag)
- Real-time updates (WebSocket)
- Tests

#### 6. Management Tools (7 tasks)
- Schema browser
- Index management UI
- Backup/restore UI
- Replication control
- Configuration editor, log viewer
- Tests

#### 7. Build & Package (5 tasks)
- Windows MSI, macOS DMG, Linux AppImage
- Test installers
- Auto-update mechanism

#### 8. Documentation & Quality (5 tasks)

---

## 🚀 **V2 (Phase 3)** - 45+ tasks, 12-16 weeks

### **Proposal 8: implement-v2-sharding** (45 tasks)

#### 1. Shard Management (5 tasks)
- Hash-based assignment
- Shard metadata
- Rebalancing
- Health monitoring
- Tests

#### 2. Raft Consensus (5 tasks)
- openraft integration
- Leader election
- Log replication
- Snapshot transfer
- Tests

#### 3. Distributed Query Coordinator (6 tasks)
- Shard identification
- Plan decomposition
- Scatter/gather execution
- Result merging
- Pushdown optimizations
- Tests

#### 4. Cross-Shard Traversal (4 tasks)
- Remote node fetching
- Edge caching
- Network hop minimization
- Tests

#### 5. Cluster Management API (5 tasks)
- GET /cluster/status
- POST /cluster/add_node
- POST /cluster/remove_node
- POST /cluster/rebalance
- Tests

#### 6. Integration & Testing (6 tasks)
- Distributed query tests
- Failover tests
- Partition tolerance tests
- Scalability benchmarks
- Coverage validation

#### 7. Documentation & Quality (4 tasks)
- Deployment guide
- CHANGELOG v1.0.0
- Quality checks

---

## 📊 **Summary by Phase**

| Phase | Proposals | Tasks | Duration | Complexity |
|-------|-----------|-------|----------|------------|
| **MVP** | 4 | 180+ | 8-12 weeks | High |
| **V1** | 3 | 120+ | 6-8 weeks | Medium-High |
| **V2** | 1 | 45+ | 12-16 weeks | Very High |
| **TOTAL** | **8** | **345+** | **26-36 weeks** | **6-9 months** |

---

## 🎯 **Implementation Order**

### **Sequential Dependencies**

```
MVP Storage (2-3 weeks)
    ↓
MVP Indexes (1 week)
    ↓
MVP Executor (2-3 weeks)
    ↓
MVP API (1 week)
    ↓ MVP COMPLETE
V1 Authentication (1 week)
    ↓
V1 Replication (2 weeks)
    ↓
V1 GUI (3 weeks)
    ↓ V1 COMPLETE
V2 Sharding (12-16 weeks)
    ↓ V2 COMPLETE
```

### **Parallel Opportunities**

After MVP:
- V1 Authentication + V1 GUI (can be parallel)
- V1 Replication (depends on auth, blocks V2)

---

## 📝 **Next Steps**

1. **Review proposals** in `openspec/changes/`
2. **Start with** `implement-mvp-storage` (foundational)
3. **Follow tasks.md** checklist sequentially
4. **Update** tasks with `[x]` as completed
5. **Archive** proposal after deployment

---

## 📚 **Related Documentation**

- `docs/ROADMAP.md` - High-level timeline
- `docs/ARCHITECTURE.md` - Technical architecture
- `docs/specs/` - Detailed specifications
- `openspec/project.md` - Project conventions
- `openspec/AGENTS.md` - OpenSpec workflow

---

**All proposals ready for implementation!** 🚀

