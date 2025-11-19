# Critical Storage Engine: Architecture Design

## Overview

This document outlines the architecture for a custom graph-native storage engine designed to achieve Neo4j-level performance by eliminating LMDB overhead and optimizing for relationship-centric workloads.

## Core Design Principles

### 1. Relationship-Centric Storage
**Problem**: Current LMDB-based storage treats relationships as secondary entities, requiring expensive traversals.

**Solution**: Design storage around relationship access patterns, making relationships first-class citizens.

```rust
// Current: Relationship as secondary entity
struct RelationshipRecord {
    id: u64,
    from: u64,
    to: u64,
    type_id: u32,
    // Linked to nodes via pointers
}

// Target: Relationship as primary entity with direct access
struct RelationshipSegment {
    type_id: u32,
    relationships: Vec<RelationshipData>, // Contiguous storage
    adjacency_outgoing: HashMap<NodeId, Range>, // Direct access to node's relationships
    adjacency_incoming: HashMap<NodeId, Range>, // Direct access to incoming relationships
}
```

### 2. Single Large Memory Map
**Problem**: Multiple mmap files cause cache thrashing and complex memory management.

**Solution**: Single large memory-mapped file with internal segmentation.

```rust
struct GraphStorage {
    mmap: MmapMut,
    layout: StorageLayout,
}

struct StorageLayout {
    node_segment: Range<u64>,
    relationship_segments: HashMap<TypeId, Range<u64>>,
    property_segment: Range<u64>,
    metadata_segment: Range<u64>,
}
```

### 3. Type-Based Segmentation
**Problem**: All relationships in single structure causes poor locality.

**Solution**: Separate segments per relationship type for optimal cache performance.

```rust
// Relationships grouped by type for locality
graph_store/
├── nodes.bin          // All nodes contiguous
├── rels_friends.bin   // All FRIEND relationships
├── rels_follows.bin   // All FOLLOWS relationships
├── rels_likes.bin     // All LIKES relationships
└── properties.bin     // Properties with references
```

## Detailed Architecture

### Storage Format Specification

#### 1. Node Storage Format
```
Node Record (64 bytes):
+-------------------+-------------------+-------------------+-------------------+
| id (8)           | first_rel_ptr (8) | prop_ptr (8)      | flags (4)         |
+-------------------+-------------------+-------------------+-------------------+
| label_id (4)     | reserved (4)      | created_at (8)    | updated_at (8)    |
+-------------------+-------------------+-------------------+-------------------+
| data (16)        | padding (8)       | checksum (8)      | magic (4)         |
+-------------------+-------------------+-------------------+-------------------+

Total: 64 bytes (cache line aligned)
```

#### 2. Relationship Storage Format
```
Relationship Record (32 bytes):
+-------------------+-------------------+-------------------+-------------------+
| id (8)           | from_node (8)     | to_node (8)       | type_id (4)       |
+-------------------+-------------------+-------------------+-------------------+
| prop_ptr (4)     | flags (2)         | checksum (2)      | magic (4)         |
+-------------------+-------------------+-------------------+-------------------+

Total: 32 bytes (optimal for relationship-heavy graphs)
```

#### 3. Adjacency List Format
```
Adjacency Header (16 bytes):
+-------------------+-------------------+-------------------+-------------------+
| node_id (8)      | count (4)         | type_id (4)       |
+-------------------+-------------------+-------------------+-------------------+

Adjacency Entries (8 bytes each):
+-------------------+-------------------+
| rel_id (8)       |
+-------------------+-------------------+
```

### Memory Layout Strategy

#### 1. File Structure
```
graph.db (single file):
+-------------------+
| Header (4KB)     |
+-------------------+
| Node Segment      |
| (pre-allocated)   |
+-------------------+
| Relationship      |
| Segments          |
| (by type)         |
+-------------------+
| Property Segment  |
+-------------------+
| Free Space        |
+-------------------+
```

#### 2. Growth Strategy
- **Pre-allocation**: Large chunks to minimize growth operations
- **Type-based growth**: Relationship segments grow independently
- **Lazy initialization**: Segments created on first relationship of type

#### 3. Memory Mapping
- **Single mmap**: Entire file memory-mapped
- **Internal offsets**: Segments accessed via calculated offsets
- **Transparent growth**: File growth without remapping

### Compression Strategies

#### 1. Adjacency List Compression
```rust
enum CompressionType {
    None,           // No compression (< 10 relationships)
    VarInt,         // Variable-length encoding (10-1000 relationships)
    Delta,          // Delta encoding for sorted IDs
    Dictionary,     // Dictionary compression for dense graphs
}
```

#### 2. Relationship Property Compression
- **Inline storage**: Small properties stored with relationship
- **Reference storage**: Large properties in separate segment
- **Type-aware compression**: Different algorithms per property type

### I/O Optimization Strategies

