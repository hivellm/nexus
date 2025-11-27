# Executor Concurrent Execution Specification

## Purpose

This specification defines the requirements for concurrent query execution in the Nexus executor. The current implementation uses a global executor lock that serializes all queries, resulting in single-threaded execution and 60% lower throughput compared to Neo4j. This specification establishes the architecture and requirements for removing this bottleneck and enabling true concurrent query execution across multiple CPU cores.

## Requirements

### Requirement: Concurrent Query Execution

The system SHALL support concurrent execution of multiple queries without global serialization locks.

#### Scenario: Multiple Read Queries Execute Concurrently

Given 10 concurrent read-only queries submitted to the system
When all queries are executed
Then all queries SHALL execute in parallel without waiting for locks
And total execution time SHALL be approximately equal to single query time
And CPU utilization SHALL scale with available cores (target: 70%+ on 8-core machine)
And throughput SHALL be at least 500 queries per second

#### Scenario: Mixed Read and Write Queries

Given 5 concurrent read queries and 5 concurrent write queries
When all queries are executed
Then read queries SHALL NOT block other read queries
And write queries SHALL NOT block read queries to different data
And only conflicting writes SHALL block each other
And overall throughput SHALL be at least 400 queries per second

### Requirement: Thread-Safe Executor

The executor SHALL be safely cloneable or usable across multiple threads without data races.

#### Scenario: Executor Cloning for Concurrent Use

Given a configured executor instance
When the executor is cloned for use in multiple threads
Then each clone SHALL maintain references to shared immutable state (catalog, storage)
And each clone SHALL have independent per-query state (variables, context)
And operations on one clone SHALL NOT interfere with operations on another
And the Rust compiler SHALL verify thread-safety (Send + Sync bounds)

#### Scenario: Shared State Access

Given multiple executor instances accessing shared catalog
When concurrent queries look up labels or types
Then catalog access SHALL use fine-grained read-write locks
And read operations SHALL NOT block other read operations
And write operations SHALL acquire exclusive locks only for modified data
And no deadlocks SHALL occur under concurrent access

### Requirement: MVCC Snapshot Isolation

The system SHALL implement Multi-Version Concurrency Control (MVCC) for read isolation.

#### Scenario: Read Query with Consistent Snapshot

Given a read query starting at time T1
And concurrent write operations modifying data
When the read query executes
Then it SHALL see a consistent snapshot of data as of time T1
And it SHALL NOT see uncommitted changes from concurrent writes
And it SHALL NOT see changes committed after T1
And read operations SHALL NOT acquire locks on data

#### Scenario: Write Query with Version Management

Given a write query modifying a node at time T1
And the node has version V1
When the write commits at time T2
Then a new version V2 SHALL be created
And concurrent reads SHALL continue seeing V1 until T2
And subsequent reads SHALL see V2 after T2
And old versions SHALL be garbage collected when no longer needed

### Requirement: Thread Pool for Query Execution

The system SHALL use a thread pool to execute queries concurrently across available CPU cores.

#### Scenario: Thread Pool Initialization

Given a system with N CPU cores
When the executor initializes
Then a thread pool SHALL be created with N worker threads
And worker threads SHALL remain alive for the lifetime of the system
And the thread pool SHALL have a bounded task queue (size: 1000)
And rejected tasks SHALL return an error to the client

#### Scenario: Query Distribution to Workers

Given a thread pool with 8 worker threads
And 100 queries submitted sequentially
When queries are dispatched to the thread pool
Then queries SHALL be distributed across all 8 workers
And each worker SHALL execute queries from its queue
And load balancing SHALL distribute work evenly
And completed queries SHALL return results to the client

### Requirement: Per-Query Execution Context

Each query execution SHALL have isolated state that does not interfere with concurrent queries.

#### Scenario: Independent Query Variables

Given two concurrent queries Q1 and Q2
And Q1 binds variable `x` to value 10
And Q2 binds variable `x` to value 20
When both queries execute concurrently
Then Q1 SHALL see `x = 10` throughout its execution
And Q2 SHALL see `x = 20` throughout its execution
And variable bindings SHALL NOT leak between queries

#### Scenario: Isolated Result Sets

Given two concurrent queries returning large result sets
When both queries execute and build result sets
Then each query SHALL have its own result set storage
And result set memory SHALL be allocated per-query
And one query SHALL NOT corrupt another query's results
And result sets SHALL be properly cleaned up after query completion

