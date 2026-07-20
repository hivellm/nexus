# Tasks: phase0_fix-where-clause-index-seek

A WHERE-form equality predicate on an indexed property (`MATCH (n:Person) WHERE
n.age = 30 RETURN n`) full-scans instead of seeking the index, because
`node_index_seek_for` (`strategy.rs:1307-1342`) only inspects inline pattern
properties and the executor's filter→index fast path `try_index_based_filter`
discards its own index lookup and falls back to a scan
(`operators/scan.rs:181-186`, `:244-247`) — with no notification at all
(`unindexed.rs:151-152` only fires for `Equal`). Two further gaps share the same
code: composite indexes are never probed for inline multi-property selectors
(#7), and the join cost estimator feeds a COST value into a variable used as
CARDINALITY (#8).

Order matters: prove the seek regression (§1) before touching the seek code
(§3), because the fix must be validated against the actual full-scan-vs-seek
plan shape, not assumed from reading the code. Land the seek fix (§3) before the
notification fix (§4) — #6's tests depend on #5's corrected plan shape to assert
the notification correctly falls silent for the now-covered equality case while
still firing for range/IN/string-match forms; validating #6 against the OLD
(broken) #5 behavior would produce tests that pass for the wrong reason. #7
(composite indexes) and #8 (join cost) are independent defects in the same
files and follow afterward (§5, §6), ahead of the tail.

## 1. Reproduce #5: WHERE-form misses the index seek
- [ ] 1.1 Write a failing test: `CREATE INDEX FOR (n:Person) ON (n.age)`, then
  run `MATCH (n:Person) WHERE n.age = 30 RETURN n` and assert the physical plan
  contains `NodeIndexSeek` (or equivalent seek operator), not
  `NodeByLabel`/`AllNodesScan`. Confirm it fails today (the plan falls back to a
  scan operator)
- [ ] 1.2 Confirm the inline-pattern-form sibling query `MATCH (n:Person
  {age:30}) RETURN n` DOES produce a `NodeIndexSeek` today — the baseline that
  shows the asymmetry, since `node_index_seek_for` only ever sees inline
  properties (`strategy.rs:1307-1342`)
- [ ] 1.3 Trace and record that `try_index_based_filter`
  (`operators/scan.rs:181-186`, `:244-247`) finds non-empty `entity_ids` from
  the index for the query in 1.1 but discards them and returns `Ok(None)` in
  both the equality and range branches — confirming the stub is genuinely
  reached, not dead code, for this query

## 2. Reproduce #6's baseline: notification fires only for Equal
- [ ] 2.1 With the same `:Person(age)` index, run `MATCH (n:Person) WHERE
  n.age > 30 RETURN n` (range predicate) and confirm today NO unindexed-access
  notification is emitted despite the full scan — trace `unindexed.rs:151-152`
  gating on `BinaryOperator::Equal` only
- [ ] 2.2 Repeat for `IN`, `STARTS WITH`, and `CONTAINS` predicates against the
  same index — confirm none emit a notification today

## 3. Fix #5: plan-time index seek for WHERE-form predicates
- [ ] 3.1 Choose the implementation strategy from the proposal's "What Changes"
  (lift WHERE predicates into `NodeIndexSeek` at plan time vs. finish
  `try_index_based_filter`'s row construction) and record the decision and why
  in the proposal
- [ ] 3.2 Implement the chosen fix so `MATCH (n:Person) WHERE n.age = 30 RETURN
  n` produces the same seek-based plan (or the same seek-based row
  construction) as `MATCH (n:Person {age:30}) RETURN n`
- [ ] 3.3 Make the §1.1 test pass; add a range-predicate seek variant (`WHERE
  n.age > 30`) if the chosen strategy naturally extends to range, and record
  explicitly whether range seeking is in scope here or deferred to a follow-up
  task
- [ ] 3.4 Confirm the row set returned by the WHERE-form and inline-form queries
  is identical on a populated dataset — this is a plan-selection fix, not a
  semantics fix, so results must not change

## 4. Fix #6: complete the unindexed-access notification
- [ ] 4.1 Extend `unindexed.rs` (`:151-152`) to evaluate range (`>`, `<`, `>=`,
  `<=`), `IN`, `STARTS WITH`, and `CONTAINS` operators against
  `prop_idx.has_index`, not just `Equal`
- [ ] 4.2 Make the §2.1 and §2.2 tests assert the notification now fires for the
  range/IN/string-match forms that remain unindexed (or, per §3.3, for
  range if it was left out of the §3 seek fix), AND assert it stays SILENT for
  the equality case now covered by the §3 seek fix — the notification must
  track reality, not just "any WHERE on this property"

## 5. Fix #7: composite-index detection for inline multi-property selectors
- [ ] 5.1 Write a failing test: register a composite index covering `(a, b)`
  (e.g. `CREATE CONSTRAINT ... REQUIRE (n.a, n.b) IS NODE KEY` or an equivalent
  composite `CREATE INDEX`), then run `MATCH (n:L {a:1, b:2}) RETURN n` and
  assert the plan uses `Operator::CompositeBtreeSeek`, not a full scan. Confirm
  it fails today (`node_index_seek_for` only calls
  `prop_idx.has_index(label_id, key_id)` per single property,
  `strategy.rs:1332`)
- [ ] 5.2 Implement composite-index detection in the planner: when an inline
  property map's keys match a registered composite index's key set, emit
  `Operator::CompositeBtreeSeek` instead of falling through to a scan
- [ ] 5.3 Make the §5.1 test pass; add a partial-match test (the inline selector
  has only SOME of the composite index's keys) and confirm it correctly falls
  back to the existing single-property-index or scan path rather than
  mis-seeking on an incomplete key

## 6. Fix #8: join cost estimator category error
- [ ] 6.1 Write a test asserting `Operator::Join { join_type: Inner, .. }`
  cost estimation produces a cardinality consistent with
  `estimate_operator_cardinality` applied to each input, not with
  `estimate_plan_cost`'s cost units. Confirm today's value diverges (`cost.rs:296-297`
  assigns `estimate_plan_cost`'s output into `left_cardinality`/`right_cardinality`)
- [ ] 6.2 Replace `estimate_plan_cost(&[*left.clone()])` /
  `estimate_plan_cost(&[*right.clone()])` at `cost.rs:296-297` with
  `estimate_operator_cardinality`, matching the `Union` arm's pattern at
  `:332-333`
- [ ] 6.3 Make the §6.1 test pass; run a multi-way join query through the
  planner and confirm plan selection changes only where the old cardinality
  value was wrong, with no unrelated plan regressions

## 7. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 7.1 Update `docs/specs/cypher-subset.md` with the WHERE-form index-seek
  contract and the completed unindexed-notification coverage; add a CHANGELOG
  entry
- [ ] 7.2 Tests: WHERE-form equality seek, WHERE-form range/IN/string-match
  notification behavior, composite-index seek (full and partial match),
  join-cost cardinality — all passing
- [ ] 7.3 Run `cargo +nightly fmt --all`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo +nightly test
  --workspace` — all green

## Related
- `phase0_fix-correlated-predicate-index-seek` — the sibling correlated-predicate
  index bypass in the same code area
