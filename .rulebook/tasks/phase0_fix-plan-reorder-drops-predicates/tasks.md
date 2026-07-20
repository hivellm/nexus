# Tasks: phase0_fix-plan-reorder-drops-predicates

`optimize_operator_order` buckets `NodeIndexSeek`, `VariableLengthPath`, and
`QuantifiedExpand` into a catch-all `others` bucket
(`cost.rs:610-612`) that is recombined **after** `filters` and
`optimized_joins` (`cost.rs:654-659`), even though these operators bind the
variables the filters (and downstream expansions) depend on. Any WHERE
predicate — or `Expand` — that references a variable bound by one of these
operators is silently dropped (runs against empty input) once the binding
operator reappears downstream and regenerates its full, unfiltered result.
Trigger: with `CREATE INDEX FOR (n:Person) ON (n.id)`,
`MATCH (n:Person {id:'b'}) WHERE n.age > 30 RETURN n` returns rows that
violate `age > 30`.

Order matters: prove the drop with failing tests across all three affected
operator kinds (§1) before touching the bucketing logic (§3), and confirm the
exact mechanism (§2) — the bucketing match and the recombine order — so the
fix closes the same gap for every current binding operator, not just the one
in the reproduction case.

## 1. Reproduce the loss first
- [ ] 1.1 Write a failing integration test: with `CREATE INDEX FOR (n:Person)
  ON (n.id)`, seed `:Person` nodes including one with `id:'b'` and
  `age <= 30`, run `MATCH (n:Person {id:'b'}) WHERE n.age > 30 RETURN n`, and
  assert it returns 0 rows. Confirm it fails today (the `age<=30` row is
  returned too)
- [ ] 1.2 Write a failing test for the Expand case: with the same index,
  `MATCH (a:Person {id:'x'})-[:R]->(b) RETURN b` — assert the compiled plan
  places `NodeIndexSeek(a)` before `Expand`. Confirm today's plan places
  `Expand` before `NodeIndexSeek(a)` (inspect the plan's operator order
  directly, per `cost.rs:590-614`/`:638-660`)
- [ ] 1.3 Write a failing test for the VariableLengthPath case:
  `MATCH (a:A)-[:R*1..2]->(b) WHERE b.name='x' RETURN b` against a graph
  where `b` is reachable and matches — assert it returns the row. Confirm it
  returns 0 rows today
- [ ] 1.4 Confirm `engine/tests/indexes.rs:202-217` passes unchanged despite
  the misordering (it asserts operator presence, not order) — this is why
  the regression has no existing coverage

## 2. Confirm the mechanism
- [ ] 2.1 Trace `optimize_operator_order`'s bucketing match
  (`cost.rs:590-614`): confirm `NodeIndexSeek`, `VariableLengthPath`, and
  `QuantifiedExpand` fall through the `_ => others.push(operator)` arm
  (`:610-612`) rather than the `scans`/`expansions` arms
- [ ] 2.2 Trace the recombine order (`cost.rs:638-660`): confirm `others` is
  appended after `filters` and `optimized_joins` in both the
  `unwind_before_scan` and normal branches, so any operator depending on an
  `others`-bucketed operator's output runs first
- [ ] 2.3 Confirm `seed_scan_variable` (`operators/dispatch.rs:497-539`)
  regenerates the full result set independently of upstream rows, and that
  `evaluate_predicate_on_row` (`eval/helpers.rs:868-876`) treats a
  missing/unbound variable as `Null` → `false`, so a Filter placed before its
  binding operator is a silent no-op rather than a hard error
- [ ] 2.4 Confirm reachability: this path requires a real property index
  installed (`dispatch.rs:1301-1303`); note why planner unit tests built via
  `QueryPlanner::new` (which install no index) never exercised it, so the
  fix's test coverage must install a real index

## 3. Fix the ordering
- [ ] 3.1 Add `NodeIndexSeek`, `CompositeBtreeSeek`, `SpatialSeek`,
  `VariableLengthPath`, and `QuantifiedExpand` to the scan/expand buckets in
  `optimize_operator_order` (`cost.rs:590-614`) so they recombine before
  `filters` (`:638-660`) in both branches
- [ ] 3.2 Evaluate making `optimize_operator_order` order-preserving with
  respect to variable dependencies (a topological ordering by bound/consumed
  variables) instead of enumerating operator kinds by name; implement it if
  scope allows — a hand-maintained category-bucket list will silently regress
  again for the next binding operator kind added. If out of scope for this
  fix, record the trade-off in the proposal's "What Changes" section
- [ ] 3.3 Strengthen `engine/tests/indexes.rs:202-217` (or add a companion
  test) to assert the compiled operator **order**, not just presence, so this
  class of regression fails CI going forward
- [ ] 3.4 Make the §1 tests pass; re-run them to confirm the predicate,
  Expand-ordering, and VariableLengthPath results are now correct

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md` if it documents planner
  operator-ordering guarantees; add a CHANGELOG entry
- [ ] 4.2 Tests: the three §1 regression tests pass; the strengthened
  `indexes.rs` order assertion passes; add a case combining an index seek
  with a downstream Expand that references the seek's variable
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-correlated-predicate-index-seek`, `phase0_fix-where-clause-index-seek`
  — other planner/index-seek defects in the same area