### Requirement: Concurrent Storage Access

The storage layer SHALL support concurrent reads and isolated writes.

#### Scenario: Concurrent Node Reads

Given 10 concurrent queries reading different nodes
When all queries execute simultaneously
Then all reads SHALL proceed in parallel
And no read operations SHALL block other reads
And read latency SHALL NOT increase with concurrency
And no data corruption SHALL occur

#### Scenario: Concurrent Writes with Locking

Given 2 concurrent queries writing to the same node
When both queries attempt to modify the node
Then the first query SHALL acquire a write lock
And the second query SHALL wait for the lock to be released
And after the first commits, the second SHALL acquire the lock
And the final state SHALL reflect both writes in correct order

### Requirement: Performance Guarantees

The concurrent execution system SHALL meet specified performance targets.

#### Scenario: Throughput Target

Given a benchmark running 1000 queries sequentially
When measured on an 8-core machine
Then throughput SHALL be at least 500 queries per second
And average query latency SHALL be less than 8ms
And p95 query latency SHALL be less than 15ms
And p99 query latency SHALL be less than 25ms

#### Scenario: CPU Utilization Target

Given a benchmark running continuous concurrent queries
When monitored with system profiling tools
Then CPU utilization SHALL be at least 70% on all cores
And no single core SHALL be more than 90% utilized
And thread pool workers SHALL be balanced across cores
And idle time SHALL be less than 30% per core

#### Scenario: Scalability with Core Count

Given the same workload on machines with different core counts
When tested on 4-core, 8-core, and 16-core machines
Then throughput SHALL scale approximately linearly with core count
And 16-core throughput SHALL be at least 1.8x the 8-core throughput
And 8-core throughput SHALL be at least 1.8x the 4-core throughput
And overhead per core SHALL be less than 10%

### Requirement: No Breaking Changes

The concurrent execution changes SHALL NOT break existing APIs or query behavior.

#### Scenario: Query Results Unchanged

Given existing queries Q1, Q2, ... QN
And their expected results R1, R2, ... RN
When queries are executed with concurrent execution enabled
Then Q1 SHALL still return R1
And Q2 SHALL still return R2
And all queries SHALL return identical results to the serial implementation

#### Scenario: API Compatibility

Given the existing HTTP API for Cypher queries
When concurrent execution is enabled
Then the API request format SHALL remain unchanged
And the API response format SHALL remain unchanged
And clients SHALL NOT need any modifications
And no new required parameters SHALL be added

## Implementation Notes

### Thread Pool Options

Two viable options for thread pool implementation:

1. **Rayon** - Data parallelism library
   - Pros: Work stealing, battle-tested, simple API
   - Cons: Less control over thread lifecycle
   
2. **tokio::task::spawn_blocking** - Tokio blocking task pool
   - Pros: Integrates with existing async code, auto-scaling
   - Cons: Not optimized for long-running tasks

**Recommendation**: Start with `tokio::task::spawn_blocking` for easier integration, can switch to Rayon if needed.

### Executor Architecture

```rust
// Option 1: Clonable Executor (RECOMMENDED)
#[derive(Clone)]
struct Executor {
    storage: Arc<RwLock<Storage>>,  // Shared
    catalog: Arc<Catalog>,           // Shared, lock-free
    // No per-query state here
}

impl Executor {
    fn execute(&self, query: Query) -> Result<ResultSet> {
        let ctx = ExecutionContext::new(query.params);
        // All per-query state in ctx
        self.execute_with_context(query, ctx)
    }
}

// Option 2: Actor Model
struct ExecutorActor {
    executor: Executor,
}

impl Handler<QueryMessage> for ExecutorActor {
    fn handle(&mut self, msg: QueryMessage) -> Result<ResultSet> {
        self.executor.execute(msg.query)
    }
}
```

### MVCC Implementation

```rust
struct Version {
    epoch: u64,       // Transaction start time
    data: NodeData,   // Version data
    next: Option<Box<Version>>,  // Older version
}

struct VersionedNode {
    current: Arc<Version>,
    lock: RwLock<()>,
}

// Read at epoch E sees version with max(epoch <= E)
fn read_at_epoch(node: &VersionedNode, epoch: u64) -> Arc<Version> {
    let mut ver = node.current.clone();
    while ver.epoch > epoch {
        ver = ver.next.as_ref().unwrap().clone();
    }
    ver
}
```

