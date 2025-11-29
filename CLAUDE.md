# Nexus - Claude Code Development Guide

> **Last Updated**: 2025-11-28
> **Version**: 0.12.0
> **Status**: ✅ Production Ready - 195/195 Neo4j Compatibility Tests Passing

---

## 📋 Table of Contents

1. [Quick Reference](#quick-reference)
2. [Project Overview](#project-overview)
3. [Common Commands](#common-commands)
4. [Architecture Overview](#architecture-overview)
5. [Development Workflow](#development-workflow)
6. [Testing Strategy](#testing-strategy)
7. [SDK Development](#sdk-development)
8. [Key Patterns & Conventions](#key-patterns--conventions)
9. [Important Constraints](#important-constraints)
10. [Troubleshooting](#troubleshooting)

---

## Quick Reference

### Most Common Commands

```bash
# Build release version (ALWAYS use for production/testing)
cargo +nightly build --release --workspace

# Run server (development)
./target/release/nexus-server

# Run all tests
cargo test --workspace --verbose

# Run specific test
cargo test --package nexus-core --test test_name

# Check code quality
cargo +nightly fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run Neo4j compatibility tests (195 tests)
powershell -ExecutionPolicy Bypass -File scripts/test-neo4j-nexus-compatibility-200.ps1
```

### Server Access Points

- **REST API**: `http://localhost:15474`
- **Cypher Endpoint**: `POST /cypher`
- **KNN Search**: `POST /knn_traverse`
- **Health Check**: `GET /health`
- **Statistics**: `GET /stats`
- **Database Management**: `GET /databases`, `POST /databases`, `DELETE /databases/{name}`

---

## Project Overview

**Nexus** is a high-performance property graph database with native vector search, designed for read-heavy workloads and RAG (Retrieval-Augmented Generation) applications.

### Key Characteristics

- **Neo4j-Inspired Architecture**: Fixed-size record stores, O(1) traversal via linked lists
- **~55% openCypher Compatibility**: Core clauses + ~60 functions (195/195 compatibility tests passing)
- **Native Vector Search**: First-class HNSW KNN indexes per label
- **ACID Transactions**: Simplified MVCC via epochs, single-writer model
- **Multi-Database Support**: Isolated databases within single server instance
- **Production Ready**: 1478+ tests passing, 70%+ coverage

### Target Use Cases

✅ **Perfect for:**
- RAG applications (semantic + graph traversal)
- Recommendation systems with hybrid search
- Knowledge graphs with vector embeddings
- Code analysis and LLM assistance (call graphs, dependency analysis)
- Social networks with similarity search

❌ **Not ideal for:**
- Write-heavy OLTP workloads
- Simple key-value storage
- Complex graph algorithms requiring full Cypher

---

## Common Commands

### Build & Run

```bash
# Development build
cargo build --workspace

# Release build (ALWAYS use for production/testing)
cargo +nightly build --release --workspace

# Build specific package
cargo build --release --package nexus-core
cargo build --release --package nexus-server
cargo build --release --package nexus-cli

# Start server (release mode)
./target/release/nexus-server

# Start server with custom config
NEXUS_ADDR=0.0.0.0:15474 NEXUS_DATA_DIR=./custom-data ./target/release/nexus-server

# Start server in background (Windows)
start /B target\release\nexus-server.exe

# Start server in background (Linux/WSL)
nohup ./target/release/nexus-server > /tmp/nexus-server.log 2>&1 &
```

### Testing

```bash
# Run all tests
cargo test --workspace --verbose

# Run tests for specific package
cargo test --package nexus-core
cargo test --package nexus-server

# Run specific test file
cargo test --package nexus-core --test integration_test

# Run test with name filter
cargo test --package nexus-core test_create_node

# Run Neo4j compatibility tests (195 tests)
powershell -ExecutionPolicy Bypass -File scripts/test-neo4j-nexus-compatibility-200.ps1

# Check test coverage (95%+ required)
cargo llvm-cov --workspace --ignore-filename-regex 'examples'
```

### Code Quality

```bash
# Format code (MUST run before commit)
cargo +nightly fmt --all

# Lint code (NO warnings allowed)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Check for unused dependencies
cargo +nightly udeps --workspace

# Quality check pipeline (run before commit)
cargo +nightly fmt --all && \
cargo clippy --workspace -- -D warnings && \
cargo test --workspace --verbose
```

### SDK Testing

```bash
# Run all SDK tests (automated)
powershell -ExecutionPolicy Bypass -File sdks/run-all-comprehensive-tests.ps1

# Test individual SDKs
cd sdks/python && python test_sdk_comprehensive.py
cd sdks/typescript && npx tsx test-sdk-comprehensive.ts
cd sdks/rust && cargo run --example test_sdk
cd sdks/go/test && go run test_sdk.go
cd sdks/TestConsoleSimple && dotnet run
cd sdks/n8n && npx tsx test-integration.ts
```

### CLI Development

```bash
# Build CLI
cargo build --release --package nexus-cli

# Run CLI
./nexus-cli/target/release/nexus --help

# Test CLI commands
./nexus-cli/target/release/nexus db list
./nexus-cli/target/release/nexus query "MATCH (n) RETURN count(n)"
```

---

## Architecture Overview

### System Layers

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

### Core Components

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Catalog** | LMDB (heed) | Label/Type/Key → ID bidirectional mappings |
| **Record Stores** | memmap2 | Fixed-size node (32B) & relationship (48B) records |
| **Page Cache** | Custom | 8KB pages with Clock/2Q/TinyLFU eviction |
| **WAL** | Append-only log | Write-ahead log for crash recovery |
| **MVCC** | Epoch-based | Snapshot isolation without locking readers |
| **Label Index** | RoaringBitmap | Compressed bitmap per label |
| **KNN Index** | hnsw_rs | HNSW vector search per label |
| **Executor** | Custom | Cypher parser, planner, operators |

### Project Structure

```
nexus/
├── nexus-core/           # 🧠 Core graph engine library
│   ├── src/
│   │   ├── catalog/      # Label/type/key mappings (LMDB)
│   │   ├── storage/      # Record stores (nodes, rels, props)
│   │   ├── page_cache/   # Memory management (8KB pages)
│   │   ├── wal/          # Write-ahead log
│   │   ├── index/        # Indexes (bitmap, KNN, full-text)
│   │   ├── executor/     # Cypher parser & execution
│   │   ├── transaction/  # MVCC & locking
│   │   ├── graph/        # Graph algorithms & correlation analysis
│   │   └── auth/         # Authentication & authorization
│   └── tests/            # Core tests
├── nexus-server/         # 🌐 HTTP server (Axum)
│   ├── src/
│   │   ├── api/          # REST endpoints
│   │   ├── middleware/   # Auth, rate limiting
│   │   └── config.rs     # Server configuration
│   └── tests/            # Server tests
├── nexus-protocol/       # 🔌 Integration protocols
│   └── src/
│       ├── rest.rs       # REST client
│       ├── mcp.rs        # MCP client
│       └── umicp.rs      # UMICP client
├── nexus-cli/            # 💻 Command-line interface
│   └── src/
│       ├── commands/     # CLI commands
│       └── config.rs     # CLI configuration
├── sdks/                 # 📦 Official SDKs
│   ├── rust/             # Rust SDK
│   ├── python/           # Python SDK
│   ├── typescript/       # TypeScript SDK
│   ├── go/               # Go SDK
│   ├── csharp/           # C# SDK
│   └── n8n/              # n8n community node
├── tests/                # 🧪 Integration tests
│   └── cross-compatibility/ # Neo4j compatibility tests
├── docs/                 # 📚 Documentation
│   ├── ARCHITECTURE.md   # System architecture
│   ├── ROADMAP.md        # Development roadmap
│   ├── USER_GUIDE.md     # User documentation
│   └── specs/            # Detailed specifications
└── scripts/              # 🔧 Utility scripts
    ├── install.sh        # Linux/macOS installer
    └── install.ps1       # Windows installer
```

---

## Development Workflow

### 1. Before Starting Development

**CRITICAL**: Always check existing documentation first:

```bash
# Check architecture
cat docs/ARCHITECTURE.md

# Check roadmap for planned features
cat docs/ROADMAP.md

# Check active tasks
ls rulebook/tasks/

# Review AGENTS.md for standards
cat AGENTS.md
```

### 2. Feature Development Process

#### For Small Changes (<200 lines)
```bash
# 1. Create feature branch
git checkout -b feature/your-feature-name

# 2. Make changes with tests
# 3. Run quality checks
cargo +nightly fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace

# 4. Commit
git commit -m "feat(component): Add feature description"
```

#### For Large Features (>200 lines or architectural changes)
```bash
# 1. Check if task exists in rulebook
ls rulebook/tasks/

# 2. If task exists, follow tasks.md
cat rulebook/tasks/[task-name]/tasks.md

# 3. If no task, create proposal first
# See rulebook/RULEBOOK.md for process
```

### 3. Code Quality Checklist

**MUST pass before committing:**

- [ ] `cargo +nightly fmt --all` (no changes)
- [ ] `cargo clippy --workspace -- -D warnings` (no warnings)
- [ ] `cargo test --workspace --verbose` (100% pass)
- [ ] Test coverage ≥ 95% for new code
- [ ] Documentation updated
- [ ] CHANGELOG.md updated (if applicable)

### 4. Commit Message Format

Use conventional commits:

```bash
# Format: <type>(<scope>): <description>

# Examples:
git commit -m "feat(storage): Add page cache eviction policy"
git commit -m "fix(executor): Correct WHERE clause evaluation"
git commit -m "docs(readme): Update quick start examples"
git commit -m "test(core): Add relationship traversal tests"
git commit -m "refactor(index): Simplify KNN search logic"
git commit -m "perf(cache): Optimize cache hit rate"
```

**Commit types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `test`: Adding tests
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `chore`: Maintenance tasks

---

## Testing Strategy

### Test Coverage Requirements

**CRITICAL**: 95%+ test coverage required for all new code.

### Test Organization

```
nexus-core/
├── src/
│   ├── module.rs           # Implementation
│   └── module.rs (tests)   # Unit tests (#[cfg(test)] module)
└── tests/
    └── integration_test.rs # Integration tests

nexus-server/
├── src/
│   └── api/endpoint.rs     # Implementation + unit tests
└── tests/
    └── integration_tests.rs # Server integration tests

tests/
└── cross-compatibility/    # Neo4j compatibility tests
```

### Test Types

#### 1. Unit Tests (in module files)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let record = NodeRecord { /* ... */ };
        assert_eq!(record.label_bits, 0x01);
    }

    #[tokio::test]
    async fn test_async_query() {
        let engine = Engine::new().unwrap();
        let result = engine.execute("MATCH (n) RETURN n").await.unwrap();
        assert!(result.rows.len() >= 0);
    }
}
```

#### 2. Integration Tests (in /tests directory)

```bash
# Run all integration tests
cargo test --workspace

# Run specific integration test
cargo test --test integration_test

# Run Neo4j compatibility tests
powershell -ExecutionPolicy Bypass -File scripts/test-neo4j-nexus-compatibility-200.ps1
```

#### 3. SDK Tests

All SDKs have comprehensive test suites (30+ tests each):

```bash
# Run all SDK tests
powershell -ExecutionPolicy Bypass -File sdks/run-all-comprehensive-tests.ps1

# Individual SDK tests
cd sdks/python && python test_sdk_comprehensive.py
cd sdks/typescript && npx tsx test-sdk-comprehensive.ts
```

### Neo4j Compatibility Testing

**CRITICAL**: We maintain 100% Neo4j compatibility (195/195 tests passing).

```bash
# Run full compatibility suite
powershell -ExecutionPolicy Bypass -File scripts/test-neo4j-nexus-compatibility-200.ps1

# Results should show:
# ✅ Section 1: Basic Queries (20/20)
# ✅ Section 2: Pattern Matching (25/25)
# ✅ Section 3: Aggregations (15/15)
# ... (195 total tests)
```

---

## SDK Development

### SDK Architecture

All SDKs follow consistent patterns:

1. **Row Format**: Server returns Neo4j-compatible array format `[[value1, value2]]`
2. **Endpoint**: `/cypher` (not `/query`)
3. **Request Body**: `{"query": "...", "parameters": {...}}`
4. **Response Format**: `{"columns": [...], "rows": [...], "execution_time_ms": ...}`

### SDK Compatibility Patterns

#### Languages with Native Support
- **Python**: Uses `list[Any]` (flexible types)
- **TypeScript**: Uses `any[]` (flexible types)
- **Rust**: Uses `serde_json::Value` (flexible types)
- **n8n**: Uses TypeScript types (flexible)

#### Languages with Helper Methods
- **Go**: `QueryResult.RowsAsMap()` converts `[][]interface{}` to `[]map[string]interface{}`
- **C#**: `QueryResult.RowsAsMap()` converts `List<List<object?>>` to `List<Dictionary<string, object?>>`

### Adding a New SDK

```bash
# 1. Create SDK directory
mkdir -p sdks/[language-name]

# 2. Implement core client with methods:
#    - executeCypher(query, parameters)
#    - ping()
#    - listDatabases(), createDatabase(), dropDatabase()
#    - switchDatabase()

# 3. Ensure row format compatibility
#    - Option A: Use flexible JSON types (Python, TypeScript, Rust)
#    - Option B: Add RowsAsMap() helper (Go, C#)

# 4. Create comprehensive test suite (30+ tests):
#    - CRUD operations
#    - Parameterized queries
#    - Aggregations (COUNT, SUM, AVG, MAX, MIN, COLLECT)
#    - String functions (toUpper, toLower, substring)
#    - Mathematical operations
#    - NULL handling
#    - CASE expressions
#    - UNION queries
#    - MERGE operations

# 5. Add to SDK test runner
#    Edit sdks/run-all-comprehensive-tests.ps1

# 6. Update documentation
#    - sdks/[language]/README.md
#    - sdks/TEST_COVERAGE_REPORT.md
```

### SDK Testing Checklist

- [ ] All CRUD operations working
- [ ] Parameterized queries tested
- [ ] All aggregation functions tested (COUNT, SUM, AVG, MAX, MIN, COLLECT)
- [ ] String functions tested
- [ ] Mathematical operations tested
- [ ] NULL handling tested
- [ ] CASE expressions tested
- [ ] UNION queries tested
- [ ] MERGE operations tested
- [ ] Database management tested (list, create, drop, switch)
- [ ] Error handling tested
- [ ] Row format compatible with server
- [ ] 30+ tests passing
- [ ] Documentation complete

---

## Key Patterns & Conventions

### Rust Code Conventions

```rust
// Naming
fn my_function() {}           // snake_case for functions/variables
struct MyStruct {}            // PascalCase for types/traits
const MAX_SIZE: usize = 100;  // SCREAMING_SNAKE_CASE for constants

// Error handling - use Result<T>
fn read_node(&self, node_id: u64) -> Result<NodeRecord> {
    // Implementation
}

// Async functions - use tokio
#[tokio::test]
async fn test_async_query() {
    let result = engine.execute("MATCH (n) RETURN n").await.unwrap();
    assert!(result.rows.len() >= 0);
}

// Documentation - all public APIs must have docs
/// Reads a node record from storage.
///
/// # Arguments
///
/// * `node_id` - The ID of the node to read
///
/// # Examples
///
/// ```
/// let node = store.read_node(42)?;
/// ```
///
/// # Errors
///
/// Returns `Error::NotFound` if node doesn't exist.
pub fn read_node(&self, node_id: u64) -> Result<NodeRecord> {
    // Implementation
}
```

### Storage Record Formats

#### Node Record (32 bytes)
```rust
struct NodeRecord {
    label_bits: u64,      // Bitmap of label IDs (64 labels max)
    first_rel_ptr: u64,   // Head of relationship linked list
    prop_ptr: u64,        // Pointer to property chain
    flags: u64,           // Deleted, locked, version bits
}
```

#### Relationship Record (48 bytes)
```rust
struct RelationshipRecord {
    src_id: u64,          // Source node ID
    dst_id: u64,          // Destination node ID
    type_id: u32,         // Relationship type ID
    next_src_ptr: u64,    // Next relationship from source
    next_dst_ptr: u64,    // Next relationship to destination
    prop_ptr: u64,        // Pointer to properties
    flags: u32,           // Status flags
}
```

### API Response Format

All API endpoints return consistent format:

```json
{
  "columns": ["n.name", "n.age"],
  "rows": [
    ["Alice", 30],
    ["Bob", 25]
  ],
  "execution_time_ms": 3,
  "stats": {
    "nodes_created": 0,
    "relationships_created": 0,
    "properties_set": 0
  }
}
```

### Database Management Patterns

```bash
# List databases
curl http://localhost:15474/databases

# Create database
curl -X POST http://localhost:15474/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "mydb"}'

# Switch database (session)
curl -X PUT http://localhost:15474/session/database \
  -H "Content-Type: application/json" \
  -d '{"name": "mydb"}'

# Drop database
curl -X DELETE http://localhost:15474/databases/mydb
```

---

## Important Constraints

### CRITICAL Constraints

#### 1. Neo4j Compatibility - DO NOT BREAK

**NEVER modify server response formats** - it breaks Neo4j compatibility.

```rust
// ❌ WRONG - Don't change row format
{"rows": [{"name": "Alice", "age": 30}]}  // Object format

// ✅ CORRECT - Always use Neo4j array format
{"rows": [["Alice", 30]]}  // Array format
```

**Why**: We maintain 100% Neo4j compatibility (195/195 tests passing). Changing server format would break all SDKs and compatibility tests.

**Solution**: If SDKs need different formats, add helper methods in SDKs (like `RowsAsMap()`), never change server.

#### 2. Test Coverage - NO EXCEPTIONS

- **Minimum**: 95% coverage for all new code
- **Check**: `cargo llvm-cov --workspace --ignore-filename-regex 'examples'`
- **Enforcement**: PRs with <95% coverage will not be merged

#### 3. Code Quality - ZERO WARNINGS

```bash
# MUST pass before commit
cargo +nightly fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings

# NO warnings allowed
```

#### 4. Database Isolation

Each database is completely isolated:
- Separate LMDB catalog
- Separate record stores
- Separate indexes
- No cross-database queries (by design)

#### 5. Single-Writer Transaction Model

- **ONE writer** at a time per partition
- Epoch-based MVCC for readers
- No distributed transactions (V1)

### Performance Constraints

- **Target Throughput**: 100K+ point reads/sec, 10K+ KNN queries/sec
- **Target Latency**: <1ms point reads, <2ms KNN queries (p95)
- **Memory**: Record stores memory-mapped, page cache for hot data
- **Storage**: SSD recommended, NVMe ideal

### Security Constraints

- **Default**: Authentication DISABLED for localhost (127.0.0.1)
- **Production**: Authentication REQUIRED for public binding (0.0.0.0)
- **Rate Limits**: 1000 req/min, 10000 req/hour per API key
- **API Keys**: 32-character random strings, Argon2 hashed

---

## Troubleshooting

### Common Issues

#### 1. Build Failures

```bash
# Issue: "edition 2024 is unstable"
# Solution: Use nightly toolchain
cargo +nightly build --release --workspace

# Issue: Dependency conflicts
# Solution: Update dependencies
cargo update

# Issue: Out of memory during build
# Solution: Reduce codegen units
# Edit Cargo.toml: codegen-units = 1
```

#### 2. Test Failures

```bash
# Issue: Tests fail due to parallel execution
# Solution: Run tests serially
cargo test --workspace -- --test-threads=1

# Issue: Integration tests timeout
# Solution: Increase timeout
RUST_TEST_THREADS=1 cargo test --workspace -- --nocapture

# Issue: Database locked
# Solution: Clean test databases
rm -rf nexus-core/test_data
```

#### 3. Server Issues

```bash
# Issue: "Address already in use"
# Solution: Kill existing server
pkill -f nexus-server
# Or on Windows:
taskkill /F /IM nexus-server.exe

# Issue: Server won't start
# Solution: Check data directory permissions
ls -la data/
chmod -R 755 data/

# Issue: Out of memory
# Solution: Reduce page cache size
# Set NEXUS_PAGE_CACHE_SIZE=512 (MB)
```

#### 4. SDK Test Failures

```bash
# Issue: SDK tests fail with 404
# Solution: Check endpoint is /cypher not /query

# Issue: SDK deserialization error
# Solution: Check row format is [[value1, value2]] not [{"col": value}]

# Issue: Server not responding
# Solution: Ensure server is running
curl http://localhost:15474/health
```

### Debugging Techniques

#### Enable Debug Logging

```bash
# Set log level
export RUST_LOG=nexus_core=debug,nexus_server=debug

# Run server with debug logs
RUST_LOG=debug ./target/release/nexus-server

# Filter specific module
RUST_LOG=nexus_core::executor=debug ./target/release/nexus-server
```

#### Profile Performance

```bash
# Build with debug symbols
cargo build --release --package nexus-core

# Run with profiler (Linux)
perf record ./target/release/nexus-server
perf report

# Run with profiler (Windows)
# Use Windows Performance Analyzer
```

#### Inspect Database

```bash
# Query statistics
curl http://localhost:15474/stats

# Check database list
curl http://localhost:15474/databases

# Test connectivity
curl http://localhost:15474/health
```

### Getting Help

- **Issues**: https://github.com/hivellm/nexus/issues
- **Discussions**: https://github.com/hivellm/nexus/discussions
- **Email**: team@hivellm.org
- **Documentation**: docs/ directory in repository

---

## Additional Resources

### Documentation

- **Architecture**: `docs/ARCHITECTURE.md` - Complete system design
- **Roadmap**: `docs/ROADMAP.md` - Development phases and milestones
- **User Guide**: `docs/USER_GUIDE.md` - Usage documentation
- **Authentication**: `docs/AUTHENTICATION.md` - Security setup
- **Performance**: `docs/PERFORMANCE.md` - Benchmarks and tuning

### Specifications

- **Storage Format**: `docs/specs/storage-format.md` - Record store layouts
- **Cypher Subset**: `docs/specs/cypher-subset.md` - Supported syntax
- **Page Cache**: `docs/specs/page-cache.md` - Memory management
- **WAL & MVCC**: `docs/specs/wal-mvcc.md` - Transaction model
- **KNN Integration**: `docs/specs/knn-integration.md` - Vector search
- **API Protocols**: `docs/specs/api-protocols.md` - REST, MCP, UMICP

### Testing Reports

- **SDK Coverage**: `sdks/TEST_COVERAGE_REPORT.md` - All SDK test results
- **SDK Results**: `sdks/SDK_TEST_RESULTS_FINAL.md` - Final SDK test summary
- **Test Summary**: `tests/TEST_SUMMARY.md` - Integration test results

---

## Quick Wins for New Contributors

### Easy First Tasks

1. **Add new Cypher function** - See `nexus-core/src/executor/mod.rs`
2. **Improve error messages** - Make errors more descriptive
3. **Add examples** - Create example queries in `examples/`
4. **Fix typos in docs** - Documentation improvements always welcome
5. **Add SDK examples** - Show how to use SDKs in different scenarios

### Medium Difficulty Tasks

1. **Optimize query planner** - Improve cost estimates
2. **Add new index type** - Implement specialized index
3. **Improve test coverage** - Add edge case tests
4. **Performance tuning** - Profile and optimize hot paths
5. **Add monitoring metrics** - Expose more statistics

### Advanced Tasks

1. **Implement distributed features** - See V2 roadmap
2. **Advanced query optimization** - JIT compilation improvements
3. **New protocol support** - Additional API protocols
4. **Replication enhancements** - Improve fault tolerance
5. **GUI improvements** - Enhance desktop application

---

**Last Updated**: 2025-11-28
**Maintained By**: HiveLLM Contributors
**License**: Apache 2.0

For the latest updates, always check:
- `README.md` - Project overview
- `CONTRIBUTING.md` - Contribution guidelines
- `CHANGELOG.md` - Recent changes
- `docs/ROADMAP.md` - Future plans
