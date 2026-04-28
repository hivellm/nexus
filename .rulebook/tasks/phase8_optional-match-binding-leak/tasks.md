## 1. Implementation
- [ ] 1.1 Reproduce the bug end-to-end with a Rust integration test under `crates/nexus-core/tests/optional_match_binding_leak_test.rs`. Capture the four scenarios from the proposal as failing assertions.
- [ ] 1.2 Inspect `crates/nexus-core/src/executor/operators/expand.rs:374-387` (no-match LEFT-OUTER preserve branch) and `:228-240` (chained-OPTIONAL no-source path). Confirm the variable scope leak — the no-match branch likely calls `set_variable(rel_var, Null)` without also calling `set_variable(target_var, Null)`, or vice versa.
- [ ] 1.3 Fix: on every no-match LEFT-OUTER path, explicitly bind the relationship variable, the target variable, and any path variable to `Value::Null` before emitting the row. Cover both first-hop and chained shapes.
- [ ] 1.4 Re-run the integration test from §1.1 — assertions flip to passing.
- [ ] 1.5 Add a proptest covering: for every node with no outgoing edges of a given type, OPTIONAL MATCH binds rel + target to NULL, NEVER to the source.
- [ ] 1.6 Run the Neo4j diff-suite — 300/300 stays.
- [ ] 1.7 Confirm aggregations on top of OPTIONAL MATCH (`count(b)`, `collect(b)`) report the correct (0 / empty) values when the OPTIONAL MATCH had zero matches.
- [ ] 1.8 Update `docs/specs/cypher-subset.md` OPTIONAL MATCH semantics section.
- [ ] 1.9 Run `cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings` clean and `cargo +nightly fmt --all -- --check` clean.

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
