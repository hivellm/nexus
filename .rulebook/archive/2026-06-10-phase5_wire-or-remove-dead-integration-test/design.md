# Design: Test Inventory & Coverage Diff

## Status: item 1.1 complete — awaiting wire-vs-remove decision (item 1.2)

---

## Dead file: `tests/integration_test.rs`

Root `Cargo.toml` is a virtual workspace (`[workspace]` only, no `[package]`, no `[[test]]`).
No member crate's `Cargo.toml` declares a `[[test]]` target pointing at this file.
**The file is never compiled and never runs.**

Additional problem: the file contains syntax errors. Lines 686–688 show a truncated
`Value::Number(Number::from(20))],` fragment that belongs to `test_executor_e2e_order_by_limit`,
and a code fragment appears again at lines 2067–2089 after the API-performance section.
The file cannot compile even if wired as-is.

---

## Test inventory: `tests/integration_test.rs` (30 tests, 2091 lines)

### Group A — Storage/catalog/WAL/tx/cache integration (15 tests)

| # | Test name | Area |
|---|-----------|------|
| 1 | `test_workspace_compiles` | smoke |
| 2 | `test_tokio_runtime` | smoke |
| 3 | `test_catalog_storage_integration` | catalog + storage |
| 4 | `test_relationship_traversal_integration` | storage + traversal |
| 5 | `test_transaction_wal_integration` | tx + WAL |
| 6 | `test_page_cache_storage_integration` | cache + storage |
| 7 | `test_full_transaction_lifecycle` | tx lifecycle |
| 8 | `test_wal_crash_recovery` | WAL recovery |
| 9 | `test_page_cache_eviction_integration` | cache eviction |
| 10 | `test_multi_module_transaction` | multi-component tx |
| 11 | `test_mvcc_snapshot_isolation` | MVCC |
| 12 | `test_node_insert_performance` | perf: node insert |
| 13 | `test_node_read_performance` | perf: node read |
| 14 | `test_checkpoint_integration` | WAL checkpoint |
| 15 | `test_concurrent_transactions` | concurrency |

### Group B — Executor E2E (4 tests, all broken)

| # | Test name | Area |
|---|-----------|------|
| 16 | `test_executor_e2e_simple_match` | executor: MATCH |
| 17 | `test_executor_e2e_aggregation` | executor: GROUP BY |
| 18 | `test_executor_e2e_pattern_traversal` | executor: traversal |
| 19 | `test_executor_e2e_order_by_limit` | executor: ORDER BY / LIMIT (BROKEN — truncated body) |

### Group C — API error-handling (6 tests, use stale nexus-server API)

| # | Test name | Area |
|---|-----------|------|
| 20 | `test_api_error_handling_400_bad_request` | HTTP 400 |
| 21 | `test_api_error_handling_404_not_found` | HTTP 404 |
| 22 | `test_api_error_handling_405_method_not_allowed` | HTTP 405 |
| 23 | `test_api_error_handling_408_request_timeout` | HTTP 408 |
| 24 | `test_api_error_handling_500_internal_server_error` | HTTP 500 |
| 25 | `test_api_error_handling_malformed_requests` | malformed body |

### Group D — API performance (5 tests, use stale nexus-server API)

| # | Test name | Area |
|---|-----------|------|
| 26 | `test_api_performance_health_check` | perf: health |
| 27 | `test_api_performance_cypher_queries` | perf: cypher |
| 28 | `test_api_performance_concurrent_requests` | perf: concurrent |
| 29 | `test_api_performance_large_payloads` | perf: large body |
| 30 | `test_api_performance_mixed_workload` | perf: mixed |

---

## Coverage diff against existing per-crate tests

### Group A (tests 1–15): FULLY DUPLICATED

`crates/nexus-core/tests/integration.rs` contains exact equivalents for all 15:

