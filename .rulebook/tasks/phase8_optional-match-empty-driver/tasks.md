## 1. Implementation
- [ ] 1.1 Reproduce the bug end-to-end with a Rust integration test under `crates/nexus-core/tests/optional_match_empty_driver_test.rs`. Capture the four shape-variants in the proposal as failing assertions.
- [ ] 1.2 Locate the planner branch that emits operators for a top-level OPTIONAL MATCH at `crates/nexus-core/src/executor/planner/queries.rs` (the same loop that handles `MatchClause.optional`).
- [ ] 1.3 Detect the "no prior driver" case (first clause of the query is an OPTIONAL MATCH, or the rolling row count would be zero pre-OPTIONAL).
- [ ] 1.4 Inject an implicit-driver source operator before the OPTIONAL pattern. Two implementation choices: (a) add `Operator::SingleEmptyRow` and emit it as the first operator; (b) add `optional: bool` to `Operator::NodeByLabel` / `AllNodesScan` and let the executor emit a wrapped-NULL row internally when the scan is empty.
- [ ] 1.5 Wire the chosen executor branch.
- [ ] 1.6 Re-run the integration test from §1.1 — assertions flip to passing.
- [ ] 1.7 Run the Neo4j diff-suite (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`) against a live Neo4j 2025.09.0 reference; confirm 300/300 still passes and `Section 11` (OPTIONAL MATCH) compatibility moves from 0% toward 100%.
- [ ] 1.8 Update `docs/specs/cypher-subset.md` OPTIONAL MATCH section.
- [ ] 1.9 Run `cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings` clean and `cargo +nightly fmt --all -- --check` clean.

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
