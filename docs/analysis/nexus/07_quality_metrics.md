# 07 — Quality Metrics

## Test surface

| Metric | Value | Source |
|--------|-------|--------|
| Workspace tests passing | **2310** | `cargo +nightly test --workspace`, README |
| Workspace tests ignored | 67 | same |
| Workspace tests failing | 0 | same |
| Lib-only (`-p nexus-core --lib`) | 2019 | README |
| V2 sharding unit tests | 143 | `crates/nexus-core/src/sharding/` |
| Raft unit tests | 65 | `crates/nexus-core/src/sharding/raft/` |
| Coordinator unit tests | 46 | `crates/nexus-core/src/coordinator/` |
| Cluster isolation integration | ~20 | `crates/nexus-core/tests/cluster_isolation_tests.rs` |
| V2 E2E sharding | ~12 | `crates/nexus-core/tests/v2_sharding_e2e.rs` |
| Cluster mode (multi-tenant) | ~41 | `crates/nexus-core/src/cluster/` |
| Neo4j diff-suite | **300 / 300** | `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` |
| 74-test cross-bench | 73 / 74 Nexus-faster, 52 / 74 row-identical | `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` |
| SDK comprehensive tests | ≥ 30 per SDK × 6 SDKs | `sdks/run-all-comprehensive-tests.ps1` |

## Coverage (per layer, from PERFORMANCE / phase reports)

| Module | Coverage | Notes |
|--------|----------|-------|
| Catalog (LMDB) | 98.64 % | 21 tests |
| Record stores | 96.96 % | 18 tests |
| Page cache | 96.15 % | 21 tests |
| WAL | 96.71 % | 16 tests |
| MVCC / transactions | 99.02 % | 20 tests |
| Storage layer global | 96.06 % | 133 tests including 15 integration |
| Executor | not published | mixed; lower than storage |
| Planner | not published | mixed |
| Coordinator | not published | 46 tests but coverage % not reported |

**Action item:** publish per-module coverage in CI artifact (`cargo llvm-cov --workspace --json`). Storage hits 95 %+ goal; executor / planner / coordinator are unknowns.

## CI / quality gates (per `AGENTS.md`)

| Gate | Threshold | Enforcement |
|------|-----------|-------------|
| Format (`cargo +nightly fmt --all`) | no diff | pre-commit hook |
| Lint (`cargo clippy --workspace -- -D warnings`) | 0 warnings | pre-commit hook |
| Tests | 100 % pass | pre-commit hook |
| Coverage | ≥ 95 % new code | manual check (`cargo llvm-cov`) |
| Security audit | `npm audit --production` per release | manual |
| Pre-commit hook bypass | forbidden (`--no-verify`) | rule in `.claude/rules/git-safety.md` |
| Unwrap in `bin/` | banned outside `#[cfg(test)]` | `scripts/ci/check_no_unwrap_in_bin.sh` |

## Tech-debt signals (grep-level)

| Signal | Count est. | Hotspot files |
|--------|----|---------------|
| `// TODO` in `execution/jit/` | high | Cranelift codegen disabled — most ops are stubs |
| `// TODO: Check if index is actually cached` | 1 | `crates/nexus-core/src/cache/mod.rs` (property-index eviction) |
| `#[ignore] // TODO: Fix - uses default data dir` | 2 | `crates/nexus-core/src/engine/tests.rs` (parallel-test isolation) |
| Deferred phase items | several | `phase6_spatial-planner-followups`, `phase6_opencypher-quantified-path-patterns`, `phase6_opencypher-subquery-transactions` |
| Version-string drift | — | README v1.13.0 / CHANGELOG v1.2.0 / SDK v1.15.0 |