#### 1. Direct I/O Implementation
```rust
struct DirectFile {
    file: File,
    alignment: usize,  // SSD block size alignment
}

impl DirectFile {
    fn write_aligned(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        // Ensure offset and size are block-aligned
        // Use O_DIRECT for direct DMA transfers
    }
}
```

#### 2. Write-Ahead Logging (WAL)
- **Group commit**: Batch multiple operations
- **Parallel writes**: WAL and data files written concurrently
- **Checksums**: Data integrity verification

#### 3. Prefetching Strategy
```rust
struct Prefetcher {
    ahead_pages: usize,      // Pages to prefetch ahead
    threshold: usize,        // Trigger prefetch after N sequential reads
}

impl Prefetcher {
    fn prefetch_if_sequential(&mut self, offset: u64) {
        // Detect sequential access patterns
        // Prefetch upcoming pages
    }
}
```

## Implementation Roadmap

### Phase 1: Core Engine (Weeks 1-4)
1. **Basic Storage Engine**
   - Single file memory mapping
   - Basic CRUD operations
   - Transaction support

2. **Node Storage**
   - Contiguous node storage
   - Basic property support
   - Node relationship pointers

3. **Relationship Storage**
   - Type-based segmentation
   - Basic adjacency lists
   - Relationship property support

### Phase 2: Optimization (Weeks 5-8)
1. **Compression Implementation**
   - Adjacency list compression
   - Property compression
   - Memory usage optimization

2. **I/O Optimization**
   - Direct I/O implementation
   - SSD-aware allocation
   - Prefetching strategies

3. **Indexing Enhancement**
   - Skip lists for large lists
   - Bloom filters for existence checks
   - Advanced traversal optimizations

### Phase 3: Production Readiness (Weeks 9-12)
1. **Reliability**
   - Crash recovery
   - Data consistency checks
   - Backup and restore

2. **Performance Tuning**
   - Memory tuning
   - I/O tuning
   - Concurrency optimization

3. **Integration**
   - Executor integration
   - Migration tools
   - Monitoring and metrics

## Performance Projections

### Expected Improvements

| Operation | Current (ms) | Target (ms) | Improvement |
|-----------|-------------|-------------|-------------|
| CREATE Relationship | 57.33 | 5.0 | 91% |
| Single Hop Relationship | 3.90 | 1.0 | 74% |
| Relationship with WHERE | 5.26 | 1.5 | 71% |
| Count Relationships | 2.05 | 1.0 | 51% |

### Memory Efficiency Targets

- **Nodes**: 64 bytes per node (current: ~100 bytes)
- **Relationships**: 32 bytes per relationship (current: ~80 bytes)
- **Adjacency Lists**: 50% compression ratio
- **Total Overhead**: <200MB for 1M relationships

### I/O Performance Targets

- **Sequential Reads**: 80% of theoretical SSD bandwidth
- **Random Reads**: 50% improvement over LMDB
- **Writes**: 70% improvement with group commit
- **Cache Hit Rate**: >90% for hot data

## Risk Analysis

### Technical Risks

1. **Data Corruption**
   - **Mitigation**: Comprehensive checksums, transaction rollback
   - **Testing**: Corruption simulation, recovery validation

2. **Performance Regression**
   - **Mitigation**: Continuous benchmarking, performance gates
   - **Fallback**: LMDB compatibility maintained

3. **Memory Management Complexity**
   - **Mitigation**: Incremental implementation, extensive testing
   - **Tools**: Memory profiling, leak detection

### Operational Risks

1. **Migration Complexity**
   - **Mitigation**: Automated migration tools, data validation
   - **Testing**: Migration testing with large datasets

2. **Debugging Difficulty**
   - **Mitigation**: Comprehensive logging, monitoring tools
   - **Tools**: Custom profiling tools, debug interfaces

## Success Metrics

### Performance Metrics
- [ ] CREATE Relationship: ≤ 5.0ms average
- [ ] Relationship queries: ≤ 1.5ms average
- [ ] Memory usage: ≤ 200MB per 1M relationships
- [ ] I/O overhead: ≤ 50% of LMDB

### Quality Metrics
- [ ] Data consistency: 100% accuracy
- [ ] Crash recovery: < 5 minutes for 1M relationships
- [ ] Memory leaks: Zero detected
- [ ] Performance regression: < 5%

### Operational Metrics
- [ ] Migration time: < 1 hour per 1M relationships
- [ ] Monitoring coverage: 100% of operations
- [ ] Alert accuracy: > 95% true positive rate

## Conclusion

The custom storage engine represents a fundamental redesign to achieve Neo4j performance parity. By optimizing for relationship-centric workloads and eliminating LMDB overhead, we project 80-90% performance improvement in relationship operations while maintaining data consistency and reliability.

The modular architecture allows for incremental implementation and testing, with clear milestones and rollback capabilities to mitigate risks.
