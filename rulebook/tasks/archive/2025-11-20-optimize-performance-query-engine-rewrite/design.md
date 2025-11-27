# Query Execution Engine Rewrite: Architecture Design

## Overview

This document outlines the architecture for rewriting Nexus's query execution engine from an interpreted, AST-based approach to a compiled, vectorized execution model optimized for modern hardware.

## Core Design Principles

### 1. Vectorized Execution Over Interpretation
**Problem**: Current execution interprets Cypher AST nodes one-by-one with virtual method dispatch.

**Solution**: Transform queries into columnar data operations that leverage SIMD instructions.

```rust
// Current: Interpreted execution (slow)
fn execute_where(&self, ast_node: &WhereClause) -> ResultSet {
    match ast_node.condition {
        Condition::Equal(field, value) => {
            let mut result = Vec::new();
            for record in self.scan_all() {
                if self.eval_equal(record, field, value) {
                    result.push(record);
                }
            }
            result
        }
        // ... more interpreted cases
    }
}

// Target: Vectorized execution (fast)
fn execute_where_vectorized(&self, condition: &VectorizedCondition) -> ColumnarResult {
    let field_column = self.get_column(condition.field);
    let value_vector = SIMD::splat(condition.value);

    let mask = field_column.simd_eq(value_vector);
    self.filter_by_mask(mask)
}
```

### 2. Compilation Over Interpretation
**Problem**: Cypher queries are parsed and interpreted on every execution.

**Solution**: Compile Cypher patterns into specialized Rust functions with query-specific optimizations.

```rust
// Current: Generic interpreter
fn execute_cypher(&self, query: &str) -> Result<QueryResult> {
    let ast = self.parse(query)?;
    self.interpret_ast(&ast)
}

// Target: Compiled execution
fn execute_compiled(&self, compiled_query: &CompiledQuery) -> Result<QueryResult> {
    // Direct native function call - no parsing, no interpretation
    compiled_query.execute_fn(self)
}
```

### 3. Columnar Data Layout
**Problem**: Data is stored and processed in row-based format unsuitable for SIMD.

**Solution**: Transform data into columnar format for vectorized operations.

```rust
// Row-based (current)
struct NodeRecord {
    id: u64,
    label: u32,
    properties: HashMap<String, Value>,
}

// Columnar (target)
struct NodeColumns {
    ids: Vec<u64>,        // SIMD-friendly
    labels: Vec<u32>,     // SIMD-friendly
    prop_keys: Vec<String>,
    prop_values: Vec<Value>,
}
```

## Detailed Architecture

### Phase 7.1: Vectorized Execution Foundation

#### 1. Columnar Data Structures
```
ColumnarResult {
    node_ids: Vec<u64>,      // 8-byte alignment, SIMD-ready
    node_labels: Vec<u32>,   // 4-byte alignment, SIMD-ready
    relationship_ids: Vec<u64>,
    relationship_types: Vec<u32>,
    // Property columns dynamically added
}
```

#### 2. SIMD-Accelerated Operators
```rust
pub struct VectorizedOperators {
    // SIMD register width (256-bit AVX2 = 32 bytes)
    vector_width: usize,
}

impl VectorizedOperators {
    pub fn filter_equal_i64(&self, column: &[u64], value: u64) -> Vec<bool> {
        let value_vec = SIMD::set1_epi64x(value);

        column.chunks_exact(self.vector_width / 8)
            .flat_map(|chunk| {
                let data_vec = SIMD::loadu_si256(chunk.as_ptr());
                let mask = SIMD::cmpeq_epi64(data_vec, value_vec);
                self.mask_to_bool_vec(mask)
            })
            .collect()
    }

    pub fn filter_range_i32(&self, column: &[u32], min: u32, max: u32) -> Vec<bool> {
        let min_vec = SIMD::set1_epi32(min as i32);
        let max_vec = SIMD::set1_epi32(max as i32);

        // Vectorized range check: min <= value <= max
    }
}
```

#### 3. Memory Layout Optimization
```rust
#[repr(align(32))]  // AVX2 alignment
pub struct AlignedColumn<T> {
    data: Vec<T>,
    _alignment: [T; 0],  // Ensures alignment
}

impl<T> AlignedColumn<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        let mut data = Vec::with_capacity(capacity);
        // Ensure allocation is aligned
        data.reserve_exact(capacity % (32 / std::mem::size_of::<T>()));
        Self { data, _alignment: [] }
    }
}
```

### Phase 7.2: JIT Query Compilation

