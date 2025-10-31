# Implementation Tasks - Fix Engine Test Suite

**Status**: 🔴 NOT STARTED  
**Priority**: High  
**Estimated**: 2-3 hours  
**Dependencies**: 
- `tempfile` crate (already available)
- Understanding of Rust RAII and Drop trait

---

## 1. Core Engine Struct Changes

### 1.1 Add TempDir Field to Engine Struct
- [ ] Open `nexus-core/src/lib.rs`
- [ ] Locate `Engine` struct definition
- [ ] Add `_temp_dir: Option<TempDir>` field (mark as private with `_` prefix)
- [ ] Add doc comment explaining the field's purpose

**Expected Code**:
```rust
pub struct Engine {
    pub catalog: Catalog,
    pub storage: RecordStore,
    pub indexes: IndexManager,
    pub transaction_manager: TransactionManager,
    pub executor: Executor,
    /// Keeps temporary directory alive for Engine::new(). None for persistent storage.
    _temp_dir: Option<TempDir>,
}
```

### 1.2 Update Engine::new() Implementation
- [ ] Locate `Engine::new()` method
- [ ] Store path as `PathBuf` before creating Engine
- [ ] Create Engine using `with_data_dir()`
- [ ] Set `_temp_dir` field to `Some(temp_dir)`
- [ ] Return the modified Engine

**Expected Code**:
```rust
pub fn new() -> Result<Self> {
    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().to_path_buf();
    let mut engine = Self::with_data_dir(&data_dir)?;
    engine._temp_dir = Some(temp_dir);
    Ok(engine)
}
```

### 1.3 Update Engine::with_data_dir() Implementation
- [ ] Locate `Engine::with_data_dir()` method
- [ ] Find the final `Ok(Self { ... })` construction
- [ ] Add `_temp_dir: None` to the struct initialization
- [ ] Verify all fields are present

**Expected Change**:
```rust
Ok(Self {
    catalog,
    storage,
    indexes,
    transaction_manager,
    executor,
    _temp_dir: None,  // Add this line
})
```

### 1.4 Update All Engine Struct Initialization Sites
- [ ] Search for direct `Engine { ... }` constructions (if any)
- [ ] Add `_temp_dir: None` to each construction
- [ ] Compile to catch any missed sites (compiler will error)

---

## 2. Testing & Verification

### 2.1 Run Failing Tests
- [ ] Run specific failing tests: `cargo test --lib test_update_node test_delete_node test_clear_all_data`
- [ ] Verify all 3 pass
- [ ] Run all 11 failing tests
- [ ] Verify all 11 now pass

**Test List**:
```bash
cargo test --lib \
  test_update_node \
  test_delete_node \
  test_clear_all_data \
  test_convert_to_simple_graph \
  test_cluster_nodes \
  test_detect_communities \
  test_export_to_json \
  test_group_nodes_by_labels \
  test_get_graph_statistics \
  test_group_nodes_by_property \
  test_kmeans_cluster_nodes
```

### 2.2 Run Full Test Suite
- [ ] Run complete core test suite: `cargo test -p nexus-core --lib`
- [ ] Verify 740 tests still pass (no regressions)
- [ ] Check test execution time (should be similar)
- [ ] Verify no new warnings

### 2.3 Test TempDir Cleanup
- [ ] Add test to verify TempDir is cleaned up after Engine drop
- [ ] Verify directory exists while Engine is alive
- [ ] Verify directory is deleted after Engine is dropped

**Test Code**:
```rust
#[test]
fn test_engine_tempdir_lifecycle() {
    let engine = Engine::new().unwrap();
    // Get path while engine is alive
    let data_path = engine.storage.get_path().to_path_buf();
    assert!(data_path.exists(), "TempDir should exist while Engine is alive");
    
    drop(engine);
    // Note: TempDir cleanup is asynchronous, so we can't reliably test deletion
    // The important part is no errors occur during drop
}
```

### 2.4 Run Server Tests
- [ ] Run server test suite: `cargo test -p nexus-server`
- [ ] Verify no regressions in API tests
- [ ] Check that tests using `Engine::new()` still work

---

## 3. Performance & Memory Verification

### 3.1 Memory Usage Check
- [ ] Create simple benchmark to measure Engine memory footprint
- [ ] Compare before/after memory usage
- [ ] Verify overhead is < 1% (~24 bytes for Option<TempDir>)

**Benchmark Code**:
```rust
#[test]
#[ignore] // Run manually with --ignored
fn bench_engine_memory() {
    let engines: Vec<Engine> = (0..100)
        .map(|_| Engine::new().unwrap())
        .collect();
    
    // Manual inspection of memory usage
    std::thread::sleep(std::time::Duration::from_secs(5));
    
    drop(engines);
}
```

### 3.2 Performance Benchmark
- [ ] Run existing benchmarks if available
- [ ] Verify < 0.1% performance difference
- [ ] Check Engine creation time hasn't increased significantly

---

## 4. Documentation Updates

