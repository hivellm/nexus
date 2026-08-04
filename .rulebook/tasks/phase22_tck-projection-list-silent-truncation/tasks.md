## 1. Implementation
- [ ] 1.1 Make an unconsumed projection item a hard parse error — no truncation, no dropped clause (regression test asserting the error, not the truncated shape)
- [ ] 1.2 Support `IS NULL` / `IS NOT NULL` as a postfix operator inside a larger expression, not only as a whole predicate
- [ ] 1.3 Support `.prop` after a call result and after a parenthesised expression (`f(x).p`, `(expr).p`)
- [ ] 1.4 Support comparison chaining at the precedence the spec requires
- [ ] 1.5 Re-measure and record the new counts for RC18 and RC19 so their tasks can be sized

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
