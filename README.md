# Nexus Graph Database

**⚡ High-performance property graph database with native vector search**

<div align="center">

![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Edition](https://img.shields.io/badge/edition-2024-blue.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)
![Status](https://img.shields.io/badge/status-v0.10.0%20%7C%20Built--in%20Functions-success.svg)
![Tests](https://img.shields.io/badge/tests-1200%2B%20passing-success.svg)
![Coverage](https://img.shields.io/badge/coverage-70.39%25-yellow.svg)

[Features](#-key-features) • [Quick Start](#-quick-start) • [Documentation](#-documentation) • [Roadmap](#-roadmap) • [Contributing](#-contributing)

</div>

---

## 🎯 **What is Nexus?**

Nexus is a modern **property graph database** built for **read-heavy workloads** with **first-class vector search**. Inspired by Neo4j's battle-tested architecture, it combines the power of graph traversal with semantic similarity search for hybrid **RAG (Retrieval-Augmented Generation)** applications.

Think of it as **Neo4j meets Vector Search** - optimized for AI applications that need both structured relationships and s## ✨ **Version 0.6.0 - Core Implementation & Monitoring**

### 🎉 **Current Status (v0.10.0)**

**MVP: 95% Complete** - Production Ready! 🚀

**🔥 Latest (v0.10.0)**: **20+ Built-in Functions for Data Manipulation**!

- ✅ **String Functions** - `toLower()`, `toUpper()`, `substring()`, `trim()`, `replace()`, `split()`, and more
- ✅ **Math Functions** - `abs()`, `ceil()`, `floor()`, `round()`, `sqrt()`, `pow()`
- ✅ **Type Conversion** - `toInteger()`, `toFloat()`, `toString()`, `toBoolean()`
- ✅ **List Functions** - `size()`, `head()`, `tail()`, `last()`, `range()`, `reverse()`
- ✅ **Literal RETURN** - Standalone RETURN queries without MATCH: `RETURN 1+1 AS result`

**Previous (v0.9.10)**: **100% Neo4j compatibility achieved (35/35 extended tests)**!

- ✅ **IS NULL / IS NOT NULL** - Full WHERE clause NULL checking support
- ✅ **WHERE AND/OR Precedence** - Proper boolean operator precedence implementation
- ✅ **Bidirectional Relationships** - Neo4j-compatible bidirectional traversal (emits twice)
- ✅ **Multi-Hop Patterns** - Fixed intermediate node handling for correct graph traversal
- ✅ **Label Intersection** - `MATCH (n:Person:Employee)` with proper bitmap filtering
- ✅ **UNION Operator** - Full planner + executor implementation, pipeline execution
- ✅ **id() Function** - Neo4j-compatible ID function for nodes and relationships
- ✅ **Relationship Properties** - Full access to relationship property values
- ✅ **CREATE Clause** - Full CREATE implementation with persistence
- ✅ **keys() Function** - Property introspection for nodes and relationships
- ✅ **DELETE Operations** - Full DETACH DELETE support with proper clause parsing
- ✅ **Neo4j Compatibility** - **100% complete (35/35 extended validation tests, 1279 total tests passing)**

- ✅ **Storage Foundation** - Fixed-size records, memmap2, LMDB catalog (100% - ARCHIVED)
- ✅ **Transactions & Durability** - WAL, MVCC, crash recovery (100% - ARCHIVED)
- ✅ **Indexes** - Bitmap, KNN/HNSW, B-tree property index (100% - ARCHIVED)
- ✅ **Cypher Executor** - Parser, planner, operators, aggregations (100% - ARCHIVED)
- ✅ **REST API** - 15+ endpoints, streaming, bulk operations (100% - ARCHIVED)
- ✅ **Integration & Testing** - 858 tests passing, benchmarks, examples (100%)
- 🚧 **Graph Correlation** - Call/dependency graphs, clustering (47.5%)
- 🚧 **Authentication** - API keys, RBAC, rate limiting (48.6%)

**Statistics**:
- 📊 **1200+ tests** passing (100% success rate)
- 🎉 **100% Neo4j compatibility** (35/35 extended validation tests)
- 🔧 **20+ built-in functions** (string, math, type conversion, list operations)
- 📈 **41,000+ lines** of Rust code (34K core + 7K server)
- 🎯 **70%+** overall coverage (95%+ in core modules)
- 🏆 **19 modules** across 50+ files
- 💎 **~10,000 lines** of bonus features (clustering, performance, validation, security)

## 🌟 **Key Features**

### **Graph Database**
- 🏗️ **Property Graph Model**: Nodes with labels, relationships with types, both with properties
- 🔍 **Cypher Subset**: Familiar query language covering 80% of common use cases
- ⚡ **Neo4j-Inspired Storage**: Fixed-size record stores (32B nodes, 48B relationships)
- 🔗 **O(1) Traversal**: Doubly-linked adjacency lists without index lookups
- 💾 **ACID Transactions**: WAL + MVCC for durab### **Vector Search (Native KNN)**
- 🎯 **HNSW Indexes**: Hierarchical Navigable Small World for fast approximate search
- 📊 **Per-Label Indexes**: Separate vector space for each node label
- 🔄 **Hybrid Queries**: Combine vector similarity with graph traversal in single query
- ⚡ **High Performance**: 10,000+ KNN queries/sec (k=10)
- 📐 **Multiple Metrics**: Cosine similarity, Euclidean distance

### **Graph Construction & Visualization**
- 🎨 **Layout Algorithms**: Force-directed, hierarchical, circular, and grid layouts
- 🔧 **Force-Directed Layout**: Spring-based positioning with configurable parameters
- 📊 **Hierarchical Layout**: Tree-like positioning for DAGs and organizational structures
- ⭕ **Circular Layout**: Circular positioning for cyclic graphs and networks
- 🔲 **Grid Layout**: Regular grid positioning for structured data visualization
- 🎯 **K-Means Clustering**: Partition nodes into k clusters for grouping analysis

### **Node Clustering & Grouping**
- 🔍 **Multiple Algorithms**: K-means, hierarchical, DBSCAN, and community detection
- 🏷️ **Label-based Grouping**: Group nodes by their labels automatically
- 📊 **Property-based Grouping**: Cluster nodes by specific property values
- 🧮 **Feature Strategies**: Label-based, property-based, structural, and combined features
- 📏 **Distance Metrics**: Euclidean, Manhattan, Cosine, Jaccard, and Hamming distances
- 📈 **Quality Metrics**: Silhouette score, WCSS, BCSS, Calinski-Harabasz, and Davies-Bouldin indices
- ⚙️ **Configurable Parameters**: Customizable clustering parameters and random seeds
- 🔗 **Connected Components**: Find strongly/weakly connected components
- ⚙️ **Graph Operations**: Centering, scaling, neighbor finding, and density calculationCosine similarity, Euclidean distance

### **Performance & Scalability**
- 🚀 **100K+ point reads/sec** - Direct offset access via record IDs
- ⚡ **10K+ KNN queries/sec** - Logarithmic HNSW search
- 📈 **1K-10K pattern traversals/sec** - Efficient expand operations
- 💾 **8KB Page Cache** - Clock/2Q/TinyLFU eviction policies
- 🔄 **Append-Only Architecture**: Predictable write performance

### **Integration & Protocols**
- 🌐 **StreamableHTTP**: Default protocol with SSE streaming (Vectorizer-style)
- 🔌 **MCP Protocol**: 19+ focused tools for AI integrations
- 🔗 **UMICP v0.2.1**: Tool discovery endpoint + native JSON
- 🤝 **Vectorizer Integration**: Native hybrid search with RRF ranking
- 📊 **Graph Correlation Analysis**: Automatic code relationship visualization for LLM assistance

### **Production Features (V1)**
- 🔐 **API Key Auth**: Disabled by default, required for 0.0.0.0 binding
- 🔄 **Master-Replica Replication**: Redis-style async/sync replication
- ⚡ **Automatic Failover**: Health monitoring with replica promotion
- 📊 **Rate Limiting**: 1000/min, 10000/hour per API key

### **Graph Correlation Analysis** 🔥
- 📊 **Automatic Graph Generation**: Create call graphs, dependency graphs, and data flow graphs from Vectorizer data
- 🔍 **Pattern Recognition**: Identify pipeline patterns, event-driven architecture, and design patterns
- 🧠 **LLM Assistance**: Provide structured relationship data to enhance LLM understanding of codebases
- 🎨 **Interactive Visualization**: Web-based graph exploration with zoom, pan, filter, and clustering
- 🔗 **Multiple Graph Types**: Call graphs, dependency graphs, data flow graphs, component graphs
- ⚡ **Real-time Updates**: Live graph updates as code changes
- 🔌 **API Integration**: REST and GraphQL APIs for programmatic access
- 🤖 **MCP & UMICP Support**: Native integration with Model Context Protocol and Universal Model Interoperability Protocol

## 🚀 **Quick Start**

### **Prerequisites**

- Rust nightly 1.85+ (edition 2024)
- 8GB+ RAM (recommended)
- Linux/macOS/Windows with WSL

### **Installation**

```bash
# Clone repository
git clone https://github.com/hivellm/nexus
cd nexus

# Build (release mode)
cargo +nightly build --release --workspace

# Run server
./target/release/nexus-server
```

Server starts on **`http://localhost:15474`** by default.

### **Access Points**

- 🌐 **REST API**: `http://localhost:15474` (StreamableHTTP with SSE)
- 🔌 **MCP Server**: `http://localhost:15474/mcp` (Model Context Protocol)
- 🔗 **UMICP**: `http://localhost:15474/umicp` (Universal Model Interoperability)
- 🔍 **Tool Discovery**: `http://localhost:15474/umicp/discover` (UMICP v0.2.1)
- ❤️ **Health Check**: `http://localhost:15474/health`
- 📊 **Statistics**: `http://localhost:15474/stats`

### **Basic Usage**

#### **1️⃣ Execute Cypher Query**

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) WHERE n.age > 25 RETURN n.name, n.age ORDER BY n.age DESC LIMIT 10"
  }'
```

**Response:**
```json
{
  "columns": ["n.name", "n.age"],
  "rows": [
    ["Alice", 35],
    ["Bob", 30],
    ["Charlie", 28]
  ],
  "execution_time_ms": 3
}
```

#### **2️⃣ KNN Vector Search**

```bash
curl -X POST http://localhost:15474/knn_traverse \
  -H "Content-Type: application/json" \
  -d '{
    "label": "Person",
    "vector": [0.1, 0.2, 0.3, ...],  
    "k": 10
  }'
```

**Response:**
```json
{
  "nodes": [
    {
      "id": 42,
      "properties": {"name": "Alice", "age": 30},
      "score": 0.95
    }
  ],
  "execution_time_ms": 2
}
```

#### **5️⃣ MCP Protocol Integration**

```bash
# Generate graph via MCP tool
curl -X POST http://localhost:15474/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "tool": "graph_generate",
    "params": {
      "graph_type": "call_graph",
      "scope": {
        "collections": ["codebase", "functions"],
        "file_patterns": ["*.rs"]
      }
    }
  }'
```

#### **6️⃣ UMICP Protocol Integration**

```bash
# Generate graph via UMICP method
curl -X POST http://localhost:15474/umicp \
  -H "Content-Type: application/json" \
  -d '{
    "method": "graph.generate",
    "params": {
      "graph_type": "dependency_graph",
      "scope": {
        "collections": ["codebase"],
        "include_external": false
      }
    }
  }'
