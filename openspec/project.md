# Project Context

## Purpose

**Nexus** is a high-performance property graph database designed for read-heavy workloads with native vector search integration. Built in Rust, it combines Neo4j-inspired storage architecture with first-class KNN (K-Nearest Neighbors) support for hybrid RAG (Retrieval-Augmented Generation) applications.

### Core Goals
- **Read-Optimized Performance**: 100K+ point reads/sec, 10K+ KNN queries/sec
- **Native Vector Search**: HNSW-based KNN integrated into query execution (not a plugin)
- **Cypher Compatibility**: 20% Cypher syntax covering 80% of use cases
- **Simple Transactions**: MVCC via epochs, avoiding complex locking
- **Developer Experience**: Familiar graph patterns with modern vector capabilities

## Tech Stack

### Core Technologies
- **Language**: Rust (Edition 2024, Nightly 1.85+)
- **Async Runtime**: Tokio (full features)
- **HTTP Server**: Axum + Tower + Hyper
- **Storage**: memmap2 (memory-mapped files), LMDB (via heed)
- **Indexes**: 
  - RoaringBitmap (label indexes)
  - hnsw_rs (vector KNN)
  - Tantivy (full-text, V1)
- **Serialization**: serde, bincode, serde_json
- **Error Handling**: thiserror, anyhow
- **Observability**: tracing, tracing-subscriber

### Architecture Pattern
**Neo4j-Inspired Record Stores**:
- Fixed-size records (nodes: 32B, relationships: 48B)
- Doubly-linked adjacency lists for O(1) traversal
- Page cache with eviction policies (Clock/2Q/TinyLFU)
- Write-ahead log (WAL) for durability
- MVCC via epoch-based snapshots

## Project Conventions

### Code Style
- **Formatting**: `rustfmt` with nightly toolchain
- **Linting**: Clippy with `-D warnings` (warnings as errors)
- **Naming**:
  - Types: PascalCase (`NodeRecord`, `PageCache`)
  - Functions/variables: snake_case (`read_node`, `page_cache`)
  - Constants: SCREAMING_SNAKE_CASE (`PAGE_SIZE`, `MAX_PAGES`)
- **Documentation**: 
  - Public APIs require `///` doc comments
  - Modules documented with `//!`
  - Include examples in doc comments
- **Error Handling**:
  - Use `Result<T, Error>` for recoverable errors
  - Never `unwrap()` in production code without justification
  - Custom error types via `thiserror`

### Architecture Patterns

#### Storage Layer (Bottom-Up)
1. **Foundation (Layer 0)**:
   - `error` - Error types and Result aliases
   
2. **Storage Primitives (Layer 1)**:
   - `catalog` - Label/Type/Key ID mappings (LMDB)
   - `page_cache` - Page management with eviction
   - `storage` - Record stores (nodes, rels, props, strings)

3. **Durability & Indexing (Layer 2)**:
   - `wal` - Write-ahead log
   - `index` - Bitmap, B-tree, full-text, KNN

4. **Transaction Management (Layer 3)**:
   - `transaction` - MVCC and locking

5. **Query Execution (Layer 4)**:
   - `executor` - Cypher parser, planner, operators

6. **Network (Layer 5)**:
   - `nexus-server` - HTTP/REST API
   - `nexus-protocol` - Integration protocols

#### Dependency Rules
- **Allowed**: Higher layers depend on lower layers
- **Forbidden**: 
  - Lower layers depending on higher layers
  - Circular dependencies
  - Direct file I/O in high layers (use storage layer)

### Testing Strategy

#### Requirements
- **Minimum Coverage**: 95%
- **Test Location**: `/tests` for integration, `#[cfg(test)]` for unit tests
- **Execution**: 100% passing before moving to next task
- **Test-First**: Write tests based on specs before implementation

