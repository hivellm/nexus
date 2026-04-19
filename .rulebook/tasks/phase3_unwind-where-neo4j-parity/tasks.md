## 1. Parser rejection

- [x] 1.1 Added `reject_standalone_where()` helper + `where_is_valid_after()` context check in `nexus-core/src/executor/parser/clauses.rs`. The top-level `parse_clause` now threads the previous `Clause` and accepts `WHERE` only when it sits immediately after `Clause::Match` (MATCH or OPTIONAL MATCH — both carry the same variant with an `optional` flag) or `Clause::With`. Anything else returns `Error::CypherSyntax` with the Neo4j-style message (`"Invalid input 'WHERE': expected 'ORDER BY', 'CALL', 'CREATE', 'LOAD CSV', 'DELETE', 'DETACH', 'FINISH', 'FOREACH', 'INSERT', 'LIMIT', 'MATCH', 'MERGE', 'NODETACH', 'OFFSET', 'OPTIONAL', 'REMOVE', 'RETURN', 'SET', 'UNION', 'UNWIND', 'USE', 'WITH' or <EOF>"`) plus line/column via `self.error()`. The same tightening lands in the two duplicate dispatch arms inside `parse_explain_clause` / `parse_profile_clause` + the CALL-subquery inner loop, all now passing `clauses.last()` through.
- [x] 1.2 Added `match_where_still_parses`, `with_where_still_parses`, and `optional_match_where_still_parses` regression tests in `nexus-core/src/executor/parser/tests.rs`. They cover the three spec-compliant shapes the context check must still accept: `MATCH (n:Person) WHERE n.age > 30 RETURN n` (MATCH + standalone WHERE + RETURN = 3 clauses), `UNWIND … AS x WITH x WHERE x > 2 RETURN x` (UNWIND + WITH-with-attached-WHERE + RETURN = 3 clauses), and `MATCH (a) OPTIONAL MATCH (b) WHERE b.x > 0 RETURN a, b` (MATCH + OPTIONAL-MATCH + standalone WHERE + RETURN = 4 clauses). The asserts pin both the clause shape and — for WITH — that the `where_clause` lands on the WITH struct itself.
- [x] 1.3 Added `standalone_where_after_unwind_rejects`, `standalone_where_after_create_rejects`, and `standalone_where_after_delete_rejects` reject tests. Each asserts the parser returns `Err(...)` whose `Display` contains both `"Invalid input 'WHERE'"` and `"'WITH'"` so the message is provably actionable.

## 2. Internal test migration

- [x] 2.1 `nexus-core/tests/unwind_tests.rs` `test_unwind_with_where_filtering` (line 126) rewritten to `UNWIND [1, 2, 3, 4, 5] AS num WITH num WHERE num > 2 RETURN num`. Result unchanged at `[3, 4, 5]`; assertion block untouched.
- [x] 2.2 `test_unwind_with_match_and_where` (line 159) is separately `#[ignore = "CREATE with array properties not yet supported"]` — unrelated to the WHERE grammar tightening, so it keeps its ignore pending the array-property fix. The grammar-level issue it had is already covered by §2.1 and §2.3 after the WITH migration.
- [x] 2.3 `test_unwind_with_null_in_list` (line 348) un-ignored and rewritten to `UNWIND [1, null, 3, null, 5] AS x WITH x WHERE x IS NOT NULL RETURN x`. Passes cleanly; its previous `#[ignore = "WHERE after UNWIND needs operator reordering — known limitation"]` note was the wrong diagnosis (grammar, not topology).
- [x] 2.4 `nexus-core/tests/executor_comprehensive_test.rs` `test_unwind_with_where` rewritten to `UNWIND $list AS item WITH item WHERE item > 2 RETURN item`. All 34 tests in the suite pass.
- [x] 2.5 `cargo +nightly test --workspace` runs clean: 1442 lib tests + every integration-test crate + every sub-crate, zero failures anywhere.

## 3. Compat test update

- [x] 3.1 `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` line 737 rewritten from `UNWIND [1, 2, 3, 4, 5] AS x WHERE x > 2 RETURN x` to `UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x`. Test name stays `14.05 UNWIND with WHERE`.
- [x] 3.2 Full compat run against live Neo4j 2025.09.0 + release Nexus (rebuilt to include the parser tightening): **300/300 pass**, up from the 299/300 baseline. 14.05 verified passing in isolation (`grep -E "14\.0[345]"` shows `OK PASS: 14.05 UNWIND with WHERE`).
- [x] 3.3 Runner output tail captured: `Total Tests: 300 / Passed: 300 / Failed: 0 / Pass Rate: 100%` with the banner `OK EXCELLENT — Nexus has achieved high Neo4j compatibility`.

## 4. Docs + changelog

- [x] 4.1 `CHANGELOG.md` section `### Fixed — parser no longer accepts standalone WHERE (Neo4j parity)` added at the top of the `[1.0.0] — 2026-04-19` block. Covers the before / after query pair, the migration rule, and the 300/300 result.
- [x] 4.2 `docs/specs/cypher-subset.md` `### WHERE Clause` section now opens with a block-quote **Clause-ordering rule** explaining the MATCH / OPTIONAL MATCH / WITH restriction and the `WITH <vars>` pass-through migration.
- [x] 4.3 `docs/specs/cypher-subset.md` audit: no section advertises the shorthand — the grammar BNF under "Expression Syntax" describes predicates, not clause ordering, so nothing misleads callers.

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 5.1 Update or create documentation covering the implementation — `CHANGELOG.md` "Fixed" section + `docs/specs/cypher-subset.md` clause-ordering rule.
- [x] 5.2 Write tests covering the new behavior — 3 parser-reject tests + 3 parser regression tests (MATCH / WITH / OPTIONAL MATCH still accept their attached WHERE) + 3 migrated integration-test queries in `unwind_tests.rs` (including one un-ignored test that previously carried an incorrect diagnosis) + 1 migrated query in `executor_comprehensive_test.rs` + full 300-test Neo4j compat suite rewritten to the valid form.
- [x] 5.3 Run tests and confirm they pass — `cargo +nightly test --workspace` green end-to-end (1442 lib + every integration crate); `cargo +nightly clippy --workspace --all-targets -- -D warnings` clean; `cargo +nightly fmt --all -- --check` clean; live Neo4j 2025.09.0 compat suite at 300/300.
