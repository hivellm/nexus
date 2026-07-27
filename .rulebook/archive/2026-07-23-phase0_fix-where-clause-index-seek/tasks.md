# Tasks: phase0_fix-where-clause-index-seek

WHERE-form equality on an indexed property (`MATCH (n:Person) WHERE n.age = 30`)
full-scanned while the inline form `MATCH (n:Person {age:30})` seeked the index
(#5); the unindexed-access notification only fired for `Equal` (#6); composite
indexes were never probed for inline multi-property selectors (#7); the Join
cost estimator fed a COST value into a CARDINALITY variable (#8). Landed across
two commits: f8eb857e (#5+#6) and 67eba262 (#7+#8).

## 1-2. Reproduce (#5 seek miss + #6 notification baseline)
- [x] 1.1/1.2/1.3 Confirmed WHERE-form equality plans a scan (not `NodeIndexSeek`)
  while inline-form seeks; failing tests written and confirmed failing pre-fix
- [x] 2.1/2.2 Confirmed range/IN/STARTS WITH/CONTAINS emit NO unindexed
  notification today (only `Equal` did)

## 3. Fix #5 — plan-time index seek for WHERE-form equality
- [x] 3.1 DECISION: plan-time lift (not the `try_index_based_filter` executor
  path) — matches the plan-shape test and mirrors `node_index_seek_for`
- [x] 3.2/3.3/3.4 Decompose each WHERE into AND-conjuncts, lift a
  `var.prop = <literal>` conjunct (either operand order) on an indexed
  `(label, prop)` into `NodeIndexSeek` (constant `value`, `key_expression=None`),
  drop it from the residual Filter; OPTIONAL MATCH WHERE skipped. SCOPE: equality
  with a plan-time literal only — range/IN/string SEEKING and `$parameter`
  equality deferred (see follow-up). Results unchanged (verified)

## 4. Fix #6 — complete the unindexed-access notification
- [x] 4.1/4.2 Extended `unindexed.rs` to notify for range (>,<,>=,<=), IN,
  STARTS WITH, CONTAINS; equality-on-indexed now SILENT (seeked by #5)

## 5. Fix #7 — composite-index detection for inline multi-property selectors
- [x] 5.1/5.2/5.3 IMPLEMENTED (not deferred): `CompositeBtreeRegistry` already
  existed + was wired into the executor, but the planner had no handle and
  `CompositeBtreeSeek` was never constructed anywhere. Threaded the registry into
  `QueryPlanner` via `with_composite_index` (struct field, UNION sub-planner copy,
  production `plan_ast` wiring) and added `composite_index_seek_for`: emits the
  seek only when every declared key of a registered composite index is present as
  a plan-time literal (index key order), preferred over single-property; a partial
  selector never mis-seeks (falls through to scan). Full + partial + parity tests

## 6. Fix #8 — Join cost estimator category error
- [x] 6.1/6.2/6.3 Replaced `estimate_plan_cost(&[left/right])` with
  `estimate_operator_cardinality` in the Join Inner arm (mirrors Union); test
  asserts cardinality (29) not the cost-inflated value (348)

## 7. Tail (docs + tests — check or waive with tailWaiver)
- [x] 7.1 Update or create documentation covering the implementation — CHANGELOG
  entry ("query planner index usage and diagnostics") + `docs/specs/cypher-subset.md`
  corrected (the line claiming WHERE-form doesn't seek was now inaccurate)
- [x] 7.2 Write tests covering the new behavior — `where_clause_index_seek_test.rs`
  (9), `composite_index_seek_test.rs` (4 E2E), and planner unit tests (composite
  full/partial/preference, join-cost-from-cardinality)
- [x] 7.3 Run tests and confirm they pass — fmt + clippy clean (pre-commit hooks);
  full workspace green both passes (the lone gate failures were the known Windows
  Tantivy fulltext flakes, passing in isolation); cypher 370 / executor 208 /
  regression 141 unchanged

## Deferred / follow-up
- Range/IN/STARTS WITH/CONTAINS SEEKING and `$parameter` equality seeking (only
  literal equality seeks today; the rest notify), and the EXPLAIN/PROFILE display
  planners not wiring `property_index`/`composite_index` (pre-existing display-
  accuracy gap surfaced during #7) → filed as
  `phase0_fix-where-clause-index-seek-extensions`.

## Related
- `phase0_fix-correlated-predicate-index-seek` — sibling correlated-predicate
  index bypass in the same code area
