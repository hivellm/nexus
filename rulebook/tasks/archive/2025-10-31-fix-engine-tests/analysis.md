# Analysis Report - Engine::new() TempDir Issue

**Date**: 2025-10-31  
**Analyzed by**: AI Assistant  
**Severity**: High (11 tests failing)  

---

## Executive Summary

A critical bug in `Engine::new()` causes 11 unit tests to fail due to premature cleanup of temporary directories. The issue affects only test code, not production usage. The fix is straightforward with minimal risk.

---

## Impact Analysis

### Code Usage Statistics

`Engine::new()` is used in **38 locations** across **7 files**:

| File | Occurrences | Context |
|------|-------------|---------|
| `nexus-core/src/lib.rs` | 27 | Unit tests |
| `nexus-server/src/api/health.rs` | 5 | API tests |
| `nexus-server/src/api/schema.rs` | 2 | API tests |
| `nexus-server/src/api/data.rs` | 1 | API test |
| `nexus-server/src/api/streaming.rs` | 1 | API test |
| `nexus-server/src/api/auto_generate.rs` | 1 | API test |
| `tests/integration_test.rs` | 1 | Integration test |

**Total**: 38 usages, all in test code ✅

### Production Impact

**None** - Production code exclusively uses `Engine::with_data_dir()` with persistent directories.

### Test Impact

**11 failing tests** in `nexus-core/src/lib.rs`:
1. `test_update_node` (line ~1233)
2. `test_delete_node` (line ~1277)
3. `test_clear_all_data` (line ~1570)
4. `test_convert_to_simple_graph` (line ~1314)
5. `test_cluster_nodes` (line ~1351)
6. `test_detect_communities` (line ~1466)
7. `test_export_to_json` (line ~1501)
8. `test_group_nodes_by_labels` (line ~1384)
9. `test_get_graph_statistics` (line ~1544)
10. `test_group_nodes_by_property` (line ~1412)
11. `test_kmeans_cluster_nodes` (line ~1440)

---

## Root Cause Analysis

### The Bug

```rust
// Current implementation (BUGGY)
pub fn new() -> Result<Self> {
    let temp_dir = tempfile::tempdir()?;  // TempDir guard created
    let data_dir = temp_dir.path();       // Path borrowed
    Self::with_data_dir(data_dir)         // TempDir dropped here! 💥
}                                         // Directory deleted
```

### Why It Fails

1. `tempfile::tempdir()` returns a `TempDir` guard
2. When `TempDir` is dropped, it deletes the directory
3. `temp_dir` goes out of scope at end of function
4. Directory is deleted before `Engine` can use it
5. Subsequent file operations fail with `ENOENT`

### Error Example

```
thread 'tests::test_delete_node' panicked at nexus-core/src/lib.rs:1286:14:
called `Result::unwrap()` on an `Err` value: Io(Os { code: 2, kind: NotFound, message: "No such file or directory" })
```

---

## Recommended Solution

### Approach A: Store TempDir in Engine (RECOMMENDED)

**Pros**:
- ✅ No test changes needed
- ✅ Maintains API compatibility
- ✅ Minimal code changes
- ✅ Clear ownership semantics
- ✅ Automatic cleanup via RAII

**Cons**:
- Small memory overhead (~24 bytes per Engine)

**Implementation**:

```rust
pub struct Engine {
    pub catalog: Catalog,
    pub storage: RecordStore,
    pub indexes: IndexManager,
    pub transaction_manager: TransactionManager,
    pub executor: Executor,
    _temp_dir: Option<TempDir>,  // Add this field
}

impl Engine {
    pub fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let data_dir = temp_dir.path().to_path_buf();
        let mut engine = Self::with_data_dir(&data_dir)?;
        engine._temp_dir = Some(temp_dir);  // Keep guard alive
        Ok(engine)
    }

    pub fn with_data_dir<P: AsRef<std::path::Path>>(data_dir: P) -> Result<Self> {
        // ... existing implementation
        Ok(Self {
            catalog,
            storage,
            indexes,
            transaction_manager,
            executor,
            _temp_dir: None,  // Add this
        })
    }
}
```

**Changes Required**:
- 1 field added to `Engine` struct
- 2 lines in `Engine::new()`
- 1 line in `Engine::with_data_dir()`
- Update all struct initialization sites

### Approach B: Update Tests Individually

**Pros**:
- ✅ No struct changes
- ✅ Explicit test setup

**Cons**:
- ❌ Requires changing 38 test sites
- ❌ More code churn
- ❌ Risk of missing some tests

**Implementation**:

```rust
#[test]
fn test_delete_node() {
    let _temp_dir = TempDir::new().unwrap();  // Keep guard alive
    let mut engine = Engine::with_data_dir(_temp_dir.path()).unwrap();
    // ... test code
}
```

---

## Implementation Plan

### Phase 1: Core Fix (Approach A)
1. Add `_temp_dir: Option<TempDir>` to `Engine` struct
2. Update `Engine::new()` to store TempDir
3. Update `Engine::with_data_dir()` to set `None`
4. Update all struct initialization sites

### Phase 2: Testing
1. Run full test suite
2. Verify all 11 tests pass
3. Check for memory leaks
4. Run benchmarks

### Phase 3: Documentation
1. Document TempDir lifecycle
2. Update API docs
3. Add examples

### Phase 4: Cleanup
1. Remove test skip annotations
2. Re-enable pre-commit hooks
3. Update CI configuration

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Memory leak | Low | Medium | Add test for Drop impl |
| Performance regression | Very Low | Low | Run benchmarks |
| Breaking changes | None | None | Internal implementation only |
| Test failures | Very Low | Low | Comprehensive testing |

**Overall Risk**: **LOW** ✅

---

## Timeline Estimate

| Phase | Estimated Time |
|-------|----------------|
| Investigation | ✅ Complete |
| Implementation | 30-60 minutes |
| Testing | 30 minutes |
| Documentation | 15-30 minutes |
| Review | 30 minutes |
| **Total** | **2-3 hours** |

---

## Success Metrics

- ✅ All 11 tests pass
- ✅ 0 new test failures
- ✅ < 1% memory overhead
- ✅ < 0.1% performance impact
- ✅ 100% test coverage maintained

---

## Next Steps

1. ✅ Create OpenSpec task document
2. ⏳ Implement Approach A
3. ⏳ Run full test suite
4. ⏳ Update documentation
5. ⏳ Commit and push changes

---

## References

- `tempfile` crate docs: https://docs.rs/tempfile/
- Related commit: `678a7ad` (test fixes)
- Issue introduced: Long-standing (pre-existing)

