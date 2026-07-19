# phase7_opencypher-gap-closure — tasks

Order matters: measure first (TCK), then close grammar/execution gaps, then re-measure
and update every doc that states a compatibility number. Items 1.x are the dependency
spine; do not reorder.

## 1. Implementation
- [ ] 1.1 Vendor the official openCypher TCK feature files under `crates/nexus-core/tests/tck/opencypher/` (pin the upstream commit in a README; keep the existing Nexus-authored spatial corpus separate)
- [ ] 1.2 Extend the existing Gherkin harness (`crates/nexus-core/tests/tck/spatial/` runner) into a generic TCK runner: side-effect assertions, expected-error assertions, and an `#[ignore]`-tagged category skip-list for known-unsupported areas
- [ ] 1.3 Produce the baseline TCK pass-rate report (per-category pass/fail/skip counts) and commit it as `docs/compatibility/OPENCYPHER_TCK_REPORT.md`; wire a script entry (`scripts/compatibility/run-opencypher-tck.ps1` or cargo alias) so it is reproducible
- [ ] 1.4 Read-side dynamic labels/rel-types — parser: accept `:$param` in node and relationship patterns (`crates/nexus-core/src/executor/parser/` ast.rs Pattern + mod.rs), mirroring the write-side representation shipped in phase6_opencypher-advanced-types
- [ ] 1.5 Read-side dynamic labels/rel-types — planner/executor: resolve the parameter to label/type IDs at execution start (catalog lookup), fall back to AllNodesScan+Filter when the parameter is a LIST; error `ERR_DYNAMIC_LABEL_TYPE` on non-STRING/LIST values
- [ ] 1.6 `UNION` / `UNION ALL` inside `CALL { }`: parser accepts a union body in CallSubqueryClause (`ast.rs`), planner wires branches through the existing Union operator with per-branch variable-scope validation (all branches must export identical columns)
- [ ] 1.7 `CREATE CONSTRAINT ... FOR (n:L) REQUIRE n.p IS UNIQUE | IS NOT NULL` (and relationship form `FOR ()-[r:T]-() REQUIRE ...`): grammar in parser mapped onto the existing CreateConstraint AST/machinery; keep the legacy syntax working; remove the "Unsupported" entry from `docs/specs/cypher-subset.md`
- [ ] 1.8 Re-run the TCK after 1.4–1.7 and refresh `docs/compatibility/OPENCYPHER_TCK_REPORT.md`; update the compatibility claims in CLAUDE.md and `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` to the measured number (replace "~55%")

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation (cypher-subset.md sections for dynamic labels, CALL+UNION, FOR...REQUIRE; TCK report + how-to-run)
- [ ] 2.2 Write tests covering the new behavior (unit tests per parser/planner change, incremental 1–3 at a time, plus TCK scenarios exercising each feature)
- [ ] 2.3 Run tests and confirm they pass (`cargo +nightly test --workspace` green, TCK runner green on non-skipped categories, clippy `-D warnings` clean)

## Related (tracked elsewhere — do NOT duplicate here)
- QPP slice 2 (named/labelled bodies) → phase6_opencypher-quantified-path-patterns
- CALL { } IN TRANSACTIONS executor batching → phase6_opencypher-subquery-transactions
- Geospatial predicate execution → phase6_opencypher-geospatial-predicates
- USING INDEX/SCAN/JOIN hint enforcement → phase7_planner-using-index-hints