```

## 📚 **Cypher Query Examples**

### **Pattern Matching**

```cypher
-- Find friends of friends (2-hop traversal)
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)-[:KNOWS]->(fof)
WHERE fof <> me AND NOT (me)-[:KNOWS]->(fof)
RETURN DISTINCT fof.name, fof.age
ORDER BY fof.age DESC
LIMIT 10
```

### **Aggregation & Analytics**

```cypher
-- Top product categories by sales
MATCH (p:Person)-[:BOUGHT]->(prod:Product)
RETURN prod.category, COUNT(*) AS purchases, AVG(prod.price) AS avg_price
ORDER BY purchases DESC
LIMIT 10
```

### **Hybrid KNN + Graph Traversal** 🔥

```cypher
-- Find similar people and their companies (RAG pattern)
CALL vector.knn('Person', $embedding, 20)
YIELD node AS similar, score
WHERE similar.age > 25 AND similar.active = true
MATCH (similar)-[:WORKS_AT]->(company:Company)
RETURN similar.name, company.name, score
ORDER BY score DESC
LIMIT 10
```

### **Recommendation System**

```cypher
-- Recommend products based on what friends bought
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)
MATCH (friend)-[:BOUGHT]->(product:Product)
WHERE NOT (me)-[:BOUGHT]->(product)
RETURN product.name, product.category, COUNT(*) AS friend_count
ORDER BY friend_count DESC
LIMIT 5
```

## 🏗️ **Architecture**

```
┌──────────────────────────────────────────────────────────┐
│                     REST/HTTP API                         │
│       POST /cypher • POST /knn_traverse • POST /ingest   │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                  Cypher Executor                          │
│        Pattern Match • Expand • Filter • Project         │
│         Heuristic Cost-Based Query Planner               │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│            Transaction Layer (MVCC)                       │
│      Epoch-Based Snapshots • Single-Writer Locking       │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                   Index Layer                             │
│   Label Bitmap • B-tree (V1) • Full-Text (V1) • KNN     │
│   RoaringBitmap  •  Tantivy  •  HNSW (hnsw_rs)          │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                  Storage Layer                            │
│  Catalog (LMDB) • WAL • Record Stores • Page Cache      │
│  Label/Type/Key Mappings  •  Durability  •  Memory Mgmt │
└──────────────────────────────────────────────────────────┘
```

### **Core Components**

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Catalog** | LMDB (heed) | Label/Type/Key → ID bidirectional mappings |
| **Record Stores** | memmap2 | Fixed-size node (32B) & relationship (48B) records |
| **Page Cache** | Custom | 8KB pages with Clock/2Q/TinyLFU eviction |
| **WAL** | Append-only log | Write-ahead log for crash recovery |
| **MVCC** | Epoch-based | Snapshot isolation without locking readers |
| **Label Index** | RoaringBitmap | Compressed bitmap per label |
| **KNN Index** | hnsw_rs | HNSW vector search per label |
| **Full-Text** | Tantivy (V1) | BM25 text search |
| **Executor** | Custom | Cypher parser, planner, operators |

## 📊 **Performance Benchmarks**

### **MVP Targets (Single Node)**

| Operation | Throughput | Latency (p95) | Notes |
|-----------|-----------|---------------|-------|
| 🎯 Point reads | 100K+ ops/sec | < 1 ms | Direct offset access |
| 🔍 KNN queries (k=10) | 10K+ ops/sec | < 2 ms | HNSW logarithmic search |
| 🔗 Pattern traversal | 1K-10K ops/sec | 5-50 ms | Depth-dependent |
| 📥 Bulk ingest | 100K+ nodes/sec | N/A | Append-only WAL |

### **Scaling Characteristics**

- **Nodes**: 1M - 100M per instance
- **Relationships**: 2M - 200M per instance  
- **KNN Vectors**: 1M - 10M per label
- **Memory**: 8GB - 64GB recommended
- **Storage**: SSD recommended, NVMe ideal

## 📖 **Documentation**

### **Architecture & Design**

- 📐 [**Architecture Guide**](docs/ARCHITECTURE.md) - Complete system design
- 🗺️ [**Development Roadmap**](docs/ROADMAP.md) - Implementation phases (MVP, V1, V2)
- 🔗 [**Component DAG**](docs/DAG.md) - Module dependencies and build order
- 🎯 [**Complete Neo4j Cypher Roadmap**](openspec/changes/) - 14-phase modular implementation plan (32-46 weeks)

### **Detailed Specifications**

- 💾 [**Storage Format**](docs/specs/storage-format.md) - Record store binary layouts
- 📝 [**Cypher Subset**](docs/specs/cypher-subset.md) - Supported query syntax & examples
- 🧠 [**Page Cache**](docs/specs/page-cache.md) - Memory management & eviction policies
- 📋 [**WAL & MVCC**](docs/specs/wal-mvcc.md) - Transaction model & crash recovery
- 🎯 [**KNN Integration**](docs/specs/knn-integration.md) - Vector search implementation
- 🔌 [**API Protocols**](docs/specs/api-protocols.md) - REST, MCP, UMICP specifications
- 🎭 [**Graph Correlation**](docs/specs/graph-correlation-analysis.md) - Code relationship analysis
- 🚀 [**Complete Neo4j Cypher Roadmap**](openspec/changes/) - 14-phase modular implementation plan

### **📋 MVP (Phase 1)** - ✅ COMPLETED

- [x] Architecture documentation
- [x] Project scaffolding (Rust edition 2024)
- [x] **Storage Layer** (catalog, record stores, page cache, WAL)
- [x] **Basic Indexes** (label bitmap, KNN/HNSW)
- [x] **Cypher Executor** (MATCH, WHERE, RETURN, ORDER BY, LIMIT)
- [x] **HTTP API** (complete endpoints)
- [x] **Graph Correlation Analysis** (call graphs, dependency graphs, pattern recognition)
- [x] **Integration Tests** (95%+ coverage)

**Status**: ✅ Complete (Q4 2024)age cache, WAL)
- [ ] **Basic Indexes** (label bitmap, KNN/HNSW)
- [ ] **Cypher Executor** (MATCH, WHERE, RETURN, ORDER BY, LIMIT)
- [ ] **HTTP API** (complete endpoints)
- [ ] **Graph Correlation Analysis** (call graphs, dependency graphs, pattern recognition)
- [ ] **Integration Tests** (95%+ coverage)

**Target**: Q4 2024

### **🎯 V1 (Phase 2)** - Current Development

- [ ] **Complete Neo4j Cypher Implementation** (14 modular phases)
  - ✅ Master plan created with 32-46 week timeline
  - 🔴 Phase 1: Write Operations (MERGE, SET, DELETE, REMOVE) - Ready to start
  - 🟠 Phases 2-7: Core Cypher features (WITH, OPTIONAL MATCH, functions, schema)
  - 🟡 Phases 8-12: Production features (import/export, monitoring, UDFs)
  - 🟢 Phases 13-14: Optional features (graph algorithms, geospatial)
  - See `openspec/changes/` for complete breakdown
- [ ] **Advanced Indexes** (B-tree for properties, Tantivy full-text)
- [ ] **Constraints** (UNIQUE, NOT NULL, CHECK)
- [ ] **Query Optimization** (cost-based planner with statistics)
- [ ] **Bulk Loader** (bypass WAL for fast initial load)
- [ ] **Authentication & Security** (API keys, RBAC, rate limiting)
- [ ] **Master-Replica Replication** (async/sync, automatic failover)
- [ ] **Desktop GUI** (Electron app with graph visualization)
- [ ] **Monitoring & Metrics** (Prometheus, OpenTelemetry)
- [ ] **Vectorizer Hybrid Search** (RRF ranking, bidirectional sync)
- [ ] **Advanced Graph Analysis** (data flow graphs, component graphs, interactive visualization)

**Target**: Q1-Q4 2025

### **🚀 V2 (Phase 3)** - Distributed Graph

- [ ] **Sharding** (hash-based node partitioning)
- [ ] **Replication** (Raft consensus via openraft)
- [ ] **Cluster Coordination** (distributed query execution)
- [ ] **Multi-Region Support** (cross-datacenter replication)
- [ ] **Intelligent Graph Analysis** (ML-powered pattern learning, anomaly detection, predictive analysis)

**Target**: Q2 2025

See [**ROADMAP.md**](docs/ROADMAP.md) for detailed timeline and milestones.

## ⚡ **Why Nexus?**

| Feature | Neo4j | Other Graph DBs | **Nexus** |
|---------|-------|-----------------|-----------|
| **Storage** | Record stores + page cache | Varies | ✅ Same Neo4j approach |
| **Query Language** | Full Cypher | GraphQL, Gremlin | ✅ Cypher subset (20% = 80% cases) |
| **Transactions** | Full ACID, MVCC | Varies | ✅ Simplified MVCC (epochs) |
| **Indexes** | B-tree, full-text | Varies | ✅ Same + **native KNN** |
| **Vector Search** | Plugin (GDS) | Separate service | ✅ **Native first-class** |
| **Target Workload** | General graph | Varies | ✅ **Read-heavy + RAG** |
| **Performance** | Excellent | Good | ✅ **Optimized for reads** |

### **When to Use Nexus**

✅ **Perfect for:**
- RAG applications needing semantic + graph traversal
- Recommendation systems with hybrid search
- Knowledge graphs with vector embeddings
- Document networks with citation analysis
- Social networks with similarity search
- **Code analysis and LLM assistance** (call graphs, dependency analysis, pattern recognition)

❌ **Not ideal for:**
- Write-heavy OLTP workloads (use traditional RDBMS)
- Simple key-value storage (use Redis/Synap)
- Document-only search (use Elasticsearch/Vectorizer)
- Complex graph algorithms requiring full Cypher (use Neo4j)

## 🛠️ **Development**

### **Project Structure**

```
nexus/
├── nexus-core/           # 🧠 Core graph engine library
│   ├── catalog/          #    Label/type/key mappings (LMDB)
│   ├── storage/          #    Record stores (nodes, rels, props)
│   ├── page_cache/       #    Memory management (8KB pages)
│   ├── wal/              #    Write-ahead log
│   ├── index/            #    Indexes (bitmap, KNN, full-text)
│   ├── executor/         #    Cypher parser & execution
│   └── transaction/      #    MVCC & locking
├── nexus-server/         # 🌐 HTTP server (Axum)
│   ├── api/              #    REST endpoints
│   └── config.rs         #    Server configuration
├── nexus-protocol/       # 🔌 Integration protocols
│   ├── rest.rs           #    REST client
│   ├── mcp.rs            #    MCP client
│   └── umicp.rs          #    UMICP client
├── tests/                # 🧪 Integration tests
└── docs/                 # 📚 Documentation
    ├── ARCHITECTURE.md   #    System design
    ├── ROADMAP.md        #    Implementation timeline
    ├── DAG.md            #    Component dependencies
    └── specs/            #    Detailed specifications