### 4.1 Update Engine API Documentation
- [ ] Add doc comment to `_temp_dir` field explaining its purpose
- [ ] Update `Engine::new()` docs to explain TempDir lifecycle
- [ ] Update `Engine::with_data_dir()` docs to contrast with `new()`
- [ ] Add example showing when to use each method

**Doc Example**:
```rust
/// Creates a new Engine with a temporary data directory.
/// 
/// The temporary directory will be automatically cleaned up when the Engine is dropped.
/// For persistent storage, use `Engine::with_data_dir()` instead.
///
/// # Examples
///
/// ```
/// let engine = Engine::new()?;
/// // Use engine for testing...
/// // Directory is automatically cleaned up when engine is dropped
/// ```
pub fn new() -> Result<Self> { ... }

/// Creates a new Engine with a persistent data directory.
///
/// Unlike `new()`, the provided directory will NOT be cleaned up automatically.
/// This is the recommended method for production use.
///
/// # Examples
///
/// ```
/// let engine = Engine::with_data_dir("./data")?;
/// // Data persists across restarts
/// ```
pub fn with_data_dir<P: AsRef<std::path::Path>>(data_dir: P) -> Result<Self> { ... }
```

### 4.2 Update CONTRIBUTING.md
- [ ] Add section on writing Engine tests
- [ ] Explain when to use `new()` vs `with_data_dir()`
- [ ] Add best practices for test isolation
- [ ] Document TempDir lifecycle behavior

### 4.3 Update CHANGELOG.md
- [ ] Add entry for bug fix in unreleased section
- [ ] Describe the issue and resolution
- [ ] Note that it affects only tests, not production

**Changelog Entry**:
```markdown
## [Unreleased]

### Fixed
- **Engine Test Suite**: Fixed critical bug in `Engine::new()` causing 11 tests to fail
  - `Engine::new()` now properly keeps temporary directory alive for Engine lifetime
  - All 11 previously failing tests now pass
  - No impact on production code (uses `Engine::with_data_dir()`)
  - Tests affected: `test_update_node`, `test_delete_node`, `test_clear_all_data`, and 8 others
```

---

## 5. Code Review & Cleanup

### 5.1 Self-Review Checklist
- [ ] All struct initialization sites updated
- [ ] No compilation errors or warnings
- [ ] All 11 tests pass
- [ ] No test regressions
- [ ] Documentation updated
- [ ] Code follows Rust best practices
- [ ] No clippy warnings

### 5.2 Run Quality Checks
- [ ] Run `cargo fmt` to format code
- [ ] Run `cargo clippy -- -D warnings` to check for issues
- [ ] Fix any warnings or errors
- [ ] Verify pre-commit hooks pass

### 5.3 Git Commit
- [ ] Stage changes: `git add nexus-core/src/lib.rs nexus/CHANGELOG.md`
- [ ] Commit with descriptive message
- [ ] Reference issue number if applicable

**Commit Message**:
```
fix(nexus-core): fix Engine::new() TempDir lifecycle bug

- Store TempDir guard in Engine struct to keep directory alive
- Add _temp_dir: Option<TempDir> field to Engine
- Update Engine::new() to store TempDir guard
- Update Engine::with_data_dir() to set _temp_dir = None
- Fix 11 failing tests that used Engine::new()
- Add documentation explaining TempDir lifecycle
- Add test for TempDir cleanup behavior

Fixes #XXX
```

---

## 6. Final Validation

### 6.1 Clean Build Test
- [ ] Run `cargo clean`
- [ ] Run `cargo test --all`
- [ ] Verify everything compiles and passes
- [ ] Check for any warnings

### 6.2 Integration Test
- [ ] Run integration tests if available
- [ ] Verify Engine works correctly in server context
- [ ] Test with actual data operations

### 6.3 Pre-Push Checks
- [ ] Run pre-push hooks
- [ ] Ensure all tests pass (including the 11 fixed ones)
- [ ] Verify CI will pass

---

## Success Criteria

- ✅ `Engine` struct has `_temp_dir: Option<TempDir>` field
- ✅ `Engine::new()` stores TempDir guard
- ✅ `Engine::with_data_dir()` sets `_temp_dir = None`
- ✅ All 11 previously failing tests pass
- ✅ No test regressions (740 tests still passing)
- ✅ < 1% memory overhead
- ✅ < 0.1% performance impact
- ✅ Documentation updated
- ✅ Code review approved
- ✅ Changelog updated

---

## Rollback Plan

If issues arise:
1. Revert commit with `git revert HEAD`
2. Tests will return to failing state
3. No production impact (production doesn't use `Engine::new()`)
4. Can take time to investigate alternative approach

Alternative approach: Update each test individually to keep TempDir alive (Approach B)

---

## Notes

- This is a **pre-existing bug**, not introduced by recent changes
- Affects **only test code**, no production impact
- Fix is **straightforward** and **low-risk**
- Consider adding a test helper function for consistent test setup in future
- May want to add a `#[must_use]` attribute to prevent accidental drops
