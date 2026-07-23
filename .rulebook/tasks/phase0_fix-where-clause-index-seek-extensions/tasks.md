# Tasks: phase0_fix-where-clause-index-seek-extensions

Follow-ups scoped out of `phase0_fix-where-clause-index-seek` (parent landed in
f8eb857e + 67eba262): range/IN/STARTS WITH seeking, `$parameter` equality
seeking, and EXPLAIN/PROFILE plan-display accuracy. All are plan-selection /
diagnostics only — results must not change.

## 1. Range / IN / STARTS WITH seeking
- [ ] 1.1 Reproduce: `MATCH (n:Person) WHERE n.age > 30` with a `:Person(age)`
  index plans a scan today (and emits the unindexed notification); assert it
  should seek. Same for `IN [..]` (disjunction of point seeks) and
  `STARTS WITH 'x'` (string prefix range)
- [ ] 1.2 Add a range-seek operator / B-tree range scan (or reuse an existing
  one) and lift these WHERE forms; keep the unindexed notification firing only
  for the forms that still can't seek
- [ ] 1.3 Result-parity tests (WHERE-form vs full-scan identical rows)

## 2. `$parameter` equality seeking
- [ ] 2.1 Reproduce: `MATCH (n:Person) WHERE n.age = $age` plans a scan today
- [ ] 2.2 Add a parameter-aware seek that resolves the bound value at execution
  time (the planner has no parameter values; `NodeIndexSeek.key_expression`
  needs driving rows a bare first scan lacks)
- [ ] 2.3 Result-parity tests

## 3. EXPLAIN / PROFILE plan accuracy
- [ ] 3.1 Wire `property_index` + `composite_index` into the EXPLAIN/PROFILE
  display planners (`engine/query_pipeline.rs` ~913/960), so the shown plan
  matches `Executor::plan_ast`
- [ ] 3.2 Test: EXPLAIN of a WHERE-equality-on-indexed query shows the seek

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation (CHANGELOG;
  `docs/specs/cypher-subset.md` WHERE-form index-seek coverage)
- [ ] 4.2 Write tests covering the new behavior (range/IN/prefix seek, parameter
  seek, EXPLAIN accuracy)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)

## Related
- `phase0_fix-where-clause-index-seek` (parent), `phase0_fix-correlated-predicate-index-seek`