### Lock-Free Catalog

```rust
use dashmap::DashMap;

struct Catalog {
    labels: DashMap<String, u32>,     // Lock-free
    types: DashMap<String, u32>,      // Lock-free
    next_label_id: AtomicU32,
    next_type_id: AtomicU32,
}

impl Catalog {
    fn get_or_create_label(&self, name: &str) -> u32 {
        *self.labels.entry(name.to_string())
            .or_insert_with(|| {
                self.next_label_id.fetch_add(1, Ordering::SeqCst)
            })
            .value()
    }
}
```

## Testing Requirements

- Unit tests for executor cloning
- Unit tests for concurrent query execution
- Integration tests with 10, 50, 100 concurrent queries
- Stress tests with 1000+ concurrent queries
- Performance benchmarks measuring throughput
- CPU profiling to verify multi-core utilization
- Thread-safety tests with ThreadSanitizer
- Deadlock detection tests

---

## FUTURE OPTIMIZATIONS REQUIREMENTS (Phase 6-10)

### ADDED Requirements - Phase 6: Storage Engine Overhaul

#### Requirement: Custom Graph Storage Engine
The system SHALL implement a custom storage engine optimized for graph workloads that replaces LMDB.

##### Scenario: Graph-Native Storage Format
Given a graph database with nodes and relationships
When data is stored using the custom engine
Then relationships SHALL be stored contiguously with their source nodes
And relationship types SHALL be clustered for efficient filtering
And memory-mapped I/O SHALL be used for optimal performance
And compression SHALL be applied to adjacency lists for high-degree nodes

##### Scenario: Direct I/O Optimization
Given SSD storage devices
When data is written to disk
Then O_DIRECT SHALL be used to bypass OS page cache
And page alignment SHALL be optimized for SSD block sizes
And prefetching SHALL be implemented for sequential access patterns
And NVMe-specific optimizations SHALL be applied when available

#### Requirement: Advanced Relationship Indexing
The system SHALL implement specialized indexing for relationship traversal and filtering.

##### Scenario: Compressed Adjacency Lists
Given nodes with thousands of relationships
When adjacency lists are stored
Then lists SHALL be compressed using graph-specific algorithms
And skip-lists SHALL enable fast traversal to specific relationship types
And bloom filters SHALL provide fast existence checks
And memory usage SHALL be optimized for dense relationship graphs

### ADDED Requirements - Phase 7: Query Execution Engine Rewrite

#### Requirement: Vectorized Query Execution
The system SHALL implement SIMD-accelerated query execution for optimal performance.

##### Scenario: SIMD Aggregation Operations
Given aggregation queries with large datasets
When aggregations are executed
Then SIMD instructions SHALL be used for parallel computation
And memory access patterns SHALL be optimized for CPU cache locality
And vectorized filtering SHALL reduce branch mispredictions
And CPU cache-aware algorithms SHALL minimize cache misses

##### Scenario: JIT Query Compilation
Given frequently executed Cypher queries
When queries are processed
Then queries SHALL be compiled to native code using JIT
And query plans SHALL be cached for reuse
And expression evaluation SHALL be optimized at compile time
And runtime query optimization SHALL adapt to data characteristics

### ADDED Requirements - Phase 8: Relationship Processing Optimization

#### Requirement: Specialized Relationship Storage
The system SHALL implement dedicated storage structures for relationship data.

##### Scenario: Relationship File Separation
Given a graph database with many relationships
When data is stored
Then relationships SHALL be stored in separate files from nodes
And relationship-specific page layouts SHALL be used
And batch loading SHALL optimize sequential access
And cache locality SHALL be optimized for relationship traversal

##### Scenario: Advanced Traversal Algorithms
Given complex relationship traversal queries
When paths are computed
Then BFS/DFS SHALL use SIMD acceleration
And shortest path algorithms SHALL be optimized for graph structure
And parallel relationship expansion SHALL utilize multiple cores
And path finding SHALL use heuristic optimizations

### ADDED Requirements - Phase 9: Memory and Concurrency Optimization

#### Requirement: NUMA-Aware Memory Allocation
The system SHALL optimize memory allocation for multi-socket architectures.

##### Scenario: NUMA Thread Scheduling
Given a multi-socket server with NUMA architecture
When queries are executed
Then threads SHALL be scheduled on the same NUMA node as their data
And memory allocation SHALL have affinity to specific NUMA nodes
And cross-NUMA communication SHALL be minimized
And cache coherence SHALL be optimized for NUMA boundaries

