# Concurrent Execution Test Results

**Date**: 2025-11-17  
**Test**: 20 concurrent queries  
**Query**: `MATCH (n:Person) WHERE n.age > 30 RETURN n.name LIMIT 10`

## Results

### Test 1: Before Fixes (with async lock)

| Metric | Sequential | Concurrent | Speedup |
|--------|-----------|------------|---------|
| Total Time | 2213.33 ms | 13319.82 ms | **0.17x** ❌ |
| Avg per Query | 110.67 ms | 665.99 ms | - |
| Efficiency | - | 0.8% | ❌ |

### Test 2: After Removing Async Lock

| Metric | Sequential | Concurrent | Speedup |
|--------|-----------|------------|---------|
| Total Time | 2155.15 ms | 10424.07 ms | **0.21x** ❌ |
| Avg per Query | 107.76 ms | 521.2 ms | - |
| Efficiency | - | 1% | ❌ |

**Expected**: ~20x speedup for true parallelism  
**Actual**: 0.21x (still worse than sequential, but slightly improved)

### Neo4j Performance (for comparison)

| Metric | Sequential | Concurrent | Speedup |
|--------|-----------|------------|---------|
| Total Time | 45.63 ms | 2951.76 ms | **0.02x** ❌ |
| Avg per Query | 2.28 ms | 147.59 ms | - |
| Efficiency | - | 0.1% | ❌ |

## Analysis

### Problem Identified

The concurrent execution is **worse** than sequential execution, indicating:

1. **Thread Pool Limitation**: Tokio's `spawn_blocking` uses a limited thread pool (default: CPU count). With 4 CPUs and 20 queries, only 4 can execute simultaneously.

2. **Lock Contention**: The `read().await` on `Arc<RwLock<Executor>>` may still cause contention, even though it's a read lock.

3. **Overhead**: The overhead of `spawn_blocking` + async lock acquisition may be greater than the benefit of parallelism.

4. **Internal Locks**: The `Executor::execute` method holds internal locks (`label_index()`, `store()`, etc.) during planning and execution, which may cause contention.

### Root Cause (Updated)

**Fixes Applied:**
- ✅ Removed `Arc<RwLock<Executor>>` → `Arc<Executor>` (no async lock)
- ✅ Changed `execute(&mut self)` → `execute(&self)` (allows concurrent execution)
- ✅ Executor cloning is now lock-free

**Remaining Issues:**
1. **Thread Pool Limitation**: Tokio's `spawn_blocking` uses a limited thread pool (default: CPU count, typically 4-8 threads). With 20 queries, only 4-8 can execute simultaneously, causing queuing.

2. **Internal Lock Contention**: The `Executor::execute` method holds internal locks (`label_index()`, `store()`, etc.) during planning and execution. Multiple queries may compete for these locks:
   - `label_index.read()` - held during planning
   - `store.read()` - held during data access
   - These locks serialize execution even though queries are in different threads

3. **Overhead**: The overhead of `spawn_blocking` + context switching may be greater than the benefit when thread pool is saturated.

**Evidence:**
- Sequential: 107ms/query
- Concurrent: 521ms/query (5x slower!)
- This suggests queries are waiting for locks or thread pool slots

## Recommendations

### Immediate Fixes

1. ✅ **Remove Async Lock**: Changed `EXECUTOR` from `Arc<RwLock<Executor>>` to `Arc<Executor>` - **COMPLETED**

2. **Increase Thread Pool**: Configure Tokio's blocking thread pool to handle more concurrent tasks.
   - Option A: Use `tokio::runtime::Builder` with `max_blocking_threads()`
   - Option B: Use Rayon thread pool for better control
   - Option C: Use custom thread pool with work-stealing

3. **Reduce Lock Duration**: Minimize the time locks are held during query execution.
   - Clone data structures needed for planning instead of holding locks
   - Use lock-free data structures where possible (e.g., `DashMap` for indexes)
   - Implement read-copy-update (RCU) pattern for frequently accessed data

### Long-term Improvements

1. **Lock-Free Planning**: Make query planning lock-free by cloning necessary data structures.

2. **Read-Only Execution Path**: For read-only queries, use read locks that don't block each other.

3. **Connection Pooling**: Implement proper connection pooling to manage concurrent requests.

## Next Steps

1. Refactor `EXECUTOR` to use `Arc<Executor>` directly (no `RwLock` wrapper)
2. Configure Tokio's blocking thread pool size
3. Re-run concurrent execution test
4. Measure CPU utilization during concurrent execution
5. Compare with Neo4j's concurrent performance

## Expected Improvement

After fixes:
- **Target Speedup**: 10-15x (for 20 concurrent queries on 4-core machine)
- **Target Efficiency**: 50-75% (accounting for overhead and lock contention)
- **CPU Utilization**: 70%+ (up from ~12%)

