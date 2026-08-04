## 1. Implementation
- [ ] 1.1 Write a decomposition plan for the five steps (file list, dependency order) and record it here before editing
- [ ] 1.2 Step 1 — full expression and WHERE parsing inside a write query
- [ ] 1.3 Step 2 — relationship variables visible to SET / REMOVE / DELETE
- [ ] 1.4 Step 3 — WITH before a write clause
- [ ] 1.5 Step 4 — OPTIONAL MATCH plus null-row write semantics (no-op write, row still returned)
- [ ] 1.6 Step 5 — MERGE without an alias, and `MERGE p = pattern`
- [ ] 1.7 Confirm no refusal message of the seven remains reachable for a legal query

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