##### Scenario: Lock-Free Data Structures
Given concurrent access to shared data structures
When multiple threads access catalogs and indexes
Then lock-free alternatives SHALL replace RwLock where possible
And atomic operations SHALL be used for counters
And wait-free algorithms SHALL be implemented for critical paths
And memory barriers SHALL be optimized for performance

### ADDED Requirements - Phase 10: Advanced Features and Polish

#### Requirement: Query Result Caching
The system SHALL implement intelligent caching of query results.

##### Scenario: Result Set Caching
Given frequently executed queries with stable results
When queries are executed
Then result sets SHALL be cached with appropriate invalidation
And compression SHALL reduce memory usage for large results
And cache warming SHALL pre-populate frequently used results
And invalidation strategies SHALL maintain data consistency

##### Scenario: Observability and Monitoring
Given a production deployment
When the system is monitored
Then detailed performance metrics SHALL be collected
And query profiling SHALL identify bottlenecks
And system health SHALL be continuously monitored
And automated regression detection SHALL prevent performance degradation

## Performance Success Criteria

### Phase 6 Success Criteria
- [ ] Single-hop relationship queries: ≤ 1.0ms average
- [ ] CREATE relationship operations: ≤ 5.0ms average
- [ ] Storage I/O overhead: ≤ 50% of current levels
- [ ] Memory efficiency: ≤ 200MB for 1M relationships

### Phase 7 Success Criteria
- [ ] Complex JOIN queries: ≤ 3.0ms average
- [ ] Aggregation performance: ≤ 2.0ms for 100K nodes
- [ ] Query compilation overhead: ≤ 1ms per query
- [ ] Concurrent throughput: ≥ 500 queries/second

### Phase 8 Success Criteria
- [ ] Path finding (length 3): ≤ 2.0ms average
- [ ] High-degree node traversal: ≤ 5.0ms for 10K relationships
- [ ] Relationship property queries: ≤ 1.5ms average
- [ ] Memory per relationship: ≤ 50 bytes

### Overall Performance Targets
- [ ] **50% of Neo4j Performance**: After Phase 6 completion
- [ ] **75% of Neo4j Performance**: After Phase 6-8 completion
- [ ] **90% of Neo4j Performance**: After Phase 6-9 completion
- [ ] **95% of Neo4j Performance**: After all phases + fine-tuning

## Implementation Notes - Future Phases

### Storage Engine Architecture
```rust
// Phase 6: Custom Graph Storage Engine
struct GraphStorageEngine {
    node_store: MemoryMappedNodeStore,
    relationship_store: MemoryMappedRelationshipStore,
    adjacency_store: CompressedAdjacencyStore,
    index_manager: AdvancedIndexManager,
}

// Key components:
// - Memory-mapped storage with direct I/O
// - Relationship-centric data layout
// - Compression for adjacency lists
// - SSD-aware allocation strategies
```

### Query Execution Pipeline
```rust
// Phase 7: Vectorized Query Execution
struct VectorizedExecutor {
    vector_processor: SimdProcessor,
    jit_compiler: QueryCompiler,
    plan_cache: PlanCache,
    result_cache: ResultCache,
}

// Key features:
// - SIMD operations for aggregations
// - JIT compilation of Cypher queries
// - Vectorized filtering and projection
// - Advanced join algorithms (hash, merge)
```

### Relationship Processing
```rust
// Phase 8: Advanced Relationship Processing
struct RelationshipProcessor {
    storage_engine: SpecializedRelationshipStore,
    traversal_engine: SimdTraversalEngine,
    path_finder: OptimizedPathFinder,
    cache_manager: RelationshipCacheManager,
}

// Key optimizations:
// - Separate relationship files
// - SIMD-accelerated BFS/DFS
// - Parallel relationship expansion
// - Specialized property indexing
```

### NUMA and Concurrency
```rust
// Phase 9: NUMA-Aware Execution
struct NumaAwareExecutor {
    scheduler: NumaScheduler,
    allocator: NumaAllocator,
    lock_free_catalog: LockFreeCatalog,
    cache_partitioner: CachePartitioner,
}

// Key optimizations:
// - NUMA-aware thread scheduling
// - Memory allocation affinity
// - Lock-free data structures
// - Cache partitioning by NUMA node
```

