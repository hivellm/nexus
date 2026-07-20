# Tasks: phase0_fix-unindexed-correlated-match-drops-rows

`UNWIND $rows AS r MATCH (a:P {id: r.s})` over an UNINDEXED `:P(id)` returns only
the first driving row's matches and drops the rest. Confirmed: nodes
`{10, 20, 30, 20}`, driving `[10, 20, 99, 30]` → indexed seek yields
`[10, 20, 20, 30]` (correct), unindexed label-scan+filter yields `[10]`. Plan is
`[Unwind, NodeByLabel(a), Filter(a.id = r.s), …]`; the order is correct
(`NodeByLabel` is a scan), so this is NOT the `optimize_operator_order` bucketing
bug — the fault is downstream, in the correlated residual filter / cross product.

Order matters: reproduce (§1) before diagnosing, and pin the exact site (§2)
before changing execution (§3). The mechanism is a HYPOTHESIS — confirm it.

## 1. Reproduce the truncation
- [ ] 1.1 Write a failing integration test: seed `:P {id}` nodes (include a
  duplicate id and a driving value that matches nothing), run the unindexed
  `UNWIND … MATCH (a:P {id: r.s}) RETURN a.id`, and assert the FULL correct
  result (every match for every driving row). Confirm it fails today, returning
  only the first driving row's matches. Reuse the fixture style in
  `crates/nexus-core/tests/correlated_index_seek_e2e_test.rs`
- [ ] 1.2 Pin the reference: the SAME data + query WITH an index on `:P(id)`
  returns the correct rows (the `phase0_fix-correlated-predicate-index-seek`
  seek path). The unindexed path must match it. Use this as the oracle
- [ ] 1.3 Vary the driving order (put a miss first, put the duplicate first) to
  characterise the exact truncation: is it "only the first driving row" or "only
  rows whose value equals the first row's value"? Record which, so §2 targets the
  right mechanism

## 2. Diagnose the mechanism
- [ ] 2.1 Dump the compiled plan for the unindexed shape and confirm operator
  order (`[Unwind, NodeByLabel, Filter, …]`). Rule the reorder in or out
  explicitly against `phase0_fix-plan-reorder-drops-predicates`
- [ ] 2.2 Trace the `UNWIND × NodeByLabel` cross-product seeding
  (`dispatch.rs` `seed_scan_main_loop`, the "existing rows, no variables yet"
  branch) — confirm whether the full `ROWS × NODES` product is built with `r`
  and `a` bound per row, or whether the driving rows collapse to one
- [ ] 2.3 Trace the residual `Filter(a.id = r.s)` evaluation
  (`operators/filter.rs`) — confirm whether the correlated RHS `r.s` is
  evaluated per row or bound once (e.g. to the first row's value). The synthesized
  predicate string is re-parsed; check that path against
  `phase0_fix-where-predicate-reparse-precedence`. Write the finding down before
  touching code

## 3. Fix
- [ ] 3.1 Apply the fix at the site §2 identifies so the unindexed path evaluates
  the correlated predicate per driving row and returns every match. The result
  MUST equal the indexed seek path (§1.2 oracle) for the same data
- [ ] 3.2 Confirm no regression to the constant inline form (`{id: 42}`) or to
  non-correlated filters — the fix must be scoped to the correlated RHS case

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation if the executor filter/cross-product
  semantics change materially; add a CHANGELOG entry noting the unindexed
  correlated shape used to truncate results
- [ ] 4.2 Tests: unindexed correlated match returns all matches (incl. duplicate
  keys, misses, driving order variations); parity with the indexed seek path;
  the constant form and non-correlated filters unaffected
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` green)

## Related
- `phase0_fix-correlated-predicate-index-seek` — fixed the INDEXED path; this is
  the unindexed fallback that still misbehaves (its `correlated_index_seek_e2e_test.rs`
  documents this finding)
- `phase0_fix-plan-reorder-drops-predicates` — same `optimize_operator_order`, a
  different defect; ruled out for this shape in §2.1
- `phase0_fix-where-predicate-reparse-precedence` — residual-predicate re-parse is
  a candidate site (§2.3)
- `phase0_fix-where-clause-index-seek` — the `WHERE a.id = r.s` form of the same shape