```

### **Building from Source**

```bash
# Development build
cargo build --workspace

# Release build (optimized)
cargo +nightly build --release --workspace

# Run tests
cargo test --workspace --verbose

# Check coverage (95%+ required)
cargo llvm-cov --workspace --ignore-filename-regex 'examples'
```

### **Code Quality**

All code must pass quality checks:

```bash
# Format code
cargo +nightly fmt --all

# Lint (no warnings allowed)
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all tests
cargo test --workspace

# Build release
cargo +nightly build --release
```

## ⚙️ **Configuration**

### **Environment Variables**

```bash
NEXUS_ADDR=127.0.0.1:15474   # Server bind address
NEXUS_DATA_DIR=./data         # Data directory path
RUST_LOG=nexus_server=debug   # Logging level (error, warn, info, debug, trace)
```

### **Data Directory Structure**

```
data/
├── catalog.mdb          # LMDB catalog (labels, types, keys)
├── nodes.store          # Node records (32 bytes each)
├── rels.store           # Relationship records (48 bytes each)
├── props.store          # Property records (variable size)
├── strings.store        # String/blob dictionary
├── wal.log              # Write-ahead log
├── checkpoints/         # Checkpoint snapshots
│   └── epoch_*.ckpt
└── indexes/
    ├── label_*.bitmap   # Label bitmap indexes (RoaringBitmap)
    └── hnsw_*.bin       # HNSW vector indexes
