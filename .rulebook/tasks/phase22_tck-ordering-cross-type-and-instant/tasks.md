## 1. Implementation
- [ ] 1.1 Single comparator shared by ORDER BY, min/max, and the comparison operators
- [ ] 1.2 Cross-type total order per spec, null last ascending
- [ ] 1.3 Element-wise list ordering including nulls and mixed element types
- [ ] 1.4 Instant-based comparison and ordering for offset-aware `time` and `datetime`
- [ ] 1.5 Confirm the scalar cross-type ordering from the earlier task did not regress

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
