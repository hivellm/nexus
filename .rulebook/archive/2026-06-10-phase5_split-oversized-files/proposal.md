# Proposal: phase5_split-oversized-files

## Why
17 Rust source files exceed 1500 lines (largest: `crates/nexus-core/src/engine/mod.rs` at 5391 lines). Oversized files degrade maintainability, slow code review, increase merge-conflict surface, and exceed practical context windows for AI-assisted development. Splitting them into cohesive submodules improves navigability and enforces single-responsibility boundaries.

## What Changes
Each file >1500 lines is split into logical submodules within the same parent module. Pure code-move refactor:

- Public API preserved via `pub use` re-exports — no external path changes.
- No logic changes, no signature changes, no behavior changes.
- `mod.rs` files become thin facades declaring submodules + re-exports.
- Test files split by feature area into `tests/` submodule directories.

Files (descending size):
1. `crates/nexus-core/src/engine/mod.rs` (5391)
2. `crates/nexus-core/src/executor/planner/queries.rs` (4430)
3. `crates/nexus-core/src/executor/eval/projection.rs` (4168)
4. `crates/nexus-core/src/executor/parser/clauses.rs` (3120)
5. `crates/nexus-core/src/engine/tests.rs` (3053)
6. `crates/nexus-core/src/executor/parser/tests.rs` (2345)
7. `crates/nexus-core/src/storage/mod.rs` (2232)
8. `crates/nexus-core/src/executor/operators/aggregate.rs` (2090)
9. `crates/nexus-core/src/executor/operators/procedures.rs` (2088)
10. `crates/nexus-core/src/graph/correlation/mod.rs` (2030)
11. `tests/integration_test.rs` (1892)
12. `crates/nexus-core/src/wal/mod.rs` (1824)
13. `crates/nexus-core/src/graph/correlation/pattern_recognition.rs` (1734)
14. `crates/nexus-core/src/executor/parser/expressions.rs` (1652)
15. `crates/nexus-core/src/catalog/mod.rs` (1641)
16. `crates/nexus-server/src/api/cypher/execute.rs` (1563)
17. `crates/nexus-server/src/api/streaming.rs` (1535)

## Impact
- Affected specs: none (no behavior change)
- Affected code: nexus-core (engine, executor, storage, graph, wal, catalog), nexus-server (api), tests/
- Breaking change: NO (public paths preserved via re-exports)
- User benefit: faster reviews, smaller diffs, better AI/tooling ergonomics, lower merge-conflict risk