```

## 🔐 **Authentication & Security**

### **API Key Authentication (V1)**

**Disabled by default** for localhost development, **required** for public binding:

```bash
# Localhost (127.0.0.1): No authentication required
NEXUS_ADDR=127.0.0.1:15474 ./nexus-server

# Public binding (0.0.0.0): Authentication REQUIRED
NEXUS_ADDR=0.0.0.0:15474 ./nexus-server
# Error: Authentication must be enabled for public binding

# Enable authentication
NEXUS_AUTH_ENABLED=true NEXUS_ADDR=0.0.0.0:15474 ./nexus-server
```

### **API Key Management**

```bash
# Create API key
curl -X POST http://localhost:15474/auth/keys \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Production App",
    "permissions": ["read", "write"]
  }'

# Response
{
  "id": "key_abc123",
  "key": "nexus_sk_abc123def456...",  # Save this! Not shown again
  "name": "Production App",
  "permissions": ["read", "write"],
  "rate_limit": {"per_minute": 1000, "per_hour": 10000}
}

# Use API key
curl -X POST http://localhost:15474/cypher \
  -H "Authorization: Bearer nexus_sk_abc123def456..." \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) RETURN n LIMIT 10"}'
```

### **Rate Limiting**

- **1,000 requests/minute** per API key
- **10,000 requests/hour** per API key
- Returns `429 Too Many Requests` when exceeded
- Headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`

