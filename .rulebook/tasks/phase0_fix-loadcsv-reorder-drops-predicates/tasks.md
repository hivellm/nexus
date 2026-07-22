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
- [ ] 1.1 Write a failing test: `LOAD CSV FROM '<file>' AS row MATCH (n) WHERE
  row.<col> = n.<prop> RETURN n` (or the simplest `WHERE row.x = '...'` shape
  that reaches a `Filter` on the row variable) returns rows that satisfy the
  predicate. Confirm it returns 0 rows (or unfiltered rows) today
- [ ] 1.2 Write a failing plan-order test: assert the compiled plan places the
  `LoadCsv` operator before the `Filter` that references the row variable.
  Confirm today's plan places `Filter` first (per cost.rs bucketing/recombine)
- [ ] 1.3 If a correlated-seek shape is reachable
  (`LOAD CSV ... AS row MATCH (n:Label {id: row.id})` with an index on
  `:Label(id)`), add a plan-order test asserting `LoadCsv` precedes the
  `NodeIndexSeek` it feeds; confirm it is misordered today

## 2. Confirm the mechanism
- [ ] 2.1 Trace the bucketing match (cost.rs:603-635): confirm `Operator::LoadCsv`
  falls through `_ => others.push(operator)` rather than a before-filters bucket
- [ ] 2.2 Trace the `unwind_before_scan` detection loop (cost.rs:567-598):
  confirm it matches only `Operator::Unwind`, so a `LOAD CSV` preceding a scan
  is not detected
- [ ] 2.3 Confirm `LoadCsv` binds its `variable` independently of upstream rows
  (types.rs:611-621 + its executor) and that an unbound variable in a `Filter`
  evaluates to `Null` -> `false` (`eval/predicate.rs:490`), i.e. a silent no-op

## 3. Fix the ordering
- [ ] 3.1 Add `Operator::LoadCsv { .. }` to the `unwind_before_scan` detection
  loop (cost.rs:567-598) alongside `Operator::Unwind`
- [ ] 3.2 Route `Operator::LoadCsv` into a bucket recombined BEFORE `filters`
  (extend the `unwinds` arm or add a dedicated bucket ordered with `unwinds` in
  both recombine branches); preserve correct relative order of `LOAD CSV` vs
  `UNWIND` in the same query
- [ ] 3.3 Make the §1 tests pass; re-run to confirm the predicate and any
  correlated-seek ordering are now correct

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation
  (`docs/specs/cypher-subset.md` LOAD CSV / planning section if present;
  CHANGELOG entry)
- [ ] 4.2 Write tests covering the new behavior (the §1 regression tests plus a
  combined `LOAD CSV` + correlated `MATCH` case)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)
