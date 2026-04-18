# Nexus Graph Database

**⚡ High-performance property graph database with native vector search**

![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Edition](https://img.shields.io/badge/edition-2024-blue.svg)
![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)
![Status](https://img.shields.io/badge/status-v0.12.0%20%7C%2097.99%25%20Compatibility-success.svg)
![Tests](https://img.shields.io/badge/tests-2949%2B%20passing-success.svg)
![Coverage](https://img.shields.io/badge/coverage-70.39%25-yellow.svg)

[Features](#-key-features) • [Quick Start](#-quick-start) • [Documentation](#-documentation) • [Roadmap](#-roadmap) • [Contributing](#-contributing)

---

## 🎯 **What is Nexus?**

Nexus is a modern **property graph database** built for **read-heavy workloads** with **first-class vector search**. Inspired by Neo4j's battle-tested architecture, it combines the power of graph traversal with semantic similarity search for hybrid **RAG (Retrieval-Augmented Generation)** applications.

Think of it as **Neo4j meets Vector Search** - optimized for AI applications that need both structured relationships and semantic similarity search.

### 🎉 **Current Status (v0.12.0)**

**Production Ready!** 🚀

- ✅ **~55% openCypher Compatibility** - Core clauses (MATCH, CREATE, MERGE, SET, DELETE, REMOVE, WHERE, RETURN, ORDER BY, LIMIT, SKIP, UNION, WITH, UNWIND, FOREACH) and ~60 functions
- ✅ **97.99% Compatibility Tests** - 293/300 Neo4j compatibility tests passing + 2949+ unit tests (see [CHANGELOG.md](CHANGELOG.md) for details)
- ✅ **19 GDS Procedures** - PageRank (standard, weighted, parallel), betweenness, eigenvector, Dijkstra, A*, Yen's k-paths, Louvain, label propagation, triangle count, clustering coefficients
- ✅ **Complete Authentication** - API keys, JWT, RBAC, rate limiting
- ✅ **Multiple Databases** - Isolated databases with full CRUD API
- ✅ **Official SDKs** - Rust, Python, TypeScript, Go, C#, n8n (30+ tests each)
- ✅ **2949+ Tests Passing** - 100% success rate, 70%+ coverage
- ⚠️ **Known Limitations**: Constraints (UNIQUE, EXISTS), Advanced indexes (FULL-TEXT, POINT)

See [Neo4j Compatibility Report](docs/NEO4J_COMPATIBILITY_REPORT.md) for complete details.

## 🌟 **Key Features**

### **Graph Database**
- 🏗️ **Property Graph Model**: Nodes with labels, relationships with types, both with properties
- 🔍 **Cypher Subset**: Familiar query language covering 80% of common use cases
- ⚡ **Neo4j-Inspired Storage**: Fixed-size record stores (32B nodes, 48B relationships)
- 🔗 **O(1) Traversal**: Doubly-linked adjacency lists without index lookups
- 💾 **ACID Transactions**: WAL + MVCC for durability and consistency

### **Vector Search (Native KNN)**
- 🎯 **HNSW Indexes**: Hierarchical Navigable Small World for fast approximate search
- 📊 **Per-Label Indexes**: Separate vector space for each node label
- 🔄 **Hybrid Queries**: Combine vector similarity with graph traversal in single query
- ⚡ **High Performance**: 10,000+ KNN queries/sec (k=10)
- 📐 **Multiple Metrics**: Cosine similarity, Euclidean distance

### **Graph Analysis & Visualization**
- 🎨 **Layout Algorithms**: Force-directed, hierarchical, circular, and grid layouts
- 🔍 **Clustering**: K-means, hierarchical, DBSCAN, and community detection
- 📊 **Graph Operations**: Connected components, pattern recognition, interactive visualization
- 🎯 **Code Analysis**: Call graphs, dependency graphs, data flow graphs for LLM assistance

### **Performance Optimizations** 🔥
- ⚡ **Vectorized Query Execution** - SIMD-accelerated (40%+ faster WHERE filtering, ≤3.0ms)
- 🎯 **JIT Query Compilation** - Real-time Cypher-to-native code (50%+ improvement)
- 🔗 **Advanced Join Algorithms** - Hash/merge joins with bloom filters (60%+ improvement, ≤4.0ms)
- 🏗️ **Custom Storage Engine** - Relationship-centric layout (31,075x improvement)
- 🗄️ **Hierarchical Cache (L1/L2/L3)** - 90%+ hit rates with intelligent warming
- 🗜️ **Compression Suite** - LZ4, Zstd, SIMD RLE (30-80% space reduction)
- ⚙️ **Concurrent Execution** - Thread pool, lock-free structures, NUMA-aware
- 📊 **Query Result Caching** - Adaptive TTL with dependency-based invalidation

### **Integration & Protocols**
- 🌐 **StreamableHTTP**: Default protocol with SSE streaming
- 🔌 **MCP Protocol**: 19+ focused tools for AI integrations
- 🔗 **UMICP v0.2.1**: Tool discovery endpoint + native JSON
- 🤝 **Vectorizer Integration**: Native hybrid search with RRF ranking

### **Production Features**
- 🔐 **API Key Auth**: Disabled by default, required for 0.0.0.0 binding
- 🔄 **Master-Replica Replication**: Redis-style async/sync replication (V1)
- ⚡ **Automatic Failover**: Health monitoring with replica promotion
- 📊 **Rate Limiting**: 1000/min, 10000/hour per API key

## 🚀 **Quick Start**

### **Prerequisites**
- Rust nightly 1.85+ (edition 2024)
- 8GB+ RAM (recommended)
- Linux/macOS/Windows (native or WSL)

### **Installation**

#### **Option 1: Automated Installation (Recommended)**

**Linux/macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install/install.sh | bash
```

**Windows:**
```powershell
powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install/install.ps1 | iex"
```

The installation script downloads the latest release, installs to system PATH, and creates a system service.

**Service Management:**
- **Linux**: `sudo systemctl status nexus`, `sudo systemctl restart nexus`
- **Windows**: `Get-Service -Name Nexus`, `Restart-Service -Name Nexus`

See [scripts/install/INSTALL.md](scripts/install/INSTALL.md) for detailed instructions.

#### **Option 2: Build from Source**

```bash
git clone https://github.com/hivellm/nexus
cd nexus
cargo +nightly build --release --workspace
./target/release/nexus-server
```

Server starts on **`http://localhost:15474`** by default.

### **Access Points**
- 🌐 **REST API**: `http://localhost:15474` (StreamableHTTP with SSE)
- 🔌 **MCP Server**: `http://localhost:15474/mcp`
- 🔗 **UMICP**: `http://localhost:15474/umicp`
- 🔍 **Tool Discovery**: `http://localhost:15474/umicp/discover`
- ❤️ **Health Check**: `http://localhost:15474/health`
- 📊 **Statistics**: `http://localhost:15474/stats`

### **Basic Usage**

#### **Execute Cypher Query**

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) WHERE n.age > 25 RETURN n.name, n.age ORDER BY n.age DESC LIMIT 10"}'
```

**Response:**
```json
{
  "columns": ["n.name", "n.age"],
  "rows": [["Alice", 35], ["Bob", 30], ["Charlie", 28]],
  "execution_time_ms": 3
}
```

#### **KNN Vector Search**

```bash
curl -X POST http://localhost:15474/knn_traverse \
  -H "Content-Type: application/json" \
  -d '{"label": "Person", "vector": [0.1, 0.2, 0.3, ...], "k": 10}'
```

#### **MCP Protocol Integration**

```bash
curl -X POST http://localhost:15474/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "tool": "graph_generate",
    "params": {
      "graph_type": "call_graph",
      "scope": {"collections": ["codebase", "functions"], "file_patterns": ["*.rs"]}
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

### **Variable-Length Paths**

```cypher
-- Find all nodes within 3 hops
MATCH (start:Person {name: 'Alice'})-[*1..3]->(end)
RETURN end.name, end.age

-- Find shortest path
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
RETURN shortestPath([(a)-[*]->(b)]) AS path
```

### **Hybrid KNN + Graph Traversal**

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

## 🗄️ **Multi-Database Support**

Nexus supports multiple isolated databases within a single server instance, enabling multi-tenancy and logical data separation.

### **Cypher Commands**

```cypher
-- List all databases
SHOW DATABASES

-- Create a new database
CREATE DATABASE mydb

-- Switch to a database
:USE mydb

-- Get current database
RETURN database() AS current_db

-- Drop a database (permanent!)
DROP DATABASE mydb
```

### **REST API**

```bash
# List databases
curl http://localhost:15474/databases

# Create database
curl -X POST http://localhost:15474/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "mydb"}'

# Switch session database
curl -X PUT http://localhost:15474/session/database \
  -H "Content-Type: application/json" \
  -d '{"name": "mydb"}'

# Drop database
curl -X DELETE http://localhost:15474/databases/mydb
```

### **SDK Support**

All official SDKs (Rust, Python, TypeScript, Go, C#) include database management methods:
- `list_databases()` / `listDatabases()`
- `create_database(name)` / `createDatabase(name)`
- `switch_database(name)` / `switchDatabase(name)`
- `drop_database(name)` / `dropDatabase(name)`

See [User Guide - Multi-Database Support](docs/USER_GUIDE.md#multi-database-support) for complete documentation.

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

| Component         | Technology      | Purpose                                            |
| ----------------- | --------------- | -------------------------------------------------- |
| **Catalog**       | LMDB (heed)     | Label/Type/Key → ID bidirectional mappings         |
| **Record Stores** | memmap2         | Fixed-size node (32B) & relationship (48B) records |
| **Page Cache**    | Custom          | 8KB pages with Clock/2Q/TinyLFU eviction           |
| **WAL**           | Append-only log | Write-ahead log for crash recovery                 |
| **MVCC**          | Epoch-based     | Snapshot isolation without locking readers         |
| **Label Index**   | RoaringBitmap   | Compressed bitmap per label                        |
| **KNN Index**     | hnsw_rs         | HNSW vector search per label                       |
| **Full-Text**     | Tantivy (V1)    | BM25 text search                                   |
| **Executor**      | Custom          | Cypher parser, planner, operators                  |

## 📊 **Performance Benchmarks**

### **Throughput & Latency**

| Operation             | Throughput      | Latency (p95) | Notes                   |
| --------------------- | --------------- | ------------- | ----------------------- |
| 🎯 Point reads        | 100K+ ops/sec   | < 1 ms        | Direct offset access    |
| 🔍 KNN queries (k=10) | 10K+ ops/sec    | < 2 ms        | HNSW logarithmic search |
| 🔗 Pattern traversal  | 1K-10K ops/sec  | 5-50 ms       | Depth-dependent         |
| 📥 Bulk ingest        | 100K+ nodes/sec | N/A           | Append-only WAL         |

### **Optimization Results**

| Optimization          | Improvement     | Target        | Status                  |
| --------------------- | --------------- | ------------- | ----------------------- |
| ⚡ WHERE filtering    | 40%+ faster     | ≤3.0ms        | ✅ SIMD-accelerated     |
| 🎯 Complex queries    | 50%+ faster     | ≤4.0ms        | ✅ JIT compilation      |
| 🔗 JOIN queries       | 60%+ faster     | ≤4.0ms        | ✅ Advanced algorithms  |
| 🏗️ Storage operations | 31,075x faster  | <5ms          | ✅ Custom storage engine|
| 🗄️ Cache hit rate     | 90%+            | <3ms cached   | ✅ Hierarchical cache   |
| 🔄 Relationship traversal | 49% faster | ≤2.0ms    | ✅ Bloom filters        |
| 📊 Pattern matching   | 43% faster      | ≤4.0ms        | ✅ Parallel processing  |
| 💾 Memory usage       | 60% reduction   | Optimized     | ✅ Compression suite   |

### **Scaling Characteristics**
- **Nodes**: 1M - 100M per instance
- **Relationships**: 2M - 200M per instance
- **KNN Vectors**: 1M - 10M per label
- **Memory**: 8GB - 64GB recommended
- **Storage**: SSD recommended, NVMe ideal

### **Performance vs Neo4j** 🏆
- **Throughput**: 8.6% higher (655 vs 603 queries/sec)
- **Write Operations**: 56-85% faster (CREATE nodes 83.5% faster, CREATE relationships 56.7% faster)
- **Query Execution**: **Nexus wins 22/22 benchmarks** with advanced optimizations
- **MATCH+CREATE**: 95% improvement (34ms → 1.75ms) - **56.7% faster than Neo4j**

See [Performance Analysis](docs/PERFORMANCE.md) for comprehensive benchmarks.

## 📖 **Documentation**

### **📚 User Documentation** (New!)
- 🚀 [**Complete User Guide**](docs/users/README.md) - Comprehensive user documentation organized by topic
  - [Getting Started](docs/users/getting-started/) - Installation, Docker, Quick Start
  - [Cypher Query Language](docs/users/cypher/) - Complete Cypher reference
  - [API Reference](docs/users/api/) - REST API, Authentication, Integration
  - [Vector Search](docs/users/vector-search/) - Native vector similarity search
  - [SDKs](docs/users/sdks/) - Python, TypeScript, Rust, Go, C# SDKs
  - [Configuration](docs/users/configuration/) - Server, Logging, Performance, Cluster
  - [Operations](docs/users/operations/) - Service Management, Monitoring, Backup
  - [Use Cases](docs/users/use-cases/) - Knowledge Graphs, RAG, Recommendations
  - [Advanced Guides](docs/users/guides/) - Graph Algorithms, Multi-Database, Replication
  - [Reference](docs/users/reference/) - Functions, Data Types, Errors

### **Architecture & Design**
- 📐 [**Architecture Guide**](docs/ARCHITECTURE.md) - Complete system design
- 🗺️ [**Development Roadmap**](docs/ROADMAP.md) - Implementation phases (MVP, V1, V2)
- 🔗 [**Component DAG**](docs/DAG.md) - Module dependencies and build order

### **Compatibility & Testing**
- ✅ [**Neo4j Compatibility Report**](docs/NEO4J_COMPATIBILITY_REPORT.md) - Comprehensive compatibility analysis (287/300 tests passing - 95.99%)
- 📊 [**User Guide**](docs/USER_GUIDE.md) - Legacy usage guide (see [new docs](docs/users/) for updated version)
- 🔐 [**Authentication Guide**](docs/AUTHENTICATION.md) - Security and authentication setup

### **Detailed Specifications**
- 💾 [**Storage Format**](docs/specs/storage-format.md) - Record store binary layouts
- 📝 [**Cypher Subset**](docs/specs/cypher-subset.md) - Supported query syntax & examples
- 🧠 [**Page Cache**](docs/specs/page-cache.md) - Memory management & eviction policies
- 📋 [**WAL & MVCC**](docs/specs/wal-mvcc.md) - Transaction model & crash recovery
- 🎯 [**KNN Integration**](docs/specs/knn-integration.md) - Vector search implementation
- 🔌 [**API Protocols**](docs/specs/api-protocols.md) - REST, MCP, UMICP specifications
- 🎭 [**Graph Correlation**](docs/specs/graph-correlation-analysis.md) - Code relationship analysis

### **Roadmap**

**📋 MVP (Phase 1)** - ✅ COMPLETED
- Storage Layer, Basic Indexes, Cypher Executor, HTTP API, Graph Correlation Analysis, Integration Tests

**🎯 V1 (Phase 2)** - Core Complete
- ✅ Complete Neo4j Cypher Implementation (100% - All 14 phases)
- ✅ Authentication & Security (100% - API keys, RBAC, rate limiting)
- ✅ Graph Correlation Analysis (70% - Core functionality implemented)
- ✅ Master-Replica Replication - Redis-style async/sync replication
  - WAL streaming, full sync via snapshot, failover support, REST API endpoints
- ✅ Multi-Database Support - Isolated databases within single server
- ✅ Official SDKs - Rust, Python, TypeScript, Go, C#, n8n (30+ tests each, 100% complete)
- 🚧 Desktop GUI (Electron + Vue 3) - In Progress
- 📋 Advanced Indexes (B-tree, Full-text) - Planned
- 📋 Constraints & Schema - Planned
- 📋 Query Optimization - Planned
- 📋 Monitoring & Metrics (Prometheus) - Planned

**🚀 V2 (Phase 3)** - Distributed Graph (Planned)
- 🔮 Sharding Architecture (Week 21-24)
  - Hash partitioning, shard management, data partitioning, rebalancing
- 🔮 Replication with Raft Consensus (Week 25-28)
  - openraft integration per shard, leader election, log replication, read replicas
- 🔮 Distributed Queries (Week 29-32)
  - Query coordinator, shard-aware planning, scatter/gather execution
- 🔮 Cluster Operations (Week 33-36)
  - Node discovery, health checking, rolling upgrades, disaster recovery

See [**ROADMAP.md**](docs/ROADMAP.md) for detailed timeline and milestones.

## ⚡ **Why Nexus?**

| Feature             | Neo4j                      | Other Graph DBs  | **Nexus**                          |
| ------------------- | -------------------------- | ---------------- | ---------------------------------- |
| **Storage**         | Record stores + page cache | Varies           | ✅ Same Neo4j approach             |
| **Query Language**  | Full Cypher                | GraphQL, Gremlin | ✅ Cypher subset (20% = 80% cases) |
| **Transactions**    | Full ACID, MVCC            | Varies           | ✅ Simplified MVCC (epochs)        |
| **Indexes**         | B-tree, full-text          | Varies           | ✅ Same + **native KNN**           |
| **Vector Search**   | Plugin (GDS)               | Separate service | ✅ **Native first-class**          |
| **Target Workload** | General graph              | Varies           | ✅ **Read-heavy + RAG**            |
| **Performance**     | Excellent                  | Good             | ✅ **Optimized for reads**         |

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
```

### **Building & Testing**

```bash
# Development build
cargo build --workspace

# Release build (optimized)
cargo +nightly build --release --workspace

# Run tests
cargo test --workspace --verbose

# Check coverage (95%+ required)
cargo llvm-cov --workspace --ignore-filename-regex 'examples'

# Code quality checks
cargo +nightly fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
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

### **API Key Authentication**

**Disabled by default** for localhost development, **required** for public binding:

```bash
# Localhost (127.0.0.1): No authentication required
NEXUS_ADDR=127.0.0.1:15474 ./nexus-server

# Public binding (0.0.0.0): Authentication REQUIRED
NEXUS_AUTH_ENABLED=true NEXUS_ADDR=0.0.0.0:15474 ./nexus-server
```

### **API Key Management**

```bash
# Create API key
curl -X POST http://localhost:15474/auth/keys \
  -H "Content-Type: application/json" \
  -d '{"name": "Production App", "permissions": ["read", "write"]}'

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

Combine Nexus graph traversal + Vectorizer semantic search with Reciprocal Rank Fusion (RRF) for optimal ranking.

### **Bidirectional Sync**

- **Vectorizer → Nexus**: Document added/updated/deleted → Create/update/delete node
- **Nexus → Vectorizer**: Node created/updated/deleted → Index/update/remove in Vectorizer

## 📦 **Official SDKs**

Nexus provides official SDKs for 6 programming languages, each with 30+ comprehensive tests:

| SDK | Language | Package | Documentation |
|-----|----------|---------|---------------|
| 🦀 **Rust** | Rust | `nexus-sdk = "0.1.0"` | [sdks/rust/](sdks/rust/README.md) |
| 🐍 **Python** | Python | `pip install nexus-sdk` | [sdks/python/](sdks/python/README.md) |
| 📘 **TypeScript** | TypeScript/Node.js | `npm install nexus-sdk` | [sdks/typescript/](sdks/typescript/README.md) |
| 🐹 **Go** | Go | `go get github.com/hivellm/nexus-go` | [sdks/go/](sdks/go/README.md) |
| 💜 **C#** | .NET | `dotnet add package Nexus.SDK` | [sdks/csharp/](sdks/csharp/README.md) |
| 🔗 **n8n** | n8n Node | Community Node | [sdks/n8n/](sdks/n8n/README.md) |

### **Quick Examples**

**Python:**
```python
from nexus_sdk import NexusClient

async with NexusClient("http://localhost:15474") as client:
    result = await client.execute_cypher("MATCH (n:Person) RETURN n.name LIMIT 10")
    print(f"Found {len(result.rows)} people")
```

**TypeScript:**
```typescript
import { NexusClient } from 'nexus-sdk';

const client = new NexusClient('http://localhost:15474');
const result = await client.executeCypher('MATCH (n:Person) RETURN n.name LIMIT 10');
console.log(`Found ${result.rows.length} people`);
```

**Go:**
```go
client := nexus.NewClient("http://localhost:15474")
result, _ := client.ExecuteCypher("MATCH (n:Person) RETURN n.name LIMIT 10", nil)
fmt.Printf("Found %d people\n", len(result.Rows))
```

**All SDKs include:**
- ✅ Full CRUD operations (nodes, relationships, properties)
- ✅ Cypher query execution with parameters
- ✅ Multi-database management (list, create, switch, drop)
- ✅ Authentication support (API keys)
- ✅ Error handling and retry logic
- ✅ 30+ comprehensive tests

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

### **Replication API**

| Endpoint               | Method | Description                    |
| ---------------------- | ------ | ------------------------------ |
| `/replication/status`  | GET    | Get replication status and lag |
| `/replication/promote` | POST   | Promote replica to master      |
| `/replication/pause`   | POST   | Pause replication              |
| `/replication/resume`  | POST   | Resume replication             |
| `/replication/lag`     | GET    | Get current replication lag    |

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

### **3. Code Analysis & LLM Assistance** 🔥

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

## 🤝 **Contributing**

We welcome contributions! See [**CONTRIBUTING.md**](CONTRIBUTING.md) for guidelines.

### **Development Workflow**

1. **Fork** the repository
2. **Create feature branch**: `git checkout -b feature/your-feature`
3. **Make changes** with tests (95%+ coverage)
4. **Quality checks**: `cargo fmt`, `cargo clippy`, `cargo test`
5. **Commit**: Use conventional commits
6. **Submit PR**: Include description, tests, documentation

### **Rulebook Task Management for Major Features**

For significant features, use **Rulebook** for spec-driven development:

```bash
# View all active tasks
ls rulebook/tasks/

# Check proposal and tasks
cat rulebook/tasks/[task-name]/proposal.md
cat rulebook/tasks/[task-name]/tasks.md
```

**Current Active Tasks:**
- ✅ **Complete Neo4j Cypher** - All 14 phases complete (100%)
- ✅ **Authentication System** - API keys, RBAC, rate limiting (100%)
- ✅ **Neo4j Compatibility** - 287/300 tests passing (95.99%)
- ✅ **Multi-language SDKs** - Rust, Python, TypeScript, Go, C#, n8n (100%)
- 🚧 **OPTIONAL MATCH fixes** - 8 remaining compatibility issues

See `rulebook/RULEBOOK.md` for complete workflow.

## 📜 **License**

Licensed under the **Apache License, Version 2.0**.

See [LICENSE](LICENSE) for details.

## 🙏 **Acknowledgments**

- **[Neo4j](https://neo4j.com/)**: Inspiration for record store architecture and Cypher language
- **[HNSW](https://arxiv.org/abs/1603.09320)**: Efficient approximate nearest neighbor algorithm
- **[OpenCypher](https://opencypher.org/)**: Cypher query language specification ([GitHub](https://github.com/opencypher/openCypher))
- **[Rust Community](https://www.rust-lang.org/)**: Amazing ecosystem of high-performance crates

## 📞 **Contact & Support**

- 🐛 **Issues**: [github.com/hivellm/nexus/issues](https://github.com/hivellm/nexus/issues)
- 💬 **Discussions**: [github.com/hivellm/nexus/discussions](https://github.com/hivellm/nexus/discussions)
- 📧 **Email**: team@hivellm.org
- 🌐 **Repository**: [github.com/hivellm/nexus](https://github.com/hivellm/nexus)

---

<div align="center">

**Built with ❤️ in Rust** 🦀

_Combining the best of graph databases and vector search for the AI era_

[⭐ Star us on GitHub](https://github.com/hivellm/nexus) • [📖 Read the Docs](docs/) • [🚀 Get Started](#-quick-start)

</div>
