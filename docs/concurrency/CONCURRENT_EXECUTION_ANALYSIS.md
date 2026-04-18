# Concurrent Execution Analysis

## Current Status

**After removing async lock:**
- Speedup: 0.21x (still worse than sequential)
- Efficiency: 1%
- Avg query time: 521ms (vs 107ms sequential)

## Problem Diagnosis

### 1. Thread Pool Bottleneck

Tokio's `spawn_blocking` uses a limited thread pool:
- Default size: `num_cpus::get()` (typically 4-8 threads)
- With 20 concurrent queries, only 4-8 execute simultaneously
- Remaining queries queue, causing serialization

**Solution**: Increase blocking thread pool size or use custom thread pool.

### 2. Internal Lock Contention

Even though queries run in different threads, they compete for internal locks:

```rust
// In Executor::execute
let label_index_guard = self.label_index();  // Read lock
let planner = QueryPlanner::new(..., &label_index_guard);
// Lock held during entire planning phase
```

**Impact**: 
- Multiple queries may wait for `label_index.read()`
- `store.read()` may serialize data access
- Locks held for entire query execution duration

**Solution**: 
- Minimize lock scope (clone data instead of holding locks)
- Use lock-free data structures (DashMap, etc.)
- Implement RCU pattern for read-heavy operations

### 3. Query Execution Flow

Current flow:
1. Clone executor (lock-free ✅)
2. `spawn_blocking` → thread pool
3. `execute()` → acquire `label_index.read()` lock
4. Planning phase (lock held)
5. Execution phase → acquire `store.read()` lock
6. Return results

**Bottlenecks**:
- Step 3-4: Planning locks serialize queries
- Step 5: Store locks serialize data access

## Recommended Next Steps

### Priority 1: Increase Thread Pool Size

```rust
// In main.rs, configure Tokio runtime
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(num_cpus::get())
    .max_blocking_threads(num_cpus::get() * 4)  // 4x more blocking threads
    .enable_all()
    .build()?;
```

### Priority 2: Reduce Lock Scope

```rust
// Instead of:
let label_index_guard = self.label_index();
let planner = QueryPlanner::new(..., &label_index_guard);

// Do:
let label_index_snapshot = self.label_index().clone();  // Clone data
let planner = QueryPlanner::new(..., &label_index_snapshot);
// Lock released immediately
```

### Priority 3: Lock-Free Data Structures

Replace `Arc<RwLock<LabelIndex>>` with `Arc<DashMap<...>>` for read-heavy operations.

## Expected Improvements

After implementing all fixes:
- **Target Speedup**: 10-15x (for 20 queries on 4-core machine)
- **Target Efficiency**: 50-75%
- **CPU Utilization**: 70%+ (up from ~12%)

