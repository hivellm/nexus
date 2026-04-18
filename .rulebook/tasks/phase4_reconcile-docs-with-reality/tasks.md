## 1. Implementation
- [ ] 1.1 Run the Neo4j compatibility test suite (`scripts/test-neo4j-nexus-compatibility-200.ps1`) and record the real pass/fail split
- [ ] 1.2 Run `cargo test --workspace` (serial if needed) and record the actual test count
- [ ] 1.3 Update `CLAUDE.md`, `README.md`, `docs/NEO4J_COMPATIBILITY_REPORT.md` with the real numbers + one canonical "status" section
- [ ] 1.4 Reconcile `docs/ROADMAP.md` phase dates with `CHANGELOG.md` entries
- [ ] 1.5 Grep `rg -n 'Production Ready|100% compatible|2949\+ tests'` and decide per-hit: keep with link, rewrite, or delete

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 The doc edits themselves are the output; ensure they link to the test run that produced the numbers
- [ ] 2.2 Add a CI job (or note in `.github/workflows/`) that reruns the compatibility suite weekly and posts the delta
- [ ] 2.3 Run `cargo doc --workspace --no-deps` to make sure no `//!` references break
