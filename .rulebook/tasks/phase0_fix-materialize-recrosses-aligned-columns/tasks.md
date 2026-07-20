# Tasks: phase0_fix-materialize-recrosses-aligned-columns

`materialize_rows_from_variables` re-crosses columns that
`apply_cartesian_product` has already aligned, producing an `N^k` intermediate
(`384^3 ~= 56.6M` rows, ~13 GB) for a `k`-pattern `MATCH (a),(b),…`. The final
result is correct only because downstream dedup collapses it; the peak-memory
intermediate is unguarded and freezes the host.

SAFETY: never run the detonating fixture (`NODES=8, ROWS=6`) until §3 lands. All
reproduction in §1 MUST use a tiny fixture whose WRONG `N^k` intermediate is
itself small (e.g. `NODES=3, ROWS=2` -> aligned 6, wrong `6^3=216`, both cheap)
and assert on a PEAK-size signal, not by trying to allocate the big one.

Order: reproduce safely (§1) -> pin the exact site + correct semantics (§2) ->
fix (§3) -> re-enable the ignored canary + verify O(N) peak (§4).

## 1. Reproduce the over-production safely
- [ ] 1.1 Add a SAFE unit/integration test for a two-pattern
  `UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d}) RETURN a.id, b.id`
  over an UNINDEXED `:P(id)` with a tiny fixture (aligned length small enough
  that even the buggy `N^k` intermediate is affordable). Assert the observable
  wrong behaviour today: the number of rows materialised BEFORE the filters
  (peak intermediate) is `N^k`, not `N`. Prefer instrumenting
  `materialize_rows_from_variables` (e.g. a test-only counter or by asserting
  `total_combinations`) over trying to observe memory
- [ ] 1.2 Confirm the FINAL result is already correct (dedup masks it): the same
  query returns exactly the expected joined rows. This proves the fix is a
  peak-memory fix, not a correctness fix, and pins the invariant §3 must keep
- [ ] 1.3 Characterise the exponent: vary the pattern count (2 vs 3 patterns)
  and confirm the intermediate scales as `N^k`. Record `k` and `N` so §3's
  assertion targets the exact reduction to `O(N)`

## 2. Diagnose the mechanism
- [ ] 2.1 Confirm the call sequence for the two-pattern plan: `NodeByLabel(b)`
  routes through `apply_cartesian_product` (aligns `r,a,b` to length 384,
  `handled_cross_product=false`), then `seed_scan_main_loop` calls
  `materialize_rows_from_variables` on the already-aligned variables. Cite the
  exact lines (`dispatch.rs::seed_scan_main_loop` :56-116, `eval/helpers.rs`
  :56-175, :177-306)
- [ ] 2.2 Confirm the misclassification: `needs_cartesian_product` at
  `eval/helpers.rs:261` (`has_multiple_arrays && all_multi_element &&
  all_same_len`) is true for already-aligned same-length columns, so `:263-305`
  re-crosses them into `N^k`. Establish the distinguishing signal between
  "already aligned (zip)" and "independent (cross)" — e.g. whether
  `apply_cartesian_product` / the seed branch can mark the variables as aligned,
  or whether `materialize` should never run after an alignment step
- [ ] 2.3 Write the chosen approach down before touching code. Two candidates:
  (a) `materialize` ZIPs when the arrays are known-aligned; (b)
  `apply_cartesian_product` updates the result set directly and
  `seed_scan_main_loop` skips `materialize` for that branch. Pick one and state
  why it cannot regress the legitimate independent-arrays cross (single-pattern
  scans that genuinely still need crossing)

## 3. Fix
- [ ] 3.1 Apply the §2 fix so a `k`-pattern `MATCH` materialises `O(N)` peak rows
  (the aligned columns zipped), not `O(N^k)`. The final joined result MUST stay
  identical to today (the §1.2 oracle)
- [ ] 3.2 Confirm the legitimate cross-product path is untouched: cases where
  two arrays are genuinely independent and still need crossing (verify against
  existing comma-separated `MATCH` / cartesian tests and the Neo4j compat suite
  shape) still produce all combinations

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation: add a
  CHANGELOG entry noting comma-separated multi-pattern `MATCH` used to
  materialise an `N^k` intermediate (unbounded peak memory / host freeze) before
  filtering and is now `O(N)`; update the `oom_budget_verification_test.rs`
  module doc that claims the intermediate is "tiny"
- [ ] 4.2 Write tests covering the new behavior: re-enable
  `raising_budget_lets_the_same_query_return_exact_rows` in
  `crates/nexus-core/tests/oom_budget_verification_test.rs` (remove the
  `#[ignore]`) and confirm it passes with the original `NODES=8, ROWS=6`
  fixture, now that the peak intermediate is `O(N)`; keep the §1 peak-size
  assertion as a regression guard
- [ ] 4.3 Run tests and confirm they pass. Run ONLY targeted single-file tests
  with limited `-j` (`cargo +nightly test -p nexus-core --test
  oom_budget_verification_test -j 4`, plus the new §1 test file) — do NOT run
  the full workspace or `llvm-cov` while this shape can still blow up. Then
  `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`

## Related
- `phase0_fix-cypher-oom-process-abort` — added the `apply_cartesian_product`
  budget guard; this task fixes the UNGUARDED downstream `materialize` step it
  does not cover. The `oom_budget_verification_test.rs` canary lives with that fix
- `phase0_fix-unindexed-correlated-match-drops-rows` — same two-pattern /
  correlated shape; its `compute_row_dedup_key` widening is what MASKS this
  over-production in the final result (so correctness tests pass while peak
  memory explodes)
