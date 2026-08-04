## 1. Implementation
- [ ] 1.1 Bind a path value for the zero-length case (`MATCH p = (a)`)
- [ ] 1.2 Bind a path value for fixed-length single-hop and multi-hop patterns
- [ ] 1.3 Preserve pattern-written node/relationship order, including alternating directions
- [ ] 1.4 `nodes(p)`, `relationships(p)`, `length(p)` over the bound path
- [ ] 1.5 OPTIONAL MATCH yields a null path when unmatched; MERGE binds one

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