| Dead test | Live equivalent | Notes |
|-----------|-----------------|-------|
| `test_workspace_compiles` | same name | asserts `CARGO_PKG_NAME == "nexus-core"` (live) vs `"nexus"` (dead) |
| `test_tokio_runtime` | same name | identical |
| `test_catalog_storage_integration` | same name | live uses `TestContext`; dead uses `TempDir` directly — same behavior |
| `test_relationship_traversal_integration` | same name | live copies packed fields to avoid UB; dead reads directly — live is stricter |
| `test_transaction_wal_integration` | same name | live uses `wal.path()` method; dead uses `wal.path` field — API drift |
| `test_page_cache_storage_integration` | same name | identical |
| `test_full_transaction_lifecycle` | same name | live uses `Catalog::with_isolated_path`; dead uses `Catalog::new` — live is cleaner |
| `test_wal_crash_recovery` | same name | identical logic |
| `test_page_cache_eviction_integration` | same name | identical |
| `test_multi_module_transaction` | same name | identical |
| `test_mvcc_snapshot_isolation` | same name | identical |
| `test_node_insert_performance` | same name | identical |
| `test_node_read_performance` | same name | identical |
| `test_checkpoint_integration` | same name | identical |
| `test_concurrent_transactions` | same name | identical |

**Conclusion**: zero unique coverage in Group A.

### Group B (tests 16–19): PARTIAL / BROKEN

`test_executor_e2e_simple_match`, `test_executor_e2e_aggregation`,
`test_executor_e2e_pattern_traversal`, and `test_executor_e2e_order_by_limit`
use a stale `Executor` constructor signature
(`Executor::new(catalog, store, label_index, knn_index)`) that no longer matches
the current API. `test_executor_e2e_order_by_limit` additionally has a truncated
body (syntax error at line 687).

`crates/nexus-core/tests/integration_extended.rs` and the many regression test
files cover equivalent or deeper executor behavior via `setup_test_engine()` /
`engine.execute_cypher(...)`.

**Conclusion**: zero unique non-broken coverage; the live tests are superior.

### Group C (tests 20–25): NOT COVERED IN PER-CRATE TESTS (but broken)

`crates/nexus-server/tests/integration_tests.rs` covers `test_health_check`,
`test_404_endpoint`, `test_concurrent_requests`, `test_http_methods`,
`test_large_payload`, and `test_error_handling` at a higher level (real HTTP).
However the dead Group-C tests use a stale `NexusServer` struct constructor
(missing `engine` field in tests 21–25, present in test 20 only) and stale
`api::*::init_*` global initializer patterns that no longer match the current
server architecture.

`crates/nexus-server/tests/integration_tests.rs` provides similar HTTP-level
error-handling coverage without the stale API surface.

**Conclusion**: conceptually non-overlapping with live tests but uncompilable;
no unique coverage that isn't already present in live per-crate tests.

### Group D (tests 26–30): NOT COVERED IN PER-CRATE TESTS (but broken)

No direct performance-benchmark equivalents in `nexus-server` tests. However:
- `crates/nexus-core/tests/performance_tests.rs`,
  `integration_performance_test.rs`, and `benchmark_*.rs` cover storage-level
  throughput.
- The API-level throughput assertions (>1000 req/sec health, >100 req/sec cypher)
  are reasonable but would need the server constructors fixed before they could
  compile.

**Conclusion**: thin unique coverage (API throughput numbers), but uncompilable
due to the same stale `NexusServer` constructor issues as Group C.

---

## Summary table

| Group | Tests | Unique compilable coverage |
|-------|-------|---------------------------|
| A — storage/WAL/tx/cache | 15 | 0 (all duplicated in `integration.rs`) |
| B — executor E2E | 4 | 0 (stale API + syntax error) |
| C — API error handling | 6 | ~0 (stale API, similar coverage in `nexus-server/tests/`) |
| D — API performance | 5 | thin (no direct peer, but broken) |
| **Total** | **30** | **~0 compilable unique tests** |

---

## Recommendation for item 1.2

The file provides **no unique compilable coverage**. All 15 storage-layer tests
are identically covered by `crates/nexus-core/tests/integration.rs` (the live
version is actually more correct — it fixes unaligned-reference UB and uses
`TestContext` isolation). The executor, API, and performance tests are uncompilable
due to stale constructor signatures and a syntax error.

**Recommended decision: REMOVE** (requires explicit user authorization per Tier-1 rules).

If the user prefers to wire it, the minimum repair work is:
1. Fix the syntax error in `test_executor_e2e_order_by_limit` (lines 686–689, 2067–2089).
2. Delete the 15 Group-A tests (fully duplicated).
3. Fix `Executor::new` call signatures in Group B (4 tests).
4. Fix `NexusServer` struct construction in Groups C and D (11 tests) — add the
   `engine` field and replace `api::*::init_*` globals with the current
   `AppState`-based injection pattern.
5. Split the result into ≤1500-line files and place under
   `crates/nexus-server/tests/` (since the API tests depend on `nexus-server`).
6. Run `cargo check` and `cargo test`.
