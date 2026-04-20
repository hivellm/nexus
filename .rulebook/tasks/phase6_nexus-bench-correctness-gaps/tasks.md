## 1. Composite `:Label {prop: value}` filter (HIGH)

- [x] 1.1 Engine-level regression test — `match_scopes_by_label_and_property_together` in `crates/nexus-core/src/engine/tests.rs`. **Surprising finding**: the simple two-label synthetic case passes. The bench reproducer (TinyDataset + SmallDataset, 103 total edges) still fails. The bug is data-shape-sensitive — `traversal.small_one_hop_hub` reproducing at bench size while this synthetic test passes means the fix lives downstream of "does the planner understand composite filter" and probably in "how is the composite filter applied when the cardinality estimate favours a full scan". Next action shifts to §1.2 diagnosis with the bench-scale data loaded
- [ ] 1.2 Trace the pattern-walker in `crates/nexus-core/src/executor` — identify whether label + property are AND-ed at plan time or the property filter is silently dropped when a label is present
- [ ] 1.3 Fix the narrowest layer and assert the scenario catalogue's `traversal.small_one_hop_hub` matches Neo4j on the next bench run

## 2. Variable-length path `*m..n` (HIGH)

- [ ] 2.1 Engine-level regression test: `MATCH (a:P {id: 0})-[:KNOWS*1..3]->(n) RETURN count(DISTINCT n)` returns 15 on SmallDataset
- [ ] 2.2 Also run the relaxed version `MATCH (a)-[:KNOWS*1..3]->(n)` starting from a node with a known id to isolate whether the bug is in the anchor or the path expansion
- [ ] 2.3 Fix the variable-length operator and confirm `traversal.small_var_length_1_to_3` matches Neo4j on the next bench run

## 3. `db.*` catalog procedures return empty yield (MEDIUM)

- [ ] 3.1 Engine-level regression test for `db.labels()` — on a two-dataset load (TinyDataset + SmallDataset) it yields 6 labels (A, B, C, D, E, P)
- [ ] 3.2 Same shape of test for `db.relationshipTypes()` (expects 1: KNOWS) and `db.propertyKeys()` (expects at least `id`, `name`, `score`)
- [ ] 3.3 Walk the procedure dispatch and YIELD wiring; identify whether the procedure body emits no rows or the YIELD plumbing drops them
- [ ] 3.4 Additionally: `CALL db.indexes() YIELD *` errors at parse time (column 25) — teach the parser `YIELD *` or accept the procedure's column list
- [ ] 3.5 Fix + re-run bench; `procedure.db_labels`, `procedure.db_relationship_types`, `procedure.db_property_keys`, `procedure.db_indexes` all content-match Neo4j

## 4. Integer arithmetic promoted to float (LOW)

- [ ] 4.1 Engine-level regression: `RETURN 1 + 2 * 3 AS n` returns `NexusValue::Int(7)`, not `NexusValue::Float(7.0)`
- [ ] 4.2 Same assertion for other integer-only expressions (`RETURN 10 - 4`, `RETURN 100 / 4`)
- [ ] 4.3 Fix the expression evaluator so the result type follows Cypher rules (integer stays integer until a float operand is introduced)
- [ ] 4.4 Re-run bench; `scalar.arithmetic` content-matches Neo4j

## 5. `WITH` → `RETURN <expr>` projection drop (MEDIUM — three scenarios)

- [ ] 5.1 Engine-level regression: `MATCH (n) WITH count(n) AS total, max(n.score) AS hi RETURN hi > 0.99 AS any_high` returns one `Bool(false)` row, not the WITH projection raw
- [ ] 5.2 Engine-level regression: `MATCH (n:A) WITH collect(n.id) AS ids RETURN size(ids) AS s` returns `Number(20)`, not the raw list
- [ ] 5.3 Engine-level regression: `MATCH (n:A) WITH n.score AS s WHERE s > 0.1 RETURN count(*) AS c` returns one row, not zero
- [ ] 5.4 Trace the planner's WITH → RETURN chain — likely the RETURN expression is being discarded in favour of the WITH projection's column set
- [ ] 5.5 Fix and confirm `subquery.exists_high_score` + `subquery.size_of_collect` + `subquery.with_filter_count` all content-match Neo4j

## 6. Float-accumulation order in `avg()` (LOW — diagnostic)

- [ ] 6.1 Decide the fix direction: Kahan summation in `sum()` / `avg()`, or a per-ULP epsilon in the divergence guard, or document-and-accept
- [ ] 6.2 Apply the chosen direction; if Kahan, add a regression test asserting the `:A` label's avg score is numerically stable across run invocations
- [ ] 6.3 Re-run bench; `aggregation.avg_score_a` content-matches Neo4j (or is normalised by the guard)

## 7. `ORDER BY` null-positioning inverted (MEDIUM — two scenarios)

- [ ] 7.1 Engine-level regression: seed nodes with + without a `score` property, `MATCH (n) RETURN n.name ORDER BY n.score DESC LIMIT 5` — null-score rows appear first
- [ ] 7.2 Engine-level regression: same seed, `MATCH (n) RETURN n.name ORDER BY n.score ASC LIMIT 5` — null-score rows appear last
- [ ] 7.3 Audit the planner's ORDER BY operator comparator; flip the null-polarity so DESC puts nulls first and ASC puts them last per openCypher
- [ ] 7.4 Re-run bench; `order.top_5_by_score` AND `order.bottom_5_by_score` both content-match Neo4j

## 8. `DELETE` rejects CREATE→WITH-flow node bindings (MEDIUM)

- [ ] 8.1 Engine-level regression: `CREATE (n:BenchCycle) WITH n DELETE n RETURN 'done' AS status` succeeds with one row (status='done')
- [ ] 8.2 Widen to other upstream clauses: UNWIND-produced bindings flowing into DELETE, CALL subquery returning nodes to DELETE. Confirm the clause-context check is uniformly "any node binding", not "MATCH-only"
- [ ] 8.3 Fix the parser / planner and re-run bench; `write.create_delete_cycle` executes without error and content-matches Neo4j

## 9. Re-run + publish

- [ ] 6.1 After each §1-§5 fix, rebuild `target/release/nexus-server.exe` and rerun `target/release/nexus-bench.exe --rpc-addr 127.0.0.1:15475 --neo4j-url bolt://127.0.0.1:7687 --compare --i-have-a-server-running --load-dataset --format both --output target/bench/report`
- [ ] 6.2 Update the "Bench table" section of `proposal.md` with the fresh classification counts and the per-scenario p50s on the rows the fix touched; note which scenarios still diverge
- [ ] 6.3 Final run: zero content-divergent scenarios. The harness's 9 `#[ignore]` comparative tests all still pass as a single `cargo test --features live-bench,neo4j -- --ignored --test-threads=1` batch

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 7.1 Update or create documentation — CHANGELOG entry per fix under `1.0.0 → Fixed`; mention in `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` if any fix closes a documented gap
- [ ] 7.2 Write tests covering the new behavior — §1.1 / §2.1 / §3.1-3.2 / §4.1 / §5.1 above
- [ ] 7.3 Run tests and confirm they pass — `cargo +nightly test --workspace` + the comparative bench `#[ignore]` suite (strict content parity) in a single invocation
