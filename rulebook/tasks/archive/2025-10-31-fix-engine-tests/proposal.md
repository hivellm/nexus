# Fix Engine Test Suite - Proposal

## Why

11 unit tests in `nexus-core/src/lib.rs` are failing due to a critical bug in `Engine::new()` that causes premature deletion of temporary directories. This prevents proper testing of core Engine functionality and blocks CI/CD pipelines. The bug is a pre-existing issue unrelated to recent changes but must be fixed to ensure test suite reliability and code quality.

## What Changes

### Core Issue
The `Engine::new()` method creates a `TempDir` guard but immediately drops it, causing the temporary directory to be deleted before the Engine can use it:

```rust
pub fn new() -> Result<Self> {
    let temp_dir = tempfile::tempdir()?;  // TempDir created
    let data_dir = temp_dir.path();       // Path borrowed
    Self::with_data_dir(data_dir)         // TempDir dropped! 💥
}                                         // Directory deleted immediately
```

### Proposed Solution
Store the `TempDir` guard in the `Engine` struct to keep the directory alive for the lifetime of the Engine:

```rust
pub struct Engine {
    pub catalog: Catalog,
    pub storage: RecordStore,
    pub indexes: IndexManager,
    pub transaction_manager: TransactionManager,
    pub executor: Executor,
    _temp_dir: Option<TempDir>,  // Add this field (private)
}
```

## Current Status

**Progress**: 0% Complete (Investigation phase complete)

### Failing Tests (11 total)
- ❌ `test_update_node` - Update node properties
- ❌ `test_delete_node` - Delete node from graph
- ❌ `test_clear_all_data` - Clear all graph data
- ❌ `test_convert_to_simple_graph` - Convert to simple graph format
- ❌ `test_cluster_nodes` - Node clustering algorithms
- ❌ `test_detect_communities` - Community detection
- ❌ `test_export_to_json` - Export graph to JSON
- ❌ `test_group_nodes_by_labels` - Group nodes by label
- ❌ `test_get_graph_statistics` - Calculate graph statistics
- ❌ `test_group_nodes_by_property` - Group nodes by property
- ❌ `test_kmeans_cluster_nodes` - K-means clustering

### Error Pattern
All tests fail with the same error:
```
called `Result::unwrap()` on an `Err` value: Io(Os { code: 2, kind: NotFound, message: "No such file or directory" })
```

## Root Cause Analysis

### The Bug
1. `Engine::new()` calls `tempfile::tempdir()` which returns a `TempDir` guard
2. The `TempDir` guard is **not** stored anywhere
3. At the end of `Engine::new()`, the `TempDir` goes out of scope
4. Rust's RAII (Drop) automatically deletes the directory
5. The returned `Engine` references a deleted directory
6. All subsequent file operations fail

### Why It Wasn't Caught Earlier
- Production code uses `Engine::with_data_dir()` with persistent directories
- The 11 tests were likely being skipped or ignored
- Bug has existed since initial implementation

## Impact

### Test Impact
**High** - 11 out of 740 tests failing (1.5% failure rate)

### Code Usage
- **38 locations** use `Engine::new()`
- **All in test code** (no production usage)
- Files affected:
  - `nexus-core/src/lib.rs` (27 usages)
  - `nexus-server/src/api/health.rs` (5 usages)
  - `nexus-server/src/api/schema.rs` (2 usages)
  - `nexus-server/src/api/data.rs` (1 usage)
  - `nexus-server/src/api/streaming.rs` (1 usage)
  - `nexus-server/src/api/auto_generate.rs` (1 usage)
  - `tests/integration_test.rs` (1 usage)

### Production Impact
**None** - Production code exclusively uses `Engine::with_data_dir()` with persistent directories

## Implementation Strategy

### Approach A: Store TempDir in Engine (RECOMMENDED)

**Pros**:
- ✅ No test code changes needed (38 locations unchanged)
- ✅ Maintains API compatibility
- ✅ Minimal code changes (~10 lines)
- ✅ Clear ownership semantics
- ✅ Automatic cleanup via RAII
- ✅ Follows Rust best practices

**Cons**:
- Small memory overhead (~24 bytes per Engine instance)
- Requires updating struct initialization sites

**Changes Required**:
1. Add `_temp_dir: Option<TempDir>` field to `Engine` struct
2. Update `Engine::new()` to store the guard (2 lines)
3. Update `Engine::with_data_dir()` to set `None` (1 line)
4. Update all struct initialization sites

### Approach B: Update Tests Individually (ALTERNATIVE)

**Pros**:
- ✅ No struct changes needed
- ✅ Explicit test setup

**Cons**:
- ❌ Requires changing 38 test locations
- ❌ More code churn
- ❌ Risk of missing some tests
- ❌ Less maintainable

**Not Recommended** - Too much code churn for minimal benefit

## Success Metrics

- ✅ All 11 failing tests pass
- ✅ 0 new test failures introduced
- ✅ TempDir properly cleaned up after Engine drop
- ✅ No memory leaks detected
- ✅ < 1% memory overhead
- ✅ < 0.1% performance impact
- ✅ Documentation updated

## Implementation Tasks

See `tasks.md` for detailed task breakdown and progress tracking.

## Risks & Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Memory leak from TempDir storage | Low | Medium | Add explicit test for Drop behavior |
| Performance regression | Very Low | Low | Run benchmarks before/after |
| Breaking changes | None | None | Internal implementation only |
| Missing initialization sites | Low | High | Compiler will catch all sites |

**Overall Risk**: **LOW** ✅

## Timeline

- **Investigation** ✅: Complete (analysis done)
- **Implementation**: 30-60 minutes (struct + new() + with_data_dir())
- **Testing**: 30 minutes (run full test suite)
- **Documentation**: 15-30 minutes (update docs)
- **Review**: 30 minutes (code review)
- **Total**: **2-3 hours**

## Dependencies

- ✅ `tempfile` crate (already in use)
- ✅ No external dependencies
- ✅ No breaking API changes

## Affected Specs

- Storage engine (`specs/storage/spec.md`) - Document TempDir lifecycle
- Testing guide (`CONTRIBUTING.md`) - Add test best practices
- Engine API docs - Document `new()` vs `with_data_dir()` behavior

## Affected Code

### Core Changes
- `nexus-core/src/lib.rs`:
  - `Engine` struct definition
  - `Engine::new()` implementation
  - `Engine::with_data_dir()` implementation
  - All struct initialization sites

### Test Changes
- None (if using Approach A)

## Breaking Changes

**None** - All changes are internal implementation details

## Next Steps

1. ✅ Create proposal and task documents (complete)
2. ⏳ Implement Approach A (struct changes)
3. ⏳ Run full test suite and verify fixes
4. ⏳ Update documentation
5. ⏳ Commit and merge changes

## References

- Issue thread: Test failures in `nexus-core/src/lib.rs`
- Related work: Test suite fixes (commit `678a7ad`)
- `tempfile` crate: https://docs.rs/tempfile/
- Rust RAII pattern: https://doc.rust-lang.org/book/ch15-03-drop.html