#### Test Structure
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_node_crud() {
        // Arrange
        let store = RecordStore::new().unwrap();
        let node = NodeRecord { /* ... */ };
        
        // Act
        store.write_node(42, &node).unwrap();
        let read = store.read_node(42).unwrap();
        
        // Assert
        assert_eq!(read.label_bits, node.label_bits);
    }

    #[tokio::test]
    async fn test_async_query() {
        let engine = Engine::new().unwrap();
        let result = engine.execute("MATCH (n) RETURN n").await.unwrap();
        assert!(result.rows.len() > 0);
    }
}
```

#### Test Types
1. **Unit Tests**: Per-module functionality in isolation
2. **Integration Tests**: Cross-module interactions (`/tests`)
3. **End-to-End Tests**: Full query execution flows
4. **Performance Tests**: Benchmarks for throughput/latency targets

### Git Workflow

#### Commit Conventions (Conventional Commits)
```bash
feat(catalog): Add label ID caching for faster lookups
fix(wal): Correct checkpoint truncation logic
docs(readme): Update quick start examples
test(storage): Add crash recovery integration tests
chore(deps): Update heed to 0.20.5
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`

#### Branching Strategy
- **main**: Production-ready code
- **feature/[name]**: New features
- **hotfix/[name]**: Critical fixes

#### Quality Gates
**Before ANY commit**:
```bash
cargo +nightly fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --release
```

**Never commit**: Failing tests, clippy warnings, unformatted code

#### Push Mode
**MANUAL** - Always provide push commands (SSH with password):
```
✋ MANUAL ACTION REQUIRED:
Run manually: git push origin main
```

## Domain Context

### Graph Database Concepts

#### Property Graph Model
- **Nodes**: Entities with labels and properties
  - Labels: Categorize nodes (e.g., `Person`, `Company`)
  - Properties: Key-value pairs (e.g., `{name: "Alice", age: 30}`)
- **Relationships**: Directed edges with type and properties
  - Type: Categorizes relationship (e.g., `KNOWS`, `WORKS_AT`)
  - Direction: Always src → dst

#### Cypher Query Language (Subset)
```cypher
-- Pattern matching
MATCH (n:Person)-[r:KNOWS]->(m:Person)
WHERE n.age > 25
RETURN n.name, m.name
ORDER BY n.age DESC
LIMIT 10

-- KNN + Graph hybrid
CALL vector.knn('Person', $embedding, 10)
YIELD node AS similar, score
MATCH (similar)-[:WORKS_AT]->(company)
RETURN similar.name, company.name, score
```

### Vector Search (KNN)
- **HNSW Algorithm**: Hierarchical Navigable Small World graphs
- **Per-Label Indexes**: Separate vector space for each node label
- **Distance Metrics**: Cosine similarity (default), Euclidean
- **Parameters**:
  - `M`: Max connections per layer (typically 16-48)
  - `ef_construction`: Build quality (200-400)
  - `ef_search`: Search quality (typically 2×k to 10×k)

### RAG (Retrieval-Augmented Generation)
Nexus is optimized for RAG workflows:
1. **Semantic Retrieval**: KNN to find similar documents/entities
2. **Graph Expansion**: Traverse relationships to find context
3. **Hybrid Filtering**: Combine vector similarity with graph patterns

## Important Constraints

### Performance Targets (MVP)
- **Point Reads**: 100,000+ ops/sec (single node)
- **KNN Queries**: 10,000+ ops/sec (k=10)
- **Pattern Traversal**: 1,000-10,000 ops/sec (depth-dependent)
- **Bulk Ingest**: 100,000+ nodes/sec
- **Test Coverage**: 95%+ across all modules

### Technical Constraints
- **Rust Edition**: Must use 2024 (no fallback to 2021)
- **Toolchain**: Nightly 1.85+ required
- **Single-Writer (MVP)**: One write transaction at a time (simplifies MVCC)
- **Page Size**: Fixed at 8KB (no dynamic resizing)
- **Max Vector Dimension**: 4096 (practical limit for HNSW)

### Operational Constraints
- **Deployment**: Single binary (server + engine in same process)
- **Storage**: SSD recommended, NVMe ideal (memmap2 performance)
- **Memory**: Minimum 8GB RAM, 16GB+ recommended
- **OS**: Linux primary target, macOS/Windows via WSL

### Design Constraints
- **Simplicity First**: <100 lines new code by default
- **Append-Only**: Record stores are immutable until compaction
- **No Blocking**: Never block in async context (use `spawn_blocking`)
- **Zero Unsafe**: Avoid `unsafe` unless absolutely necessary (and well-documented)

## External Dependencies

### Storage Systems
- **LMDB (via heed)**: Catalog storage for mappings
  - Version: 0.20.5
  - Purpose: Label/Type/Key → ID bidirectional maps
  - Why: Proven embedded KV store, ACID guarantees

### Indexing
- **hnsw_rs**: Vector similarity search
  - Version: 0.3.2
  - Purpose: K-nearest neighbors via HNSW algorithm
  - Why: Pure Rust, good performance, no external deps
  
- **Tantivy (V1)**: Full-text search
  - Version: 0.22.1
  - Purpose: BM25 text search per (label, key)
  - Why: Rust-native, Lucene-inspired, battle-tested

- **RoaringBitmap**: Compressed bitmaps
  - Version: 0.10.12
  - Purpose: Label index (label_id → bitmap of node_ids)
  - Why: Excellent compression, fast operations

### Integration Points
- **Vectorizer**: Embedding generation service
  - Protocol: REST/HTTP + MCP
  - Purpose: Generate vector embeddings for text
  - Integration: `nexus-protocol` client

- **Synap**: Distributed KV/Pub-Sub (optional)
  - Purpose: Graph change events, distributed cache
  - Integration: Event publishing on mutations

- **UMICP**: Universal Model Interoperability
  - Purpose: Service mesh communication
  - Integration: Cross-service graph queries

### Monitoring & Observability
- **Prometheus**: Metrics export
  - Metrics: Query latency, cache hit rate, throughput
  - Endpoint: `/metrics` (V1)

- **OpenTelemetry**: Distributed tracing
  - Traces: Query execution spans
  - Integration: Via `tracing-opentelemetry` (V1)

## Development Workflow with OpenSpec

### When to Create Proposals
**Required for**:
- New capabilities (features, APIs)
- Breaking changes (storage format, API)
- Architecture changes (new modules, patterns)
- Performance optimizations (changing behavior)

**Skip for**:
- Bug fixes (restore spec behavior)
- Typos, formatting, comments
- Non-breaking dependency updates
- Configuration changes

### Example: Adding a Feature
```bash
# 1. Check existing specs
openspec list --specs
openspec show storage-layer

