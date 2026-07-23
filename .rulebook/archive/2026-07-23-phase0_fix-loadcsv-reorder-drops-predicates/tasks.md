# Tasks: phase0_fix-loadcsv-reorder-drops-predicates

`optimize_operator_order` (`crates/nexus-core/src/executor/planner/queries/cost.rs`)
leaves `Operator::LoadCsv` in the `_ => others` catch-all, which is recombined
AFTER `filters`. `LoadCsv` binds a fresh per-row `variable` (types.rs:611-621)
just like `Unwind`, so any `WHERE` predicate on that variable — or a correlated
`MATCH` seeded from it — runs before the binding exists, evaluates the unbound
variable as `Null` (falsy), and silently drops every row. The
`unwind_before_scan` detection loop (cost.rs:567-598) recognizes only
`Operator::Unwind`, so `LOAD CSV ... MATCH (n {id: row.id})` also misses the
binder-before-scan guard.

Reproduce the loss first (§1), confirm the mechanism (§2), then fix the
bucketing + detection (§3) — mirroring `phase0_fix-plan-reorder-drops-predicates`.

## 1. Reproduce the loss first
- [x] 1.1 Behavioral failing test added
  (`tests/loader/load_csv_test.rs::test_load_csv_where_on_row_variable_keeps_matching_rows`):
  `LOAD CSV ... AS row MATCH (n:Person) WHERE row[0] = n.id RETURN n.id`.
  Confirmed today it returns `Null` (predicate dropped) instead of the match.
  (Parser rejects `WITH HEADERS`-after-`LOAD CSV` and bare `WHERE` after
  LOAD CSV, so the shape uses `MATCH ... WHERE row[0] = ...` with array indexing)
- [x] 1.2 Plan-order failing test
  (`tests/executor/plan_binding_operator_order_test.rs::load_csv_precedes_filter_on_its_bound_row_variable`):
  confirmed today's plan is `[NodeByLabel, Filter, LoadCsv, Project]` (Filter first)
- [x] 1.3 Correlated-seek plan-order test
  (`::load_csv_precedes_correlated_index_seek_on_row_variable`): with an index on
  `:Person(id)`, `MATCH (n:Person {id: row.id})` compiles to
  `[NodeIndexSeek, Filter, LoadCsv, Project]` — seek misordered before `LoadCsv` today

## 2. Confirm the mechanism
- [x] 2.1 Confirmed `Operator::LoadCsv` fell through `_ => others.push(operator)`
- [x] 2.2 Confirmed the `unwind_before_scan` loop matched only `Operator::Unwind`
- [x] 2.3 Confirmed `LoadCsv` binds `row` independently of upstream input
  (planner_core.rs:563; created regardless of input), and an unbound var in a
  `Filter` evaluates to `Null` -> `false` (empirically: query returned `Null`)

## 3. Fix the ordering
- [x] 3.1 Added `Operator::LoadCsv { .. }` to the `unwind_before_scan` detection
  loop alongside `Operator::Unwind` (cost.rs)
- [x] 3.2 Routed `Operator::LoadCsv` into the `unwinds` bucket (recombined before
  `filters` in both branches; not cost-reordered like `scans`, so `LOAD CSV`/
  `UNWIND` relative order is preserved)
- [x] 3.3 §1 tests pass; executor group 210/0, loader group 16/0

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation —
  `docs/specs/cypher-subset.md` LOAD CSV planning note + CHANGELOG entry
- [x] 4.2 Write tests covering the new behavior — 2 plan-order guards
  (Filter + correlated `NodeIndexSeek`) plus the behavioral `WHERE`-on-row case
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all` +
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` green;
  full `cargo +nightly test --workspace` run to confirm
