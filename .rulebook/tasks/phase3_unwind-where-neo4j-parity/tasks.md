## 1. Parser rejection

- [ ] 1.1 In `nexus-core/src/executor/parser/clauses.rs::parse_clause` (line 181 area), remove the standalone `"WHERE"` arm so bare WHERE stops being accepted as a top-level clause. Replace with an explicit error arm that returns `Error::CypherParse` with the Neo4j-style message: `Invalid input 'WHERE': expected 'ORDER BY', 'CALL', 'CREATE', 'LOAD CSV', 'DELETE', 'DETACH', 'FINISH', 'FOREACH', 'INSERT', 'LIMIT', 'MATCH', 'MERGE', 'NODETACH', 'OFFSET', 'OPTIONAL', 'REMOVE', 'RETURN', 'SET', 'UNION', 'UNWIND', 'USE', 'WITH' or <EOF>`. Include line + column in the error.
- [ ] 1.2 Verify `parse_match_clause` (at line 1172) and `parse_with_clause` still call `parse_where_clause` for their own attached-WHERE handling — those paths are spec-compliant and must keep working. Add a unit test in `nexus-core/src/executor/parser/tests.rs` that asserts `MATCH (n) WHERE n.x = 1 RETURN n` and `WITH x WHERE x > 0 RETURN x` still parse cleanly.
- [ ] 1.3 Add a parser-reject unit test: `UNWIND [1,2,3] AS x WHERE x > 1 RETURN x` now returns `Err(Error::CypherParse(...))` with the Neo4j-style message. Same for `CREATE (n:X) WHERE …` and `DELETE n WHERE …` (every currently-accepted standalone-WHERE position).

## 2. Internal test migration

- [ ] 2.1 Rewrite `nexus-core/tests/unwind_tests.rs` line 126 (`UNWIND [1, 2, 3, 4, 5] AS num WHERE num > 2 RETURN num`) to `UNWIND [1, 2, 3, 4, 5] AS num WITH num WHERE num > 2 RETURN num`. Confirm result unchanged — `[3, 4, 5]`.
- [ ] 2.2 Rewrite `nexus-core/tests/unwind_tests.rs` line 159 (`MATCH (p:Person) UNWIND p.tags AS tag WHERE tag = 'developer' OR tag = 'designer' RETURN p.name, tag ORDER BY p.name`) to insert `WITH p, tag` between UNWIND and WHERE. Verify result parity against the scalar expectation.
- [ ] 2.3 Rewrite `nexus-core/tests/unwind_tests.rs` line 348 (`UNWIND [1, null, 3, null, 5] AS x WHERE x IS NOT NULL RETURN x`) to `UNWIND [1, null, 3, null, 5] AS x WITH x WHERE x IS NOT NULL RETURN x`. Result stays `[1, 3, 5]`.
- [ ] 2.4 Audit `nexus-core/tests/executor_comprehensive_test.rs` for any remaining `UNWIND … WHERE …` shorthand; rewrite each with a `WITH` projection of the exact variables the WHERE predicate reads.
- [ ] 2.5 `cargo +nightly test --workspace` must stay green end-to-end — every suite that was green before the parser tightening stays green after the migration. Any regression beyond the shorthand rejections is a bug in the migration, not an accepted casualty.

## 3. Compat test update

- [ ] 3.1 Rewrite `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` line 737 query from `UNWIND [1, 2, 3, 4, 5] AS x WHERE x > 2 RETURN x` to `UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x`. Keep the test name `14.05 UNWIND with WHERE` — the migration is transparent to readers.
- [ ] 3.2 Full compat run against live Neo4j 2025.09.0 + release Nexus: expected **300/300** pass (up from 299/300). This is the headline deliverable of this task.
- [ ] 3.3 Capture the raw runner output tail showing the final summary line with `Total Tests: 300 / Passed: 300 / Failed: 0 / Pass Rate: 100%` in the archive commit message.

## 4. Docs + changelog

- [ ] 4.1 `CHANGELOG.md` entry under the next version's `Fixed` (or `Changed` if we treat it as breaking): "Parser now rejects bare `WHERE` as a top-level clause. WHERE must attach to `MATCH` / `OPTIONAL MATCH` / `WITH`. The previously-accepted shorthand `UNWIND … AS x WHERE …` now requires an intermediate `WITH x`. Matches Neo4j 2025.09.0 grammar." Point readers at the migration example.
- [ ] 4.2 `docs/specs/cypher-subset.md` — add a line under the clause-ordering section noting that `WHERE` is only valid inside MATCH / OPTIONAL MATCH / WITH, with the migration snippet. Cross-link to this task's archive entry.
- [ ] 4.3 Verify `docs/specs/cypher-subset.md` doesn't currently advertise the shorthand as a supported form; if it does, correct it.

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 5.1 Update or create documentation covering the implementation (`CHANGELOG.md` entry + `docs/specs/cypher-subset.md` clause-ordering note).
- [ ] 5.2 Write tests covering the new behavior (parser-reject tests per §1.3 + migrated queries in §2 + the full workspace test suite staying green + 300/300 compat run in §3.2).
- [ ] 5.3 Run tests and confirm they pass: `cargo +nightly test --workspace`, `cargo +nightly clippy --workspace --all-targets -- -D warnings`, `cargo +nightly fmt --all -- --check`, and the Neo4j compat suite at 300/300.
