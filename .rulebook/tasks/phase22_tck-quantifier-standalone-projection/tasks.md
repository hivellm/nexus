## 1. Implementation
- [ ] 1.1 Prove the mechanism: instrument the no-pattern planning path and record, at a `file:line`, exactly which operator drops the row for `RETURN all(x IN [1,2] WHERE x > 0)`
- [ ] 1.2 Fix it so the quantifier's bound variable is scoped to the quantifier and never becomes a query-level filter or a planned scan
- [ ] 1.3 Cover all four quantifiers plus the `filter()` legacy form, with and without an upstream clause
- [ ] 1.4 Confirm `expressions/quantifier` moved out of the single digits; if it did not, stop and re-root-cause rather than layering a second fix

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
