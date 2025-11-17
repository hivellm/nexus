# Optimize Performance: Concurrent Query Execution

**Date**: 2025-11-17  
**Status**: Draft  
**Priority**: CRITICAL  

## Why

Comprehensive performance benchmarking reveals that Nexus is **60% slower** than Neo4j in throughput (202 qps vs 507 qps) and exhibits significant performance gaps across all operation categories. The root cause analysis identified the **single most critical bottleneck**: the global executor lock that serializes all queries.

### Current Performance Status

**Benchmark Results Summary:**
- **Throughput**: Nexus 202.7 qps vs Neo4j 507.2 qps (60% slower)
- **Average Latency**: Nexus 6.0ms vs Neo4j 3.2ms (46.7% slower)
- **Write Operations**: 73-85% slower than Neo4j
- **Aggregations**: 35-75% slower than Neo4j
- **CPU Utilization**: ~12% on 8-core machine (only 1 core active)

### Root Cause: Global Executor Lock

**The #1 performance bottleneck:**

```rust
// nexus-server/src/api/cypher.rs:1441
static EXECUTOR: Arc<RwLock<Executor>> = OnceLock::new();

// Every query does this:
let mut executor = executor_guard.write().await;  // ← BLOCKS ALL OTHER QUERIES
let result = executor.execute(&query);
```

**Impact:**
- Only **1 query executes at a time** (single-threaded)
- **7 CPU cores sit idle** on an 8-core machine
- All queries wait in line for the exclusive lock
- Neo4j uses 10-20 concurrent threads

**This is NOT a Rust vs Java issue** - it's an architectural choice that can be fixed.

### Business Impact

1. **Scalability**: Cannot utilize modern multi-core servers
2. **Throughput**: 2.5x slower than Neo4j in concurrent scenarios
3. **Latency**: Queries wait for previous queries to complete
4. **Cost**: Wasted CPU resources (88% idle)
5. **Adoption**: Performance gap prevents enterprise adoption

## What Changes

This task implements **three-phase performance optimization** to achieve 90-95% of Neo4j's performance:

### Phase 0: CRITICAL - Remove Global Executor Lock (Target: 150% throughput improvement)
**Timeline**: 1-2 weeks
**Impact**: Affects ALL operations equally

1. **Refactor Executor for Concurrency**
   - Analyze executor state (identify shared vs per-query state)
   - Implement executor cloning or snapshot mechanism
   - Remove mutable shared state where possible
   - Create per-query execution context
   - Ensure thread-safety of storage layer

