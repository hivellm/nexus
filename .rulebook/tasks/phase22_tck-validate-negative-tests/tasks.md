## 1. Implementation
- [ ] 1.1 Unknown-function and wrong-entity-kind validation in the projection validator
- [ ] 1.2 Accept `i64::MIN` as a literal (apply the sign before the range check) and emit `IntegerOverflow` for out-of-range literals
- [ ] 1.3 Write-side checks: undefined variable in DELETE, label in DELETE, already-bound CREATE
- [ ] 1.4 CREATE relationship checks: untyped, undirected, bi-directed — right kind and right detail token (`RequiresDirectedRelationship`)
- [ ] 1.5 Argument count/kind validation for the list and map builtins (`range`, `reduce`, slices, map access)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass

## 3. Gates (every item, no exceptions)
- [ ] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
- [ ] 3.3 Neo4j differential suite still 300/300 (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`)
- [ ] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
- [ ] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
