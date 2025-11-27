# Critical Storage Engine Specification

## Purpose

This specification defines the requirements for a custom graph-native storage engine that replaces LMDB for relationship operations, achieving Neo4j performance parity through fundamental architectural changes.

## Requirements

### ADDED Requirements - Storage Engine Core

#### Requirement: Graph-Native Storage Format
The system SHALL implement a storage format optimized for graph data access patterns, replacing the current LMDB-based approach.

##### Scenario: Unified Graph File Structure
Given a graph database with nodes and relationships
When data is stored using the custom engine
Then a single file SHALL contain all graph data
And data SHALL be organized in contiguous segments
And segments SHALL be memory-mapped for efficient access
And file growth SHALL be pre-allocated to minimize remapping

##### Scenario: Relationship-Centric Organization
Given relationships between nodes
When relationships are stored
Then relationships SHALL be grouped by type for locality
And adjacency lists SHALL provide O(1) access to node relationships
And relationship properties SHALL be stored contiguously
And traversal operations SHALL minimize random I/O

#### Requirement: Memory-Mapped Architecture
The system SHALL use memory-mapped I/O for optimal performance and memory efficiency.

##### Scenario: Single Large Memory Map
Given a graph database file
When the storage engine initializes
Then the entire file SHALL be memory-mapped once
And internal offsets SHALL be used for segment access
And memory mapping SHALL be transparent to users
And remapping SHALL only occur during file growth

##### Scenario: Transparent File Growth
Given a growing database
When the file needs to grow
Then growth SHALL be pre-allocated in large chunks
And memory mapping SHALL be updated transparently
And existing pointers SHALL remain valid
And growth operations SHALL be optimized for SSD performance

### ADDED Requirements - Relationship Storage

#### Requirement: Type-Based Relationship Segmentation
The system SHALL organize relationships by type for optimal access patterns.

##### Scenario: Relationship Type Segregation
Given relationships of different types
When relationships are stored
Then each type SHALL have its own contiguous segment
And segments SHALL be sized appropriately for the type
And cross-type operations SHALL be minimized
And cache locality SHALL be maximized within types

##### Scenario: Adjacency List Optimization
Given a node's relationships
When adjacency information is accessed
Then outgoing relationships SHALL be stored contiguously
And incoming relationships SHALL be stored contiguously
And lists SHALL be compressed for memory efficiency
And access SHALL be O(1) for existence checks

#### Requirement: Relationship Compression
The system SHALL implement compression algorithms optimized for relationship data.

##### Scenario: Adjacency List Compression
Given large adjacency lists
When lists are stored
Then variable-length encoding SHALL be used for IDs
And delta encoding SHALL be applied to sorted relationships
And compression SHALL achieve ≥50% space savings
And decompression SHALL be fast enough for query performance

##### Scenario: Property Compression
Given relationship properties
When properties are stored
Then type-aware compression SHALL be applied
And small properties SHALL be stored inline
And large properties SHALL be referenced externally
And compression SHALL not impact query performance

### ADDED Requirements - Direct I/O Optimization

#### Requirement: Direct I/O Implementation
The system SHALL use direct I/O to bypass OS caching overhead for data files.

##### Scenario: O_DIRECT Usage
Given SSD storage devices
When data is written to disk
Then O_DIRECT SHALL be used for data files
And OS page cache SHALL be bypassed
And direct DMA transfers SHALL be utilized
And memory pressure SHALL be reduced

##### Scenario: SSD-Aware Allocation
Given SSD block characteristics
When data is allocated and written
Then allocations SHALL be aligned to SSD block boundaries
And write patterns SHALL favor sequential access
And prefetching SHALL be implemented for sequential reads
And write amplification SHALL be minimized

#### Requirement: NVMe Optimization
The system SHALL utilize NVMe-specific features when available.

##### Scenario: NVMe Feature Detection
Given NVMe storage devices
When the storage engine initializes
Then NVMe capabilities SHALL be detected
And appropriate optimizations SHALL be enabled
And performance SHALL scale with NVMe capabilities
And graceful degradation SHALL occur on non-NVMe devices

##### Scenario: Parallel I/O Channels
Given multi-queue NVMe devices
When I/O operations are performed
Then multiple queues SHALL be utilized
And operations SHALL be distributed across queues
And queue depths SHALL be optimized
And throughput SHALL be maximized

### ADDED Requirements - Advanced Indexing

#### Requirement: Skip Lists for Large Adjacency Lists
The system SHALL implement skip lists for efficient traversal of large relationship lists.

##### Scenario: Skip List Construction
Given large adjacency lists (>1000 relationships)
When skip lists are built
Then hierarchical structure SHALL enable fast access
And O(log n) traversal SHALL be achieved
And memory overhead SHALL be reasonable
And construction SHALL be incremental

##### Scenario: Skip List Query Performance
Given queries requiring relationship traversal
When skip lists are used
Then range queries SHALL be O(log n) + O(k)
And existence checks SHALL be fast
And memory usage SHALL be optimized
And cache performance SHALL be maintained

#### Requirement: Bloom Filters for Existence Checks
The system SHALL use bloom filters to accelerate relationship existence queries.

##### Scenario: Bloom Filter Integration
Given relationship existence queries
When bloom filters are available
Then false positives SHALL be acceptable
And true negatives SHALL eliminate unnecessary I/O
And filter size SHALL be optimized for memory
And construction SHALL be fast

##### Scenario: Bloom Filter Accuracy
Given bloom filter implementations
When filters are tuned
Then false positive rate SHALL be ≤1%
And memory usage SHALL be reasonable
And construction time SHALL be acceptable
And query performance SHALL improve