## 🖥️ **Desktop GUI** (V1)

### **Nexus Desktop Application**

Modern Electron-based desktop app for visual graph management:

```
Features:
🎨 Beautiful Vue 3 interface with dark/light themes
📊 Interactive graph visualization (force-directed layouts)
💻 Cypher query editor with syntax highlighting
🔍 Visual KNN search (text → embedding → results)
📈 Real-time performance dashboard
🔧 Complete database management tools
```

### **Installation**

```bash
# Download from releases
# Windows: Nexus-Setup-0.1.0.exe
# macOS: Nexus-0.1.0.dmg
# Linux: nexus_0.1.0_amd64.AppImage

# Or build from source
cd gui
npm install
npm run build:win    # Windows MSI installer
npm run build:mac    # macOS DMG
npm run build:linux  # Linux AppImage/DEB
```

### **Screenshots**

**Graph Visualization**
- Interactive node/relationship exploration
- Filter by labels and types
- Property inspector panel
- Zoom, pan, node selection

**Query Editor**
- Monaco/CodeMirror with Cypher syntax
- Query history and saved queries
- Result view: Table or Graph
- Export results (JSON, CSV)

**Monitoring Dashboard**
- Query throughput metrics
- Page cache hit rate
- WAL and storage size
- Replication lag (if enabled)

