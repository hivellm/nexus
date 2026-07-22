# Tasks: phase0_fix-plan-reorder-drops-predicates

`optimize_operator_order` buckets binding operators into a catch-all `others`
bucket that is recombined **after** `filters`, even though these operators bind
the variables the filters (and downstream expansions) depend on. Any WHERE
predicate — or `Expand` — that references a variable bound by one of these
operators is silently dropped (runs against empty input) once the binding
operator reappears downstream and regenerates its full, unfiltered result.

NOTE (state at implementation time): a prior fix
(`phase0_fix-correlated-predicate-index-seek`) had ALREADY moved `NodeIndexSeek`
and `CompositeBtreeSeek` into the `scans` bucket, so the spec's original
`NodeIndexSeek` reproduction (§1.1) and the Expand-before-seek case (§1.2)
already return correct results today. The remaining live-reachable, still-broken
binders were `VariableLengthPath`, `QuantifiedExpand`, and `SpatialSeek`, all
still falling into `others`. This fix closes those; the `NodeIndexSeek` case is
retained as a regression lock.

## 1. Reproduce the loss first
- [x] 1.1 `NodeIndexSeek` predicate-drop case — ALREADY FIXED by prior work
  (`NodeIndexSeek` is in `scans`), so it returns 0 rows correctly today. Kept as
  a passing regression lock
  (`node_index_seek_precedes_filter_and_correctly_returns_no_rows` in
  `crates/nexus-core/tests/executor/plan_binding_operator_order_test.rs`)
- [x] 1.2 Expand-after-seek ordering — ALREADY CORRECT today (`NodeIndexSeek` in
  `scans` recombines before `expansions`). The equivalent plan-order guard is
  provided for the still-broken binders (VariableLengthPath / QuantifiedExpand)
  instead, which is where the defect actually lived
- [x] 1.3 Failing VariableLengthPath test written and confirmed failing pre-fix:
  `MATCH (a:A)-[:R*1..2]->(b) WHERE b.name='x' RETURN b` returned 0 rows before
  the fix (`variable_length_path_filter_on_bound_target_returns_matching_row`);
  plus a discriminating QuantifiedExpand behavioral test using an adversarial
  decoy seed (`quantified_expand_filter_runs_against_its_bound_target_not_a_stale_binding`)
- [x] 1.4 Confirmed existing coverage asserted operator presence, not order, so
  the regression was uncaught. Addressed by adding plan-ORDER assertions (§3.3)

## 2. Confirm the mechanism
- [x] 2.1 Confirmed the bucketing match routed `VariableLengthPath`,
  `QuantifiedExpand`, and `SpatialSeek` through `_ => others.push(operator)`
  rather than `scans`/`expansions`
- [x] 2.2 Confirmed the recombine order appends `others` after `filters` in both
  the `unwind_before_scan` and normal branches
- [x] 2.3 Confirmed a binding operator regenerates its result set independently
  of upstream rows and that an unbound variable in a `Filter` evaluates to
  `Null` → `false` (`value_to_bool`, `eval/predicate.rs:490`) — a silent no-op,
  not a hard error
- [x] 2.4 Confirmed reachability: `QuantifiedExpand` is emitted by real
  Quantified Path Pattern queries with a named inner boundary node;
  `VariableLengthPath` by `*m..n`; `SpatialSeek` by `point.distance`/bbox
  patterns. Tests exercise real queries, not synthetic plans

## 3. Fix the ordering
- [x] 3.1 Added `SpatialSeek` to the `scans` bucket (both the
  `unwind_before_scan` detection loop and the main bucketing match) and
  `VariableLengthPath` + `QuantifiedExpand` to the `expansions` bucket, so all
  variable-binding operators recombine before `filters`
  (`crates/nexus-core/src/executor/planner/queries/cost.rs`).
  `NodeIndexSeek`/`CompositeBtreeSeek` were already present
- [x] 3.2 Evaluated the topological-ordering alternative; DEFERRED as
  out-of-scope for this surgical fix on a hot, entangled file. Instead added an
  explicit INVARIANT comment stating every binding operator must be bucketed
  before `filters` and the `_ => others` catch-all must never receive one, so a
  new binding operator variant is caught in review. Trade-off recorded here and
  in the CHANGELOG
- [x] 3.3 Added plan-ORDER assertions (binding-operator index < Filter index)
  for `VariableLengthPath`, `QuantifiedExpand`, and `NodeIndexSeek` in
  `plan_binding_operator_order_test.rs`, so this regression class now fails CI
- [x] 3.4 The §1 tests pass post-fix (5/5); executor group 181/181 green

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — CHANGELOG
  entry added ("WHERE predicates after variable-length paths no longer silently
  drop"). `docs/specs/cypher-subset.md` has no planner operator-ordering section,
  so no spec change was warranted
- [x] 4.2 Write tests covering the new behavior —
  `plan_binding_operator_order_test.rs`: behavioral + plan-order guards for
  `VariableLengthPath` and `QuantifiedExpand`, plus a `NodeIndexSeek` regression
  lock (registered in `tests/executor/main.rs`)
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all` clean;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean;
  `cargo +nightly test --workspace` green

## Related
- `phase0_fix-correlated-predicate-index-seek`, `phase0_fix-where-clause-index-seek`
  — other planner/index-seek defects in the same area
- `phase0_fix-loadcsv-reorder-drops-predicates` — same bug class, filed as a
  follow-up: `LOAD CSV` binds a per-row variable still left in `others`