### ADDED Requirements - Performance Guarantees

#### Requirement: Storage Performance Targets
The custom storage engine SHALL meet specific performance targets compared to LMDB.

##### Scenario: Relationship Creation Performance
Given relationship creation operations
When measured against current implementation
Then CREATE operations SHALL be ≤5.0ms average
And improvement SHALL be ≥90% over LMDB
And CPU utilization SHALL be optimized
And memory allocation SHALL be efficient

##### Scenario: Relationship Query Performance
Given relationship traversal operations
When measured against current implementation
Then single-hop queries SHALL be ≤1.0ms average
And complex traversals SHALL be ≤2.0ms average
And I/O operations SHALL be minimized
And cache hit rates SHALL be optimized

#### Requirement: Memory Efficiency Targets
The storage engine SHALL meet memory efficiency requirements.

##### Scenario: Memory Usage Optimization
Given large graph datasets
When memory usage is measured
Then per-relationship overhead SHALL be ≤50 bytes
And adjacency list compression SHALL save ≥50% space
And memory-mapped regions SHALL be managed efficiently
And fragmentation SHALL be minimized

##### Scenario: Scalability Testing
Given growing datasets
When scalability is tested
Then performance SHALL degrade gracefully
And memory usage SHALL scale linearly
And I/O patterns SHALL remain efficient
And system stability SHALL be maintained

### ADDED Requirements - Reliability and Consistency

#### Requirement: Data Consistency Guarantees
The storage engine SHALL maintain data consistency across all operations.

##### Scenario: Transaction Atomicity
Given multi-operation transactions
When transactions are executed
Then either all operations SHALL succeed
Or all operations SHALL be rolled back
And partial states SHALL not be visible
And consistency SHALL be maintained

##### Scenario: Crash Recovery
Given system crashes during operations
When the system restarts
Then data SHALL be recovered to a consistent state
And incomplete transactions SHALL be rolled back
And corruption SHALL be detected and repaired
And recovery time SHALL be reasonable

#### Requirement: Migration Compatibility
The storage engine SHALL support migration from LMDB-based storage.

##### Scenario: Data Migration
Given existing LMDB-based data
When migration is performed
Then all data SHALL be migrated accurately
And relationships SHALL be preserved
And properties SHALL be maintained
And consistency SHALL be verified

##### Scenario: Fallback Capability
Given migration issues
When problems are detected
Then fallback to LMDB SHALL be possible
And data integrity SHALL be maintained
And performance degradation SHALL be graceful
And migration SHALL be resumable

## Implementation Notes

### Storage Engine Architecture

```rust
// Core storage engine structure
pub struct GraphStorageEngine {
    mmap: MmapMut,
    layout: StorageLayout,
    compressor: RelationshipCompressor,
    prefetcher: AccessPatternPrefetcher,
}

// Storage layout for single file
struct StorageLayout {
    header: Range<u64>,
    nodes: Range<u64>,
    relationships: HashMap<TypeId, RelationshipSegment>,
    properties: Range<u64>,
    free_space: Range<u64>,
}

// Relationship segment with adjacency
struct RelationshipSegment {
    data_range: Range<u64>,
    adjacency_outgoing: AdjacencyIndex,
    adjacency_incoming: AdjacencyIndex,
    compression_type: CompressionType,
}
```

### Compression Implementation

```rust
enum CompressionType {
    None,           // < 10 relationships
    VarInt,         // 10-1000 relationships
    Delta,          // Sorted relationship IDs
    Dictionary,     // Dense relationship patterns
}

struct RelationshipCompressor {
    varint_encoder: VarIntEncoder,
    delta_encoder: DeltaEncoder,
    dictionary_builder: DictionaryBuilder,
}
```

### Direct I/O Implementation

```rust
struct DirectFile {
    file: File,
    block_size: usize,
    alignment: usize,
}

impl DirectFile {
    fn write_aligned(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        // Ensure alignment and use O_DIRECT
        // Handle partial writes and retries
    }

    fn read_aligned(&mut self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        // Direct DMA reads with alignment
        // Minimize system call overhead
    }
}
```

## Testing Requirements

### Performance Testing
- CREATE relationship operations: target ≤5.0ms average
- Relationship traversal: target ≤1.5ms average
- Memory usage: target ≤200MB for 1M relationships
- I/O throughput: target ≥80% of SSD bandwidth

### Correctness Testing
- Data consistency across crashes
- Transaction atomicity verification
- Migration accuracy validation
- Compression/decompression roundtrips

### Scalability Testing
- Performance with 10M+ relationships
- Memory usage scaling tests
- Concurrent access pattern testing
- Long-running stability tests

## Success Criteria

### Phase 1 Success Criteria (Weeks 1-4)
- [ ] Basic storage engine functional
- [ ] Relationship operations working
- [ ] 50% performance improvement demonstrated
- [ ] Data consistency maintained

### Phase 2 Success Criteria (Weeks 5-8)
- [ ] Compression algorithms implemented
- [ ] Direct I/O optimizations complete
- [ ] 70% performance improvement achieved
- [ ] Memory efficiency targets met

### Phase 3 Success Criteria (Weeks 9-12)
- [ ] Full feature set implemented
- [ ] Migration tools complete
- [ ] 80-90% performance improvement achieved
- [ ] Production readiness verified

### Final Success Criteria
- [ ] **50% of Neo4j performance** achieved
- [ ] All performance targets met
- [ ] Data consistency guaranteed
- [ ] Migration path established
- [ ] Production deployment ready
