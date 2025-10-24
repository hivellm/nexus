# Nexus Graph Database

**High-performance property graph database with native vector search**

Nexus is a modern graph database built for read-heavy workloads with first-class KNN (K-Nearest Neighbors) support. Inspired by Neo4j's architecture, it combines the power of graph traversal with vector similarity search for hybrid RAG (Retrieval-Augmented Generation) applications.

## Features

- **Property Graph Model**: Nodes with labels, relationships with types, both with properties
- **Cypher Subset**: Familiar query language covering 80% of common use cases
- **Native KNN**: HNSW-based vector search integrated into query execution
- **High Performance**: Neo4j-inspired record stores with O(1) traversal
- **ACID Transactions**: WAL + MVCC for durability and snapshot isolation
- **Read-Optimized**: Append-only architecture with page cache and smart indexing

## Quick Start

### Prerequisites

- Rust nightly 1.85+ (edition 2024)
- 8GB+ RAM (recommended)
- Linux/macOS/Windows with WSL

### Installation

```bash
# Clone repository
git clone https://github.com/hivellm/nexus
cd nexus

# Build (release mode)
cargo +nightly build --release

# Run server
./target/release/nexus-server
```

Server starts on `http://localhost:15474` by default.

### Basic Usage

#### Execute Cypher Query

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) WHERE n.age > 25 RETURN n.name, n.age LIMIT 10"
  }'
```

#### KNN Vector Search

```bash
curl -X POST http://localhost:15474/knn_traverse \
  -H "Content-Type: application/json" \
  -d '{
    "label": "Person",
    "vector": [0.1, 0.2, ..., 0.768],
    "k": 10
  }'
```

#### Bulk Ingestion

```bash
curl -X POST http://localhost:15474/ingest \
  -H "Content-Type: application/json" \
  -d '{
    "nodes": [
      {"labels": ["Person"], "properties": {"name": "Alice", "age": 30}}
    ],
    "relationships": [
      {"src": 1, "dst": 2, "type": "KNOWS", "properties": {"since": 2020}}
    ]
  }'
```

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                     REST/HTTP API                         │
│            (Cypher, KNN Traverse, Ingest)                │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                  Cypher Executor                          │
│        Pattern Match • Expand • Filter • Project         │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│               Transaction Layer (MVCC)                    │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                   Index Layer                             │
│   Label Bitmap • B-tree • Full-Text • KNN (HNSW)        │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                  Storage Layer                            │
│  Catalog (LMDB) • WAL • Record Stores • Page Cache      │
└──────────────────────────────────────────────────────────┘
```

### Key Components

- **Catalog**: LMDB-based label/type/key mappings
- **Record Stores**: Fixed-size records for nodes (32B) and relationships (48B)
- **Page Cache**: 8KB pages with clock eviction
- **WAL**: Write-ahead log for durability
- **MVCC**: Epoch-based snapshots for non-blocking reads
- **Indexes**:
  - Label Bitmap (RoaringBitmap)
  - Property B-tree (V1)
  - Full-Text (Tantivy, V1)
  - KNN (HNSW, native)

## Documentation

### Core Documentation

- [Architecture Guide](docs/ARCHITECTURE.md) - Complete system design
- [Development Roadmap](docs/ROADMAP.md) - Implementation phases
- [Component DAG](docs/DAG.md) - Module dependencies

### Specifications

- [Storage Format](docs/specs/storage-format.md) - Record store layouts
- [Cypher Subset](docs/specs/cypher-subset.md) - Supported query syntax
- [Page Cache](docs/specs/page-cache.md) - Memory management
- [WAL & MVCC](docs/specs/wal-mvcc.md) - Transaction model
- [KNN Integration](docs/specs/knn-integration.md) - Vector search
- [API Protocols](docs/specs/api-protocols.md) - REST, MCP, UMICP

## Cypher Query Examples

### Pattern Matching

```cypher
-- Find friends of friends
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)-[:KNOWS]->(fof)
WHERE fof <> me
RETURN DISTINCT fof.name
LIMIT 10
```

### Aggregation

```cypher
-- Top products by category
MATCH (p:Person)-[:BOUGHT]->(prod:Product)
RETURN prod.category, COUNT(*) AS purchases
ORDER BY purchases DESC
LIMIT 10
```

### KNN + Graph Traversal

```cypher
-- Find similar people and their companies
CALL vector.knn('Person', $embedding, 20)
YIELD node AS similar, score
WHERE similar.age > 25
MATCH (similar)-[:WORKS_AT]->(company:Company)
RETURN similar.name, company.name, score
ORDER BY score DESC
LIMIT 10
```

## Performance

### Benchmarks (Single Node, MVP Target)

| Operation | Throughput | Latency (p95) |
|-----------|-----------|---------------|
| Point reads | 100K+ ops/sec | < 1 ms |
| KNN queries (k=10) | 10K+ ops/sec | < 2 ms |
| Pattern traversal | 1K-10K ops/sec | 5-50 ms |
| Bulk ingest | 100K+ nodes/sec | N/A |