# 2. Create proposal
mkdir -p openspec/changes/add-graph-algorithms/{specs/algorithms}

# 3. Write proposal.md
cat > openspec/changes/add-graph-algorithms/proposal.md << 'EOF'
## Why
Users need shortest path and centrality algorithms for graph analytics.

## What Changes
- Add algorithms module (PageRank, BFS, Dijkstra)
- Expose via Cypher procedures
- **BREAKING**: None

## Impact
- Affected specs: NEW capability `algorithms`
- Affected code: nexus-core/src/algorithms/mod.rs (new)
EOF

# 4. Validate
openspec validate add-graph-algorithms --strict
```

### Typical Development Cycle
1. **Design Phase**: Create OpenSpec proposal
2. **Approval**: Review proposal.md + design.md
3. **Implementation**: Follow tasks.md checklist
4. **Testing**: 95%+ coverage, all tests passing
5. **Quality Checks**: fmt, clippy, build
6. **Commit**: Conventional commit message
7. **Archive**: Move to `openspec/changes/archive/` after deployment

## Quick Reference

### Build Commands
```bash
# Development build
cargo build --workspace

# Release build
cargo +nightly build --release --workspace

# Run tests
cargo test --workspace --verbose

# Check coverage
cargo llvm-cov --workspace --ignore-filename-regex 'examples'

# Format & Lint
cargo +nightly fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Project Structure
```
nexus/
├── nexus-core/        # Core engine (catalog, storage, executor)
├── nexus-server/      # HTTP API (Axum)
├── nexus-protocol/    # Integration protocols
├── tests/             # Integration tests
├── docs/              # Documentation
│   ├── ARCHITECTURE.md
│   ├── ROADMAP.md
│   ├── DAG.md
│   └── specs/         # Detailed specifications
├── openspec/          # OpenSpec proposals
│   ├── project.md     # This file
│   ├── specs/         # Current capabilities
│   └── changes/       # Proposals & archive
├── README.md
├── CHANGELOG.md
└── AGENTS.md          # AI assistant rules
```

### Key Documentation
- **Architecture**: `docs/ARCHITECTURE.md` - System design
- **Roadmap**: `docs/ROADMAP.md` - Implementation phases (MVP, V1, V2)
- **DAG**: `docs/DAG.md` - Component dependency graph
- **Specs**: `docs/specs/` - Detailed feature specifications
  - `storage-format.md` - Record store layouts
  - `cypher-subset.md` - Supported Cypher syntax
  - `page-cache.md` - Memory management
  - `wal-mvcc.md` - Transaction model
  - `knn-integration.md` - Vector search
  - `api-protocols.md` - REST, MCP, UMICP

### Support
- **Issues**: https://github.com/hivellm/nexus/issues
- **Discussions**: https://github.com/hivellm/nexus/discussions
- **Email**: team@hivellm.org