#### 1. Compiled Query Interface
```rust
pub trait CompiledQuery {
    fn execute(&self, engine: &GraphEngine) -> Result<QueryResult>;
    fn is_stale(&self, schema_version: u64) -> bool;
    fn memory_usage(&self) -> usize;
    fn compilation_time(&self) -> Duration;
}

pub struct CompiledQueryFn {
    execute_fn: Box<dyn Fn(&GraphEngine) -> Result<QueryResult>>,
    schema_version: u64,
    created_at: Instant,
    execution_count: AtomicUsize,
}
```

#### 2. Query Specialization
```rust
// Generic pattern (slow)
fn match_friends(&self, person_id: u64) -> Vec<u64> {
    self.match_pattern(
        Pattern::Relationship {
            from: person_id,
            rel_type: Some("FRIEND"),
            direction: Direction::Outgoing,
        }
    )
}

// Specialized pattern (fast)
fn match_friends_specialized(&self, person_id: u64) -> Vec<u64> {
    // Inline adjacency list access
    // Direct relationship type filtering
    // Optimized memory access patterns
    unsafe {
        let adjacency_ptr = self.get_adjacency_ptr(person_id, FRIEND_TYPE_ID);
        self.traverse_adjacency_list_fast(adjacency_ptr)
    }
}
```

#### 3. Compilation Pipeline
```rust
pub struct QueryCompiler {
    schema: Arc<Schema>,
    optimization_level: OptimizationLevel,
    enable_simd: bool,
}

impl QueryCompiler {
    pub fn compile(&self, cypher_query: &str) -> Result<CompiledQuery> {
        // 1. Parse Cypher to AST
        let ast = self.parse_cypher(cypher_query)?;

        // 2. Analyze query patterns
        let analysis = self.analyze_patterns(&ast)?;

        // 3. Generate optimized Rust code
        let rust_code = self.generate_code(&ast, &analysis)?;

        // 4. Compile to native function
        let compiled_fn = self.compile_to_function(&rust_code)?;

        Ok(CompiledQueryFn::new(compiled_fn))
    }
}
```

### Phase 7.3: Advanced Join Algorithms

#### 1. Hash Join with Bloom Filters
```rust
pub struct HashJoinProcessor {
    hash_table: HashMap<u64, Vec<Row>>,
    bloom_filter: BloomFilter,
    max_memory: usize,
}

impl HashJoinProcessor {
    pub fn build(&mut self, build_side: impl Iterator<Item = Row>) {
        for row in build_side {
            let key = self.extract_join_key(&row);
            self.hash_table.entry(key).or_default().push(row.clone());

            // Update bloom filter for fast existence checks
            self.bloom_filter.insert(key);
        }
    }

    pub fn probe(&self, probe_side: impl Iterator<Item = Row>) -> impl Iterator<Item = JoinedRow> {
        probe_side.filter_map(|row| {
            let key = self.extract_join_key(&row);

            // Fast existence check with bloom filter
            if !self.bloom_filter.might_contain(key) {
                return None;
            }

            // Actual hash table lookup
            self.hash_table.get(&key)
                .and_then(|matches| {
                    // Return joined rows
                    Some(self.create_joined_rows(&row, matches))
                })
        }).flatten()
    }
}
```

#### 2. Adaptive Join Selection
```rust
pub enum JoinAlgorithm {
    HashJoin,
    MergeJoin,
    NestedLoop,
}

pub struct JoinSelector {
    statistics: Arc<QueryStatistics>,
}

impl JoinSelector {
    pub fn select_algorithm(&self, left: &DataSource, right: &DataSource) -> JoinAlgorithm {
        let left_size = self.estimate_cardinality(left);
        let right_size = self.estimate_cardinality(right);

        // Hash join for large datasets
        if left_size > 10000 && right_size > 10000 {
            return JoinAlgorithm::HashJoin;
        }

        // Merge join for sorted data
        if self.is_sorted(left) && self.is_sorted(right) {
            return JoinAlgorithm::MergeJoin;
        }

        // Nested loop as fallback
        JoinAlgorithm::NestedLoop
    }
}
```

## Implementation Strategy

### Incremental Rollout

#### Phase 1: Vectorized Operators (Safe Rollout)
- Add vectorized operators alongside existing interpreted ones
- Feature flag to enable vectorized execution
- Gradual migration of individual operations

#### Phase 2: Query Compilation (Progressive Enhancement)
- Start with simple pattern compilation
- Cache compiled queries with LRU eviction
- Fallback to interpreted execution for complex/uncompiled queries

