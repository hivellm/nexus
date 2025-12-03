# Tasks: Fix LMDB Parallel Test Isolation

## Phase 1: Infrastructure

- [x] 1.1 Add `serial_test` and `once_cell` crates to Cargo.toml
- [x] 1.2 Create `nexus-core/src/testing/mod.rs` module
- [x] 1.3 Implement `TestContext` struct with lifecycle management
- [x] 1.4 Implement `ResourcePool` for LMDB environment reuse
- [x] 1.5 Create `create_test_executor()` with guaranteed directory existence
- [x] 1.6 Create `setup_test_engine()` with guaranteed directory existence
- [x] 1.7 Export testing module in `lib.rs` behind `#[cfg(feature = "testing")]`
- [x] 1.8 Add comprehensive documentation for testing module

## Phase 2: Migration - Core Tests

- [x] 2.1 Migrate `executor_comprehensive_test.rs`
- [x] 2.2 Migrate `test_regression_fixes.rs`
- [x] 2.3 Migrate `test_create_with_return.rs`
- [x] 2.4 Migrate `test_create_without_return.rs`
- [x] 2.5 Migrate `test_array_slicing.rs`
- [x] 2.6 Migrate `test_array_concatenation.rs`
- [x] 2.7 Migrate `test_string_concatenation.rs`
- [x] 2.8 Migrate `test_multiple_relationship_types.rs`
- [x] 2.9 Migrate `test_relationship_counting.rs`
- [x] 2.10 Migrate `geospatial_integration_test.rs`

## Phase 3: Migration - Extended Tests

- [x] 3.1 Migrate `neo4j_compatibility_test.rs`
- [x] 3.2 Migrate `integration_extended.rs`
- [x] 3.3 Migrate `regression_extended.rs`
- [x] 3.4 Migrate `builtin_functions_test.rs`
- [x] 3.5 Migrate `neo4j_behavior_tests.rs`
- [x] 3.6 Migrate `null_comparison_tests.rs`
- [x] 3.7 Migrate `logical_operators_tests.rs`
- [x] 3.8 Migrate `unwind_tests.rs`
- [x] 3.9 Migrate `validation_comprehensive_test.rs`
- [x] 3.10 Migrate `integration.rs`
- [x] 3.11 Migrate `new_functions_test.rs`
- [x] 3.12 Migrate `regression_tests.rs`
- [x] 3.13 Migrate `test_write_intensive.rs`
- [x] 3.14 Migrate `test_metadata_count_optimization.rs`
- [x] 3.15 Migrate `test_index_consistency.rs`
- [x] 3.16 Migrate `test_storage_init.rs`
- [x] 3.17 Migrate `relationship_traversal_test.rs`
- [x] 3.18 Migrate `relationship_prop_ptr_test.rs`
- [x] 3.19 Migrate `graph_comparison_test.rs`
- [x] 3.20 Migrate `performance_tests.rs`
- [x] 3.21 Migrate benchmark files (benchmark_*.rs)
- [x] 3.22 Migrate loader_comprehensive_test.rs
- [x] 3.23 Migrate phase8/9 optimization tests
- [x] 3.24 Migrate security_tests.rs

## Phase 4: Migration - Internal Tests

- [x] 4.1 Migrate `src/catalog/mod.rs` tests
- [x] 4.2 Migrate `src/database/mod.rs` tests
- [x] 4.3 Migrate `src/udf/registry.rs` tests
- [x] 4.4 Migrate `src/auth/storage.rs` tests
- [x] 4.5 Migrate `src/auth/audit.rs` tests
- [x] 4.6 Migrate `src/graph/core.rs` tests
- [x] 4.7 Migrate `src/validation.rs` tests
- [x] 4.8 Migrate `src/executor/mod.rs` tests
- [x] 4.9 Migrate `src/executor/geospatial_tests.rs`
- [x] 4.10 Migrate `src/wal/mod.rs` tests
- [x] 4.11 Migrate `src/wal/async_wal.rs` tests
- [x] 4.12 Migrate `src/lib.rs` tests
- [x] 4.13 Migrate `src/storage/adjacency_list.rs` tests
- [x] 4.14 Migrate `src/storage/mod.rs` tests
- [x] 4.15 Migrate `src/loader/mod.rs` tests
- [x] 4.16 Migrate `src/graph/procedures.rs` tests
- [x] 4.17 Migrate `src/graph/algorithms.rs` tests
- [x] 4.18 Migrate `src/auth/mod.rs` tests
- [x] 4.19 Migrate `src/plugin/tests.rs`
- [x] 4.20 Migrate `src/execution/integration_bench.rs`
- [x] 4.21 Migrate `src/storage/property_store.rs` tests

## Phase 5: Re-enable Ignored Tests

- [x] 5.1 Audit all `#[ignore]` with TODO comments
- [x] 5.2 Re-enable tests with temp dir race conditions (~60 tests)
- [x] 5.3 Re-enable tests with parallel conflicts
- [x] 5.4 Re-enable tests with LMDB environment issues
- [ ] 5.5 Add `#[serial]` attribute where necessary (deferred - not needed with TestContext isolation)

## Phase 6: Server Tests

- [x] 6.1 Migrate `nexus-server/tests/mcp_auth_test.rs`
- [x] 6.2 Migrate `nexus-server/src/api/graph_correlation_mcp_tests.rs`
- [x] 6.3 Migrate `nexus-server/src/api/cypher_test.rs`
- [x] 6.4 Migrate `nexus-server/src/api/database.rs` tests
- [x] 6.5 Migrate `nexus-server/src/api/export.rs` tests
- [x] 6.6 Migrate `nexus-server/src/api/comparison.rs` tests
- [x] 6.7 Migrate `nexus-server/src/api/ingest.rs` tests
- [x] 6.8 Migrate `nexus-server/tests/vectorizer_integration_test.rs`
- [x] 6.9 Migrate `nexus-server/src/main.rs` tests
- [x] 6.10 Migrate `nexus-server/src/config.rs` tests
- [x] 6.11 Migrate `nexus-server/src/api/property_keys.rs` tests

## Phase 7: Validation

- [x] 7.1 Run all tests locally (1262 nexus-core + 246 nexus-server tests passed)
- [ ] 7.2 Run Docker Ubuntu tests (manual - requires Docker environment)
- [ ] 7.3 Verify CI passes (manual - requires CI runs)
- [x] 7.4 Update CHANGELOG.md
- [x] 7.5 Update test documentation (spec.md and testing/mod.rs already documented)
- [x] 7.6 Remove obsolete test helper duplicates (geospatial_tests.rs, lib.rs)