### Scaling

- **Nodes**: 1M-100M per instance
- **Relationships**: 2M-200M per instance
- **KNN Index**: 1M-10M vectors per label
- **Memory**: 8GB-64GB recommended
- **Storage**: SSD recommended (NVMe ideal)

## Development

### Project Structure

```
nexus/
├── nexus-core/        # Core graph engine library
│   ├── catalog/       # Label/type/key mappings
│   ├── storage/       # Record stores
│   ├── page_cache/    # Page management
│   ├── wal/           # Write-ahead log
│   ├── index/         # Indexes (bitmap, KNN)
│   ├── executor/      # Cypher execution
│   └── transaction/   # MVCC & locking
├── nexus-server/      # HTTP server (Axum)
├── nexus-protocol/    # Integration protocols
├── tests/             # Integration tests
└── docs/              # Documentation
```

### Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo +nightly build --release

# Run tests
cargo test --all

# Run with coverage
cargo llvm-cov --all --ignore-filename-regex 'examples'
```

### Code Quality

```bash
# Format code
cargo +nightly fmt --all

# Lint (must pass with no warnings)
cargo clippy --workspace -- -D warnings

# Check all targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Configuration

### Environment Variables

```bash
NEXUS_ADDR=127.0.0.1:15474   # Server bind address
NEXUS_DATA_DIR=./data         # Data directory
RUST_LOG=nexus_server=debug   # Logging level
```

### Data Directory Structure

```
data/
├── catalog.mdb          # LMDB catalog
├── nodes.store          # Node records
├── rels.store           # Relationship records
├── props.store          # Property records
├── strings.store        # String/blob dictionary
├── wal.log              # Write-ahead log
└── indexes/
    ├── label_0.bitmap   # Label bitmaps
    └── hnsw_0.bin       # HNSW indexes
```

## Integrations

### Vectorizer Integration

Nexus integrates with [Vectorizer](https://github.com/hivellm/vectorizer) for embedding generation:

```rust
// Generate embedding via Vectorizer
let embedding = vectorizer_client.embed("text to embed").await?;

// Store in graph with KNN index
engine.create_node_with_embedding(
    vec!["Document"],
    properties,
    embedding
)?;

// Query with hybrid search
CALL vector.knn('Document', $query_embedding, 10)
YIELD node, score
MATCH (node)-[:CITES]->(related:Document)
RETURN node, related, score
```

### MCP/UMICP

- **MCP**: Model Context Protocol for AI integrations
- **UMICP**: Universal Model Interoperability for service mesh

See [API Protocols](docs/specs/api-protocols.md) for details.

## Roadmap

### MVP (Phase 1) - Current

- [x] Architecture documentation
- [x] Project scaffolding
- [ ] Storage layer (catalog, record stores, page cache, WAL)
- [ ] Basic indexes (label bitmap, KNN)
- [ ] Cypher executor (MATCH, WHERE, RETURN, ORDER BY, LIMIT)
- [ ] HTTP API
- [ ] Integration tests

### V1 (Phase 2) - Q1 2025

- [ ] Advanced indexes (B-tree, full-text)
- [ ] Constraints (UNIQUE, NOT NULL)
- [ ] Query optimization
- [ ] Bulk loader
- [ ] Monitoring & metrics

### V2 (Phase 3) - Q2 2025

- [ ] Distributed graph (sharding + replication)
- [ ] Read replicas
- [ ] Cluster coordination
- [ ] Multi-region support

See [ROADMAP.md](docs/ROADMAP.md) for detailed timeline.

## Comparison with Neo4j

| Feature | Neo4j | Nexus |
|---------|-------|-------|
| Storage | Record stores + page cache | ✅ Same approach |
| Query Language | Full Cypher | Cypher subset (20%) |
| Transactions | ACID, full MVCC | Simplified MVCC (epochs) |
| Indexes | B-tree, full-text | Same + **native KNN** |
| Clustering | Causal cluster | Future (openraft) |
| Vector Search | Plugin (GDS) | **Native first-class** |
| Target | General graph | **Read-heavy + RAG** |

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

1. Install Rust nightly 1.85+
2. Fork and clone repository
3. Create feature branch
4. Make changes with tests (95%+ coverage required)
5. Run quality checks (`cargo fmt`, `cargo clippy`, `cargo test`)
6. Submit pull request

## License

Dual-licensed under MIT OR Apache-2.0.

See [LICENSE](LICENSE) for details.

## Acknowledgments

- **Neo4j**: Inspiration for record store architecture
- **HNSW**: Efficient approximate nearest neighbor search
- **OpenCypher**: Cypher query language specification
- **Rust Community**: Amazing ecosystem of crates

## Contact

- **Issues**: https://github.com/hivellm/nexus/issues
- **Discussions**: https://github.com/hivellm/nexus/discussions
- **Email**: team@hivellm.org

---

**Built with ❤️ in Rust** 🦀