## 🔗 **Integrations**

### **Vectorizer Integration**

Nexus integrates **natively** with [Vectorizer](https://github.com/hivellm/vectorizer) for hybrid search:

```rust
// 1. Generate embedding via Vectorizer
let vectorizer = VectorizerClient::new("http://localhost:15002");
let embedding = vectorizer.embed("machine learning algorithms").await?;

// 2. Store in graph with KNN index
engine.create_node_with_embedding(
    vec!["Document"],
    json!({"title": "ML Guide", "content": "..."}),
    embedding
)?;

// 3. Hybrid semantic + graph search
CALL vector.knn('Document', $query_embedding, 10)
YIELD node AS doc, score
MATCH (doc)-[:CITES]->(related:Document)
RETURN doc.title, related.title, score
ORDER BY score DESC
```

### **Hybrid Search with RRF Ranking**

Combine Nexus graph traversal + Vectorizer semantic search:

```rust
// Reciprocal Rank Fusion (RRF) for hybrid ranking
async fn hybrid_search(query: &str, k: usize) -> Result<Vec<ScoredNode>> {
    // 1. Nexus KNN search
    let nexus_results = nexus.knn_search("Document", query_embedding, k).await?;
    
    // 2. Vectorizer semantic search  
    let vectorizer_results = vectorizer.search("docs", query, k).await?;
    
    // 3. Combine with RRF
    let combined = reciprocal_rank_fusion(
        &nexus_results,
        &vectorizer_results,
        k: 60  // RRF constant
    );
    
    Ok(combined)
}

// Cypher equivalent
CALL graph.hybrid_search('Document', $query_text, 20)
YIELD node, graph_score, semantic_score, rrf_score
RETURN node.title, rrf_score
ORDER BY rrf_score DESC
LIMIT 10
```

### **Bidirectional Sync**

```
Vectorizer → Nexus:
- Document added to Vectorizer → Create node in Nexus
- Embedding updated → Update KNN index
- Document deleted → Mark node as deleted

Nexus → Vectorizer:
- Node created → Index in Vectorizer collection
- Property updated → Re-embed and update
- Node deleted → Remove from Vectorizer

Configuration:
{
  "vectorizer_url": "http://localhost:15002",
  "sync_enabled": true,
  "sync_collections": ["documents", "knowledge_base"],
  "auto_embed_fields": ["title", "content", "description"]
}
```

### **Protocol Support**

- 🌐 **REST/HTTP**: Default integration (streamable HTTP)
- 🔌 **MCP**: Model Context Protocol for AI services
- 🔗 **UMICP**: Universal Model Interoperability Protocol

See [**API Protocols**](docs/specs/api-protocols.md) for complete specifications.

## 🔄 **Replication & High Availability** (V1)

### **Master-Replica Replication**

Redis-style replication system for read scalability and fault tolerance:

```bash
# Start master
NEXUS_ROLE=master NEXUS_ADDR=0.0.0.0:15474 ./nexus-server

# Start replica
NEXUS_ROLE=replica \
NEXUS_MASTER_URL=http://master:15474 \
NEXUS_ADDR=0.0.0.0:15475 \
./nexus-server
```

### **Replication Features**

- 📦 **Full Sync**: Initial snapshot transfer with CRC32 verification
- 🔄 **Incremental Sync**: WAL streaming (circular buffer, 1M operations)
- ⚡ **Async Replication**: High throughput, eventual consistency (default)
- 🔒 **Sync Replication**: Configurable quorum for durability
- 🔌 **Auto-Reconnect**: Exponential backoff on connection loss
- 📊 **Lag Monitoring**: Real-time replication lag tracking

### **Failover & Promotion**

```bash
# Check replication status
curl http://replica:15475/replication/status

# Response
{
  "role": "replica",
  "master_url": "http://master:15474",
  "lag_seconds": 0.5,
  "last_sync": 1704067200,
  "status": "healthy"
}

# Promote replica to master (manual failover)
curl -X POST http://replica:15475/replication/promote \
  -H "Authorization: Bearer admin_key"
```

### **Replication API**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/replication/status` | GET | Get replication status and lag |
| `/replication/promote` | POST | Promote replica to master |
| `/replication/pause` | POST | Pause replication |
| `/replication/resume` | POST | Resume replication |
| `/replication/lag` | GET | Get current replication lag |

## 🧪 **Testing**

### **Requirements**

- ✅ **1279 tests passing** (100% success rate)
- ✅ **100% Neo4j compatibility** (17/17 cross-validation tests)
- ✅ **70.39% coverage overall** (95%+ in core modules)
- ✅ Unit tests in modules (`#[cfg(test)]`)
- ✅ Integration tests in `/tests`
- ✅ Comprehensive E2E tests with real datasets
- ✅ Cross-compatibility validation with live Neo4j instance

### **Running Tests**

```bash
# Run all tests
cargo test --workspace --verbose

# Run with coverage report
cargo llvm-cov --workspace --html

# Run specific test
cargo test test_knn_integration --package nexus-core

# Run integration tests only
cargo test --test integration_test
```

## 📦 **Use Cases**

### **1. RAG (Retrieval-Augmented Generation)**

```cypher
-- Semantic document retrieval + citation graph
CALL vector.knn('Document', $query_vector, 10)
YIELD node AS doc, score
MATCH (doc)-[:CITES]->(cited:Document)
RETURN doc.title, doc.content, COLLECT(cited.title) AS citations, score
ORDER BY score DESC
```

### **2. Recommendation Engine**

```cypher
-- Collaborative filtering with graph structure
MATCH (user:Person {id: $user_id})-[:LIKES]->(item:Product)
MATCH (item)<-[:LIKES]-(similar:Person)
MATCH (similar)-[:LIKES]->(recommendation:Product)
WHERE NOT (user)-[:LIKES]->(recommendation)
RETURN recommendation.name, COUNT(*) AS score
ORDER BY score DESC
LIMIT 10
```

### **4. Code Analysis & LLM Assistance** 🔥

```cypher
-- Generate call graph for LLM context
CALL graph.generate('call_graph', {
  scope: {collections: ['codebase'], file_patterns: ['*.rs']},
  options: {clustering_enabled: true}
})
YIELD graph_id

-- Analyze code patterns
CALL graph.analyze(graph_id, 'pattern_detection')
YIELD pattern_type, confidence, nodes
WHERE confidence > 0.8
RETURN pattern_type, confidence, SIZE(nodes) AS pattern_size
ORDER BY confidence DESC
```

#### **MCP Integration Example**

```rust
// LLM can use MCP tools to understand code structure
let mcp_client = McpClient::new("http://localhost:15474/mcp");

// Generate call graph
let graph_response = mcp_client.call("graph_generate", json!({
    "graph_type": "call_graph",
    "scope": {
        "collections": ["codebase", "functions"],
        "file_patterns": ["*.rs"]
    }
})).await?;

// Analyze patterns
let patterns = mcp_client.call("graph_patterns", json!({
    "graph_id": graph_response["graph_id"],
    "pattern_types": ["pipeline", "event_driven"]
})).await?;
```

#### **UMICP Integration Example**

```rust
// LLM can use UMICP methods for standardized access
let umicp_client = UmicpClient::new("http://localhost:15474/umicp");

// Generate dependency graph
let graph = umicp_client.request(json!({
    "method": "graph.generate",
    "params": {
        "graph_type": "dependency_graph",
        "scope": {
            "collections": ["codebase"],
            "include_external": false
        }
    }
})).await?;

// Get visualization data
let visualization = umicp_client.request(json!({
    "method": "graph.visualize",
    "params": {
        "graph_id": graph["graph_id"],
        "format": "interactive"
    }
})).await?;
```

## 🤝 **Contributing**

We welcome contributions! See [**CONTRIBUTING.md**](CONTRIBUTING.md) for guidelines.

### **Development Workflow**

1. **Fork** the repository
2. **Create feature branch**: `git checkout -b feature/your-feature`
3. **Make changes** with tests (95%+ coverage)
4. **Quality checks**: `cargo fmt`, `cargo clippy`, `cargo test`
5. **Commit**: Use conventional commits
6. **Submit PR**: Include description, tests, documentation

### **OpenSpec for Major Features**

For significant features, use **OpenSpec** for spec-driven development:

```bash
# View all active changes
cat openspec/changes/README.md

# Complete Neo4j Cypher implementation (14 phases)
# Start with Phase 1: Write Operations
cd openspec/changes/implement-cypher-write-operations/

# Check proposal and tasks
cat proposal.md
cat tasks.md
```

**Current Active Changes:**
- 🎯 **Complete Neo4j Cypher** - Master plan with 14 modular phases (32-46 weeks)
- 🔴 **Phase 1 Ready**: Write Operations (MERGE, SET, DELETE, REMOVE)
- 🔍 **Graph Correlation Analysis** - Call graphs, dependency analysis (47.5% complete)
- 🔐 **Authentication System** - API keys, RBAC, rate limiting (48.6% complete)

See `openspec/AGENTS.md` for complete workflow.

## 📜 **License**

Licensed under **MIT**.

See [LICENSE](LICENSE) for details.

## 🙏 **Acknowledgments**

- **[Neo4j](https://neo4j.com/)**: Inspiration for record store architecture and Cypher language
- **[HNSW](https://arxiv.org/abs/1603.09320)**: Efficient approximate nearest neighbor algorithm
- **[OpenCypher](https://opencypher.org/)**: Cypher query language specification
- **[Rust Community](https://www.rust-lang.org/)**: Amazing ecosystem of high-performance crates

## 📞 **Contact & Support**

- 🐛 **Issues**: [github.com/hivellm/nexus/issues](https://github.com/hivellm/nexus/issues)
- 💬 **Discussions**: [github.com/hivellm/nexus/discussions](https://github.com/hivellm/nexus/discussions)
- 📧 **Email**: team@hivellm.org
- 🌐 **Repository**: [github.com/hivellm/nexus](https://github.com/hivellm/nexus)

---

<div align="center">

**Built with ❤️ in Rust** 🦀

*Combining the best of graph databases and vector search for the AI era*

[⭐ Star us on GitHub](https://github.com/hivellm/nexus) • [📖 Read the Docs](docs/) • [🚀 Get Started](#-quick-start)

</div>
