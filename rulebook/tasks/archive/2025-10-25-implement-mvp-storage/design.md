# Design Document - MVP Storage Layer

## Context

Nexus is implementing its foundational storage layer based on Neo4j-inspired architecture. This is a **green-field implementation** with complete architectural documentation already in place.

### Background
- Complete architecture documented in `docs/ARCHITECTURE.md`
- Storage format specified in `docs/specs/storage-format.md`
- MVCC and WAL specified in `docs/specs/wal-mvcc.md`
- Page cache specified in `docs/specs/page-cache.md`

### Constraints
- Must use Rust edition 2024 (nightly 1.85+)
- Must achieve 95%+ test coverage
- Must use memmap2 for memory-mapped files
- Must use heed (LMDB) for catalog
- Single-writer model for MVP (simplifies implementation)

### Stakeholders
- HiveLLM Team (core developers)
- Future users needing graph + vector search

## Goals / Non-Goals

### Goals
- ✅ Implement catalog with LMDB (bidirectional mappings)
- ✅ Implement record stores (nodes, rels, props, strings)
- ✅ Implement page cache with Clock eviction
- ✅ Implement WAL with crash recovery
- ✅ Implement basic MVCC (epoch-based, single-writer)
- ✅ Achieve 95%+ test coverage
- ✅ Meet performance target: 100K+ point reads/sec

### Non-Goals
- ❌ Query executor (deferred to Phase 1.4)
- ❌ Indexes (deferred to Phase 1.3)
- ❌ Multi-writer concurrency (V1)
- ❌ Distributed replication (V1)
- ❌ Advanced eviction policies (2Q, TinyLFU) - use simple Clock for MVP
- ❌ Compression or encryption (V1)

## Decisions

### Decision 1: Use LMDB (via heed) for Catalog

**Choice**: heed (Rust wrapper for LMDB)

**Rationale**:
- Battle-tested embedded KV store (used in production systems)
- ACID guarantees (critical for catalog integrity)
- Zero-copy reads (performance)
- LMDB is proven (used by Neo4j internally)

**Alternatives Considered**:
- RocksDB: More complex, overkill for catalog (catalog is small, <1GB)
- SQLite: Good but adds SQL overhead (we only need KV)
- Custom B-tree: Too much work, not worth it

**Trade-offs**:
- Pro: Reliability, ACID, zero-copy
- Con: Single-writer (acceptable for catalog, low write volume)

---

### Decision 2: Fixed-Size Records (32B nodes, 48B rels)

**Choice**: Fixed-size records matching Neo4j approach

**Rationale**:
- O(1) random access via direct offset calculation
- Predictable memory layout (no fragmentation)
- Simple implementation (no variable-length record management)
- Proven architecture (Neo4j uses this successfully)

**Alternatives Considered**:
- Variable-size records: More compact but complex offset management
- Column-oriented: Better for analytics but worse for traversal
- Document-based (JSON): Flexible but much slower for graph ops

**Trade-offs**:
- Pro: Simplicity, performance, predictability
- Con: Some wasted space for small entities (acceptable)

---

### Decision 3: Clock Eviction for Page Cache (MVP)

**Choice**: Simple Clock algorithm (second-chance)

**Rationale**:
- Simple to implement (~100 lines)
- Good enough for MVP (50-70% hit rate)
- Can upgrade to 2Q/TinyLFU in V1 without API changes

**Alternatives Considered**:
- LRU: Worse for sequential scans (common in graph queries)
- 2Q: Better hit rate but more complex (save for V1)
- TinyLFU: Best hit rate but requires Count-Min Sketch (V1)

**Trade-offs**:
- Pro: Simple, fast implementation
- Con: Lower hit rate than advanced policies (acceptable for MVP)

---

### Decision 4: Single-Writer MVCC (MVP)

**Choice**: One write transaction at a time (queue-based)

**Rationale**:
- Simplifies implementation (no deadlock detection)
- Acceptable for read-heavy workloads (target use case)
- Can add multi-writer in V1 without changing storage format

**Alternatives Considered**:
- Multi-writer with 2PL: Complex, requires deadlock detection
- Optimistic concurrency: Requires conflict resolution (complex)

**Trade-offs**:
- Pro: Simple, correct, no deadlocks
- Con: Lower write throughput (acceptable for MVP)

---

### Decision 5: Append-Only Architecture

**Choice**: Immutable records until compaction

**Rationale**:
- MVCC-friendly (old versions stay until GC)
- Write performance (sequential appends)
- Crash recovery (no partial writes)
- Matches WAL design (append-only log)

**Alternatives Considered**:
- In-place updates: Faster reads but complicates MVCC
- Copy-on-write: Similar to append-only but more complex

**Trade-offs**:
- Pro: Durability, MVCC simplicity, write performance
- Con: Needs periodic compaction (acceptable, run during low load)

## Risks / Trade-offs

### Risk 1: Page Cache Complexity
**Risk**: Page cache bugs could corrupt data  
**Mitigation**: 
- Comprehensive tests (pin/unpin, eviction, concurrency)
- xxHash3 checksums on all pages
- Fuzzing tests for edge cases

### Risk 2: WAL Recovery Edge Cases
**Risk**: WAL recovery might miss edge cases (partial writes, corruption)  
**Mitigation**:
- CRC32 on every WAL entry
- Extensive crash simulation tests
- Reference Neo4j/PostgreSQL WAL recovery patterns

### Risk 3: MVCC Garbage Collection
**Risk**: Old versions accumulate without GC  
**Mitigation**:
- Periodic GC based on min active snapshot
- Configurable retention (default: 1 hour)
- Monitor version count per entity

### Risk 4: Performance Targets
**Risk**: May not achieve 100K+ reads/sec initially  
**Mitigation**:
- Benchmark early and often
- Profile hot paths
- Optimize after correctness is proven

## Migration Plan

N/A - This is greenfield implementation (no existing data to migrate)

## Testing Strategy

### Unit Tests (Per Module)
- Catalog: LMDB operations, concurrent access
- Storage: Record CRUD, linked lists, property chains
- Page Cache: Eviction, pin/unpin, flush
- WAL: Append, checkpoint, recovery
- Transaction: Begin, commit, abort, MVCC visibility

### Integration Tests
- End-to-end: Create node → commit → read node
- Crash recovery: Write → crash → recover → verify
- Concurrency: Multiple readers + single writer
- Performance: Measure throughput and latency

### Coverage Target
95%+ coverage for all modules (verified with cargo llvm-cov)

## Performance Benchmarks

Target metrics to validate:
- Point read: 100,000+ ops/sec
- Point write: 10,000+ ops/sec (single-writer)
- Bulk insert: 100,000+ nodes/sec
- Page cache hit rate: 90%+ (with sufficient capacity)

## Open Questions

1. **Page size**: 4KB vs 8KB?
   - **Resolution**: Use 8KB (better for large properties, matches common SSD page size)

2. **WAL segment size**: When to rotate WAL files?
   - **Resolution**: 1GB max size or 5-minute checkpoint (whichever comes first)

3. **Compaction strategy**: Online or offline?
   - **Resolution**: Offline for MVP (run manually or during maintenance window)

4. **Error handling**: Panic vs graceful degradation?
   - **Resolution**: Return errors for recoverable issues, panic only for corruption (fail-fast)