The `AGENTS.md` Tier-1 prohibition is "no shortcuts / TODOs / placeholders." Most of the codebase honors it — the JIT compiler is the major outlier and probably needs to be **either finished or removed entirely** (a disabled JIT module is itself a Tier-1 violation per the project's own rules).

## Documentation quality

| Doc | Status |
|-----|--------|
| README.md | comprehensive; openCypher matrix at footer |
| docs/ARCHITECTURE.md | 1245 lines, comprehensive |
| docs/ROADMAP.md | 759 lines, phase-by-phase + status legend |
| docs/PERFORMANCE.md + docs/performance/* | 8 files, includes negative-results doc (good) |
| docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md | feature-by-feature |
| docs/specs/* | technical specs with SHALL/MUST + Given/When/Then |
| docs/security/AUTHENTICATION.md | full auth guide |
| docs/operations/REPLICATION.md | v1 master-replica ops |
| docs/CLUSTER_MODE.md | V2 multi-tenant operator guide |
| Per-SDK README + CHANGELOG + LICENSE | yes for all 6 |
| GraphRAG / LangChain integration cookbook | **missing** |
| Migration FROM Neo4j / Memgraph / Kuzu | **missing** |
| K8s / Helm / Docker Compose recipes | partial (`docs/docker/` exists, contents not audited here) |

## Code quality posture

- **Edition 2024 (Rust nightly 1.85+)** — modern idioms, async/await ubiquitous.
- **`anyhow::Context` + `thiserror`** for error propagation (per binary-boundary rules).
- **`parking_lot` over `std::sync`** for hot-path locks.
- **`heed` over `rocksdb`** for catalog (LMDB-native, no FFI).
- **`memmap2` for record stores** — direct kernel-mapped memory.
- **`tokio` for async runtime, `axum` for HTTP, `tower` for middleware.**
- **`hnsw_rs` for KNN, `tantivy` 0.22 for FTS, `roaring` for bitmaps.** All mainstream Rust crates with active maintenance.
- **`#[repr(C)]` for record types** with `ptr::copy_nonoverlapping` — explicit, predictable, LLVM auto-vectorizes.
- **Proptest** used for SIMD parity tests — best-practice for kernel correctness.

## Risk: where the test corpus is thin

- **Concurrency stress tests** — only "5 readers + 3 writers" mentioned in storage integration; no extended-duration soak under realistic mixed read/write workloads to surface the global-executor-lock issue earlier.
- **V2 cluster failure injection** — Raft is unit-tested for 3-node failover and 5-node partition, but no chaos-style network-partition / packet-loss / clock-skew tests in the repo as far as audited.
- **KNN correctness vs ground-truth** — recall is not measured.
- **Long-running WAL replay** — recovery is tested, but not at WAL sizes >> page cache.
- **Memory under sustained load** — only cold-start RSS measured; no leak detection across long-running sessions.

## Release hygiene

- **Conventional commits** enforced.
- **CHANGELOG.md** maintained per release.
- **Workflow:** task-driven via `mcp__rulebook__*`, with mandatory tail items (docs + tests + verify). Archive blocked without all three.
- **Version drift:** README badge (v1.13.0), CHANGELOG top entry (v1.2.0 dated 2026-04-28), SDK package versions (v1.15.0 across all 6) — these need to converge. Likely an artefact of independent crate-vs-SDK version trains.

## Recommendations

1. **Publish per-module coverage** as a CI artefact — currently we know storage is 95 %+ but executor / planner / coordinator are unaudited.
2. **Resolve the JIT module** — finish (~3 weeks) or delete (1 day). Disabled-with-TODOs violates project Tier-1.
3. **Fix the 2 ignored tests** — `tempfile::tempdir()` per test (1 day).
4. **Add chaos / failure-injection tests** for V2 Raft (network partitions, leader churn, slow disks).
5. **Measure KNN recall** at d=128/256/512/768 vs brute-force ground-truth on a public corpus (SIFT1M, GloVe).
6. **Reconcile version strings** across README / CHANGELOG / SDKs in next release train.
7. **Add migration guides** as a doc category — Neo4j → Nexus, Kuzu → Nexus, Memgraph → Nexus.
