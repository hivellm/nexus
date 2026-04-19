# Proposal: Fix LMDB Parallel Test Isolation

## Why

### Problem Statement

The project has **116+ tests marked as ignored** and **35+ tests completely removed** due to race conditions when running tests in parallel. The issues manifest specifically in Docker/CI environments (GitHub Actions) but not locally (WSL/PowerShell).

### Root Causes

1. **LMDB Environment Limits**: LMDB has a hard limit on concurrent environments per process. When `cargo nextest` runs tests in parallel, each test creates its own LMDB environment via `TempDir::new()`, quickly exhausting this limit.

2. **TempDir Race Conditions**: `TempDir::new().unwrap()` creates a temporary directory, but there's a race condition where the directory may not exist when `Catalog::new()` or `RecordStore::new()` tries to access it in high-parallelism environments.

3. **No Centralized Test Infrastructure**: Each test file has its own `create_test_executor()` or `setup_test_engine()` function, creating 620+ instances of `TempDir::new()` across the codebase. This duplication makes it impossible to implement proper resource management.

4. **Inconsistent Fixes**: Some files add `std::fs::create_dir_all()` while others don't, creating inconsistent behavior across tests.

### Impact

- **CI Reliability**: Tests pass locally but fail on GitHub Actions, blocking deployments
- **Developer Experience**: Developers can't trust CI results
- **Technical Debt**: 116+ ignored tests represent significant uncovered code paths
- **Maintenance Burden**: Each new test risks introducing the same issues

### Current Metrics

| Metric | Value |
|--------|-------|
| Tests with `#[ignore]` for race conditions | 116+ |
| Tests completely removed | 35+ |
| Files using `TempDir::new()` | 61 |
| Duplicate `create_test_executor` implementations | 14 |
| Duplicate `setup_test_engine` implementations | 13 |

## What Changes

### Solution Overview

Implement a centralized test infrastructure with proper resource management:

1. **Test Harness Module** (`nexus-core/src/testing/mod.rs`)
   - Singleton pattern for shared resources
   - Thread-local LMDB environments
   - Automatic cleanup and resource pooling

2. **Serial Test Attribute**
   - Use `serial_test` crate for tests requiring exclusive access
   - Mark database-modifying tests appropriately

3. **Standardized Test Helpers**
   - Single source of truth for `create_test_executor()`
   - Guaranteed directory existence before component initialization
   - Proper cleanup on test completion

4. **Migration Script**
   - Migrate all existing tests to use new infrastructure
   - Re-enable ignored tests
   - Restore removed tests where appropriate

### Architecture

```
nexus-core/src/testing/
├── mod.rs              # Main test harness
├── executor.rs         # Executor test helpers
├── engine.rs           # Engine test helpers
├── fixtures.rs         # Common test data setup
└── isolation.rs        # Test isolation strategies
```

### Key Components

1. **TestContext**: Manages test lifecycle and cleanup
2. **ResourcePool**: Reuses LMDB environments across tests
3. **IsolationLevel**: Configurable isolation (serial/parallel)
4. **TestFixtures**: Pre-built common scenarios

## Benefits

1. **CI Reliability**: Tests will pass consistently in all environments
2. **Performance**: Resource reuse reduces test execution time
3. **Maintainability**: Single source of truth for test infrastructure
4. **Scalability**: New tests automatically get proper isolation
5. **Coverage**: Re-enable 116+ ignored tests

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking existing tests | Gradual migration with fallback |
| Performance regression | Benchmark before/after |
| Complexity increase | Clear documentation and examples |

## Success Criteria

- [ ] All 116+ ignored tests re-enabled and passing
- [ ] CI passes consistently (10 consecutive runs)
- [ ] No `TempDir::new()` in individual test files
- [ ] Single `create_test_executor()` implementation
- [ ] Test execution time within 10% of current
