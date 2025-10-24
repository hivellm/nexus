# Nexus Graph Database

**⚡ High-performance property graph database with native vector search**

<div align="center">

![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Edition](https://img.shields.io/badge/edition-2024-blue.svg)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green.svg)
![Status](https://img.shields.io/badge/status-MVP%20Development-yellow.svg)

[Features](#-key-features) • [Quick Start](#-quick-start) • [Documentation](#-documentation) • [Roadmap](#-roadmap) • [Contributing](#-contributing)

</div>

---

## 🎯 **What is Nexus?**

Nexus is a modern **property graph database** built for **read-heavy workloads** with **first-class vector search**. Inspired by Neo4j's battle-tested architecture, it combines the power of graph traversal with semantic similarity search for hybrid **RAG (Retrieval-Augmented Generation)** applications.

Think of it as **Neo4j meets Vector Search** - optimized for AI applications that need both structured relationships and semantic similarity.

## ✨ **Version 0.1.0 - Architecture & Foundation**

### 🎉 **Current Status**
- ✅ **Complete Architecture Documentation** - Neo4j-inspired design with native KNN
- ✅ **Project Scaffolding** - Cargo workspace (edition 2024, nightly)
- ✅ **Module Structure** - Storage, executor, indexes, transactions
- ✅ **REST API Framework** - Axum-based HTTP server
- ✅ **OpenSpec Integration** - Spec-driven development workflow
- 📋 **MVP Implementation** - In progress (Phase 1)

## 🌟 **Key Features**

### **Graph Database**
- 🏗️ **Property Graph Model**: Nodes with labels, relationships with types, both with properties
- 🔍 **Cypher Subset**: Familiar query language covering 80% of common use cases
- ⚡ **Neo4j-Inspired Storage**: Fixed-size record stores (32B nodes, 48B relationships)
- 🔗 **O(1) Traversal**: Doubly-linked adjacency lists without index lookups
- 💾 **ACID Transactions**: WAL + MVCC for durability and snapshot isolation

### **Vector Search (Native KNN)**
- 🎯 **HNSW Indexes**: Hierarchical Navigable Small World for fast approximate search
- 📊 **Per-Label Indexes**: Separate vector space for each node label
- 🔄 **Hybrid Queries**: Combine vector similarity with graph traversal in single query
- ⚡ **High Performance**: 10,000+ KNN queries/sec (k=10)
- 📐 **Multiple Metrics**: Cosine similarity, Euclidean distance

### **Performance & Scalability**
- 🚀 **100K+ point reads/sec** - Direct offset access via record IDs
- ⚡ **10K+ KNN queries/sec** - Logarithmic HNSW search
- 📈 **1K-10K pattern traversals/sec** - Efficient expand operations
- 💾 **8KB Page Cache** - Clock/2Q/TinyLFU eviction policies
- 🔄 **Append-Only Architecture**: Predictable write performance

### **Integration**
- 🌐 **REST/HTTP API**: Cypher, KNN traverse, bulk ingestion
- 🔌 **MCP Protocol**: Model Context Protocol for AI integrations
- 🔗 **UMICP Support**: Universal Model Interoperability
- 🤝 **Vectorizer Integration**: Native embedding generation support

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

#### **3️⃣ Bulk Data Ingestion**

```bash
curl -X POST http://localhost:15474/ingest \
  -H "Content-Type: application/json" \
  -d '{
    "nodes": [
      {"labels": ["Person"], "properties": {"name": "Alice", "age": 30}},
      {"labels": ["Person"], "properties": {"name": "Bob", "age": 28}}
    ],
    "relationships": [
      {"src": 1, "dst": 2, "type": "KNOWS", "properties": {"since": 2020}}
    ]
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

### **Detailed Specifications**

- 💾 [**Storage Format**](docs/specs/storage-format.md) - Record store binary layouts
- 📝 [**Cypher Subset**](docs/specs/cypher-subset.md) - Supported query syntax & examples
- 🧠 [**Page Cache**](docs/specs/page-cache.md) - Memory management & eviction policies
- 📋 [**WAL & MVCC**](docs/specs/wal-mvcc.md) - Transaction model & crash recovery
- 🎯 [**KNN Integration**](docs/specs/knn-integration.md) - Vector search & hybrid queries
- 🌐 [**API Protocols**](docs/specs/api-protocols.md) - REST, MCP, UMICP specifications

## 🗺️ **Roadmap**

### **📋 MVP (Phase 1)** - Current Development

- [x] Architecture documentation
- [x] Project scaffolding (Rust edition 2024)
- [ ] **Storage Layer** (catalog, record stores, page cache, WAL)
- [ ] **Basic Indexes** (label bitmap, KNN/HNSW)
- [ ] **Cypher Executor** (MATCH, WHERE, RETURN, ORDER BY, LIMIT)
- [ ] **HTTP API** (complete endpoints)
- [ ] **Integration Tests** (95%+ coverage)

**Target**: Q4 2024

### **🎯 V1 (Phase 2)** - Advanced Features

- [ ] **Advanced Indexes** (B-tree for properties, Tantivy full-text)
- [ ] **Constraints** (UNIQUE, NOT NULL, CHECK)
- [ ] **Query Optimization** (cost-based planner with statistics)
- [ ] **Bulk Loader** (bypass WAL for fast initial load)
- [ ] **Monitoring & Metrics** (Prometheus, OpenTelemetry)

**Target**: Q1 2025

### **🚀 V2 (Phase 3)** - Distributed Graph

- [ ] **Sharding** (hash-based node partitioning)
- [ ] **Replication** (Raft consensus via openraft)
- [ ] **Cluster Coordination** (distributed query execution)
- [ ] **Multi-Region Support** (cross-datacenter replication)

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

## 🔗 **Integrations**

### **Vectorizer Integration**

Nexus integrates seamlessly with [Vectorizer](https://github.com/hivellm/vectorizer) for embedding generation:

```rust
// Generate embedding via Vectorizer
let vectorizer = VectorizerClient::new("http://localhost:15002");
let embedding = vectorizer.embed("machine learning algorithms").await?;

// Store in graph with KNN index
engine.create_node_with_embedding(
    vec!["Document"],
    json!({"title": "ML Guide", "content": "..."}),
    embedding
)?;

// Hybrid semantic + graph search
CALL vector.knn('Document', $query_embedding, 10)
YIELD node AS doc, score
MATCH (doc)-[:CITES]->(related:Document)
RETURN doc.title, related.title, score
ORDER BY score DESC
```

### **Protocol Support**

- 🌐 **REST/HTTP**: Default integration (streamable HTTP)
- 🔌 **MCP**: Model Context Protocol for AI services
- 🔗 **UMICP**: Universal Model Interoperability Protocol

See [**API Protocols**](docs/specs/api-protocols.md) for complete specifications.

## 🧪 **Testing**

### **Requirements**

- ✅ **95%+ test coverage** (strictly enforced)
- ✅ **100% tests passing** before any commit
- ✅ Unit tests in modules (`#[cfg(test)]`)
- ✅ Integration tests in `/tests`

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

### **3. Knowledge Graph + Semantic Search**

```cypher
-- Find related entities via embeddings and relationships
CALL vector.knn('Entity', $entity_embedding, 20)
YIELD node AS entity, score
MATCH (entity)-[r:RELATED_TO]->(related:Entity)
RETURN entity.name, type(r), related.name, score
ORDER BY score DESC
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
# Check existing specs
openspec list --specs

# Create proposal
openspec init add-my-feature

# Validate proposal
openspec validate add-my-feature --strict
```

See `openspec/AGENTS.md` for complete workflow.

## 📜 **License**

Dual-licensed under **MIT OR Apache-2.0**.

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