#### Phase 3: Advanced Joins (Optimization Phase)
- Implement join algorithms as optional optimizations
- Cost-based selection between algorithms
- Comprehensive testing before production deployment

### Memory Management

#### 1. Columnar Memory Pools
```rust
pub struct ColumnarMemoryPool {
    page_size: usize,
    allocated_pages: Vec<Box<[u8]>>,
    free_pages: Vec<usize>,
}

impl ColumnarMemoryPool {
    pub fn allocate_column<T>(&mut self, capacity: usize) -> &mut [T] {
        let byte_size = capacity * std::mem::size_of::<T>();
        let page_index = self.allocate_page(byte_size);

        unsafe {
            std::slice::from_raw_parts_mut(
                self.allocated_pages[page_index].as_mut_ptr() as *mut T,
                capacity
            )
        }
    }
}
```

#### 2. SIMD-Aware Allocation
- Align allocations to SIMD register boundaries
- Pre-allocate memory pools to reduce allocation overhead
- Use huge pages for large columnar data structures

### Performance Optimizations

#### 1. Prefetching
```rust
pub struct DataPrefetcher {
    prefetch_distance: usize,
}

impl DataPrefetcher {
    pub fn prefetch_column(&self, column: &[u8]) {
        for i in (0..column.len()).step_by(64) {  // Cache line size
            unsafe {
                // Prefetch data into L1/L2 cache
                std::arch::x86_64::_mm_prefetch(
                    column.as_ptr().add(i) as *const i8,
                    std::arch::x86_64::_MM_HINT_T0
                );
            }
        }
    }
}
```

#### 2. Branch Prediction Optimization
- Layout data to minimize branch mispredictions
- Use conditional moves instead of branches where possible
- Profile-guided optimization for hot code paths

### Compatibility & Migration

#### 1. Feature Flags
```rust
pub struct ExecutionEngine {
    vectorized_enabled: bool,
    compilation_enabled: bool,
    advanced_joins_enabled: bool,
}

impl ExecutionEngine {
    pub fn execute_query(&self, query: &str) -> Result<QueryResult> {
        if self.compilation_enabled {
            if let Some(compiled) = self.get_compiled_query(query) {
                return compiled.execute(self);
            }
        }

        if self.vectorized_enabled {
            return self.execute_vectorized(query);
        }

        // Fallback to interpreted execution
        self.execute_interpreted(query)
    }
}
```

#### 2. Result Compatibility
- Ensure identical results between execution modes
- Comprehensive testing against interpreted baseline
- Statistical validation for performance-critical queries

## Success Metrics

### Performance Targets

#### Phase 7.1: Vectorized Execution
- **WHERE filters**: 4-5ms → 2.5-3ms (**40-50% improvement**)
- **Simple aggregations**: 5-6ms → 3-4ms (**35-40% improvement**)
- **Memory allocation**: ≤ 50% of current levels
- **CPU SIMD utilization**: ≥ 60%

#### Phase 7.2: JIT Compilation
- **Complex queries**: 7ms → 4ms (**43% improvement**)
- **JOIN-like queries**: 6.9ms → 4ms (**42% improvement**)
- **Query compilation time**: ≤ 10ms for typical queries
- **Cache hit rate**: ≥ 80% for repeated queries

#### Phase 7.3: Advanced Joins
- **Multi-pattern queries**: 8-10ms → 4-5ms (**50-60% improvement**)
- **Complex traversals**: 6-8ms → 3-4ms (**50-60% improvement**)
- **Join performance**: ≥ 3x faster than nested loops
- **Memory efficiency**: ≤ 80% of naive implementations

### Quality Metrics

#### Correctness
- **Result accuracy**: 100% identical to interpreted execution
- **Edge case handling**: All Cypher features supported
- **Error reporting**: Equivalent error messages and codes

#### Reliability
- **Memory safety**: Zero memory corruption or leaks
- **Thread safety**: Concurrent query execution support
- **Crash recovery**: Graceful handling of compilation failures

#### Maintainability
- **Code complexity**: Modular design with clear separation of concerns
- **Testing coverage**: ≥ 95% line coverage for new code
- **Documentation**: Comprehensive API documentation

## Conclusion

The query execution engine rewrite represents a fundamental transformation from interpretation to compilation, enabling Nexus to achieve Neo4j-level performance. By leveraging modern hardware capabilities (SIMD, JIT compilation, advanced algorithms), we can eliminate the remaining 50% performance gap.

The incremental approach ensures safe deployment while providing substantial performance improvements at each phase. The architecture is designed for long-term maintainability and extensibility, establishing Nexus as a high-performance graph database platform.