2. **Implement Concurrent Execution**
   - Add thread pool for query execution (Rayon or tokio::spawn_blocking)
   - Implement query dispatcher with load balancing
   - Add read-write lock differentiation (readers don't block readers)
   - Update storage layer for concurrent access
   - Implement MVCC snapshot isolation for reads

**Expected Results:**
- Throughput: 202 qps → 500+ qps (2.5x improvement)
- CPU utilization: 12% → 80%+
- Query latency: Slight improvement (no queuing)

### Phase 1: Write Operations Optimization (Target: 50% improvement)
**Timeline**: 2-3 weeks

1. **Write Batching and Buffering**
   - Implement write buffer for CREATE operations
   - Add WAL group commit (batch multiple writes)
   - Defer index updates within transactions
   - Implement write coalescing

2. **Lock Optimization**
   - Move from table-level to row-level locking
   - Implement lock-free data structures for catalog
   - Reduce lock hold times
   - Add lock-free read paths

3. **Catalog Optimization**
   - Cache catalog lookups within transaction scope
   - Pre-allocate label/type IDs
   - Batch catalog updates

**Expected Results:**
- CREATE operations: 12-15ms → 6-8ms (40-50% improvement)
- CREATE relationship: 22ms → 10-12ms (45-55% improvement)

### Phase 2: Aggregation Optimization (Target: 40% improvement)
**Timeline**: 1-2 weeks

1. **Metadata-Based COUNT**
   - Use node count metadata for `COUNT(*)`
   - Avoid full table scans for simple counts
   - Cache count results

2. **Aggregation Pushdown**
   - Push aggregations to storage layer
   - Pre-size data structures (HashMap, Vec)
   - Implement parallel aggregation for large datasets

3. **Memory Optimization**
   - Reduce allocations during aggregation
   - Use capacity hints for collections
   - Implement result caching

**Expected Results:**
- COUNT: 6.3ms → 2.0ms (68% improvement)
- GROUP BY: 5.7ms → 3.0ms (47% improvement)
- COLLECT: 5.3ms → 3.0ms (43% improvement)

### Phase 3: Relationship Traversal Optimization (Target: 35% improvement)
**Timeline**: 2-3 weeks

1. **Adjacency List Structure**
   - Implement adjacency list for relationships
   - Co-locate relationships with source nodes
   - Add relationship-type indexes

2. **Traversal Optimization**
   - Push WHERE filters into traversal
   - Implement relationship caching
   - Optimize property access

**Expected Results:**
- Single-hop traversal: 6.0ms → 3.5ms (42% improvement)
- Relationship count: 3.5ms → 2.0ms (43% improvement)

### Phase 4: Query Optimization (Target: 30% improvement)
**Timeline**: 2-3 weeks

1. **Cost-Based Optimization**
   - Implement cost model for operators
   - Add cardinality estimation
   - Optimize join order

2. **Advanced Joins**
   - Implement hash joins for large datasets
   - Add merge joins for sorted data
   - Cache intermediate results

**Expected Results:**
- Complex queries: 7ms → 4ms (43% improvement)
- JOIN-like queries: 6.9ms → 4.0ms (42% improvement)

### Phase 5: Filter and Sorting Optimization (Target: 20% improvement)
**Timeline**: 1-2 weeks

1. **Filter Optimization**
   - Index-based filtering
   - Expression compilation
   - Filter reordering by selectivity
   - SIMD-optimized filters

2. **Sorting Optimization**
   - Index-based ordering
   - Top-K optimization for ORDER BY + LIMIT
   - Parallel sorting for large result sets

**Expected Results:**
- WHERE filters: 4-5ms → 2.5-3ms (30-40% improvement)
- ORDER BY: 4-5ms → 3-3.5ms (20-30% improvement)

## Impact

### Code Impact

**Affected Components:**
- `nexus-server/src/api/cypher.rs` - Remove global executor lock, add dispatcher
- `nexus-core/src/executor/mod.rs` - Make executor cloneable/thread-safe
- `nexus-core/src/executor/planner.rs` - Add cost-based optimization
- `nexus-core/src/storage/` - Concurrent access support
- `nexus-core/src/catalog.rs` - Lock-free catalog operations
- `nexus-core/src/transaction/` - MVCC snapshot isolation

### Performance Impact

**After Phase 0 (Week 2):**
- Throughput: 200 qps → 500+ qps (2.5x)
- All operations benefit from parallelism

**After Phase 1 (Week 5):**
- Write operations: 50% faster
- Overall: 3x faster than current

**After Phase 2 (Week 7):**
- Aggregations: Neo4j competitive
- Overall: 3.5x faster than current

**After All Phases (Week 12-14):**
- **Overall Performance: 90-95% of Neo4j**
- Throughput: 500+ qps (matches Neo4j)
- Write operations: 6-8ms (competitive)
- Aggregations: 2-3ms (competitive)
- Relationships: 3-4ms (competitive)

### Breaking Changes

- **None**: All changes are internal optimizations
- API remains unchanged
- Query syntax unchanged
- Wire protocol unchanged

### Testing Impact

- Add concurrent query tests
- Add performance regression tests
- Add benchmark suite
- All existing tests must pass

## Success Criteria

- [ ] Phase 0: Throughput ≥ 500 qps (2.5x improvement)
- [ ] Phase 0: CPU utilization ≥ 70% on 8-core machine
- [ ] Phase 1: CREATE operations ≤ 8ms average
- [ ] Phase 2: COUNT(*) ≤ 2ms average
- [ ] Phase 3: Single-hop traversal ≤ 3.5ms average
- [ ] Phase 4: Complex queries ≤ 4ms average
- [ ] Phase 5: WHERE filters ≤ 3ms average
- [ ] All existing tests pass (no regressions)
- [ ] Benchmark shows 90-95% of Neo4j performance
- [ ] No breaking changes to API

## Dependencies

- None (all changes internal)
- May benefit from Rayon for parallel execution
- May use dashmap for lock-free concurrent HashMap

## Risks and Mitigation

### Risk 1: Concurrency Bugs
**Impact**: High  
**Probability**: Medium  
**Mitigation**: 
- Extensive testing with concurrent queries
- Use Rust's ownership system to prevent data races
- Add thread-safety assertions
- Implement gradual rollout with feature flag

### Risk 2: Performance Regression
**Impact**: High  
**Probability**: Low  
**Mitigation**:
- Benchmark before/after each phase
- Keep baseline measurements
- Implement performance regression tests
- Can rollback if needed

### Risk 3: Implementation Complexity
**Impact**: Medium  
**Probability**: Medium  
**Mitigation**:
- Phased approach allows incremental progress
- Each phase is independently testable
- Can pause after Phase 0 if needed (still 2.5x improvement)

## Timeline

**Total Duration**: 12-14 weeks

- **Phase 0**: Week 1-2 (CRITICAL - must do first)
- **Phase 1**: Week 3-5
- **Phase 2**: Week 6-7
- **Phase 3**: Week 8-10
- **Phase 4**: Week 11-13
- **Phase 5**: Week 14

**First Milestone**: Week 2 - 2.5x throughput improvement

