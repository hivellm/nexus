# Tasks: phase0_fix-cypher-eval-panics

Three Cypher eval/aggregate sites panic (or silently wrap) on ordinary user-controlled numeric
input. Trigger examples: `RETURN date('2020-01-01') + duration({days: 999999999})` and
`RETURN datetime('2020-01-01T00:00:00Z') + duration({days: 100000000})` panic inside chrono;
`RETURN duration({years: 9223372036854775807}) + duration({years: 1})` wraps to a wrong duration
in release (panics in debug); `WITH [1.0,2.0,3.0] AS v UNWIND v AS x RETURN percentile_cont(x, 1.5)`
panics on an out-of-bounds array index.

Order matters: reproduce every defect with a failing test before touching any site. Fix the
temporal add/sub chrono panic first — it is the most directly reachable (any `date`/`datetime`
plus a `duration({..})` literal) and is a hard crash regardless of build profile. Duration ±
duration follows — same file, same component-arithmetic pattern, but wraps rather than panics in
release. percentile_cont is independent (different module) and follows. The dead-code parallel
AVG merge is a small "while here" cleanup with no live call site, so it is last and does not gate
the release-blocking fixes.

## 1. Reproduce each defect with a failing test
- [ ] 1.1 `RETURN date('2020-01-01') + duration({days: 999999999})` — assert a Cypher error, not
  a chrono panic (`temporal.rs:194-265`, dispatched via `try_datetime_add`, `temporal.rs:54`)
- [ ] 1.2 `RETURN datetime('2020-01-01T00:00:00Z') + duration({days: 100000000})` — same
  assertion for the datetime add arm (`temporal.rs:210-265`)
- [ ] 1.3 A subtract-direction case, e.g.
  `RETURN date('2020-01-01') - duration({days: 999999999})` — assert a Cypher error, not a panic
  (`temporal.rs:306-361`)
- [ ] 1.4 `RETURN duration({years: 9223372036854775807}) + duration({years: 1})` — assert a
  Cypher error, not a silently wrapped/wrong duration (`temporal.rs:81-86`) or a debug panic
- [ ] 1.5 A duration subtraction overflow case — assert a Cypher error (`temporal.rs:147-152`)
- [ ] 1.6 `WITH [1.0,2.0,3.0] AS v UNWIND v AS x RETURN percentile_cont(x, 1.5)` — assert a
  Cypher error, not an out-of-bounds panic (`aggregate/core.rs:866-877`)
- [ ] 1.7 `RETURN percentile_cont(1.0, -0.5)` over a small list — confirm the current
  saturating-cast behavior (no panic today) becomes an explicit Cypher error once validation is
  added, so the fix is behavior-preserving-or-better, not accidentally permissive

## 2. Fix datetime/date ± duration chrono overflow
- [ ] 2.1 In `crates/nexus-core/src/executor/eval/temporal.rs`, replace the unchecked component
  multiplies/sums feeding `duration_secs` (add arm :194,209,221,236,250; subtract arm
  :290,305,317,332,347) with `checked_mul`/`checked_add`, returning a Cypher error on overflow
- [ ] 2.2 Replace the chrono `Duration::seconds`/`::days` calls and the `DateTime`/`NaiveDate`/
  `NaiveDateTime` `+`/`-` operators at :210,:237,:265 (add) and :306,:333,:361 (subtract) with
  their checked equivalents (`Duration::try_seconds`/`try_days`,
  `checked_add_signed`/`checked_sub_signed`), returning a Cypher error instead of panicking when
  the result leaves chrono's representable range
- [ ] 2.3 Confirm the §1.1, §1.2, §1.3 tests pass

## 3. Fix duration ± duration overflow
- [ ] 3.1 In `temporal.rs:81-86` (add), replace each of the six unchecked component sums (years,
  months, days, hours, minutes, seconds) with `checked_add`, returning a Cypher error on overflow
- [ ] 3.2 In `temporal.rs:147-152` (subtract), apply the same treatment with `checked_sub`
- [ ] 3.3 Confirm the §1.4 and §1.5 tests pass in both debug and release-profile test runs (the
  defect's failure mode differs by profile, so both must be checked)

## 4. Fix percentile_cont index panic
- [ ] 4.1 In `crates/nexus-core/src/executor/operators/aggregate/core.rs:866-877`, validate
  `percentile` is within `[0,1]` before computing `position`, returning a Cypher error otherwise
- [ ] 4.2 Additionally clamp `lower_idx`/`upper_idx` with `.min(values.len()-1)` as defense in
  depth, mirroring the sibling `PercentileDisc` (:837-839)
- [ ] 4.3 Confirm the §1.6 and §1.7 tests pass

## 5. Secondary: fix or delete the dead-code parallel AVG merge
- [ ] 5.1 In `crates/nexus-core/src/executor/operators/aggregate/parallel.rs:286-309`, confirm
  `execute_parallel_aggregation`/`is_parallelizable_aggregation`/`execute_sequential_aggregation`
  still have zero call sites in the workspace
- [ ] 5.2 If kept: change the AVG merge to accumulate `(sum, count)` per chunk and compute the
  final mean as `Σsum / Σcount`, so it is correct if a future caller wires it in. If removed:
  delete the module and its now-unreachable helpers cleanly (no residual dead code, no broken
  `mod` declarations)
- [ ] 5.3 Either way, add a unit test proving the weighted-mean correctness (unequal chunk
  sizes) or confirm no test references the deleted module

## 6. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 6.1 Update `docs/specs/cypher-subset.md` with the error contract for temporal arithmetic
  overflow, duration arithmetic overflow, and the `percentile_cont` argument domain; add a
  CHANGELOG entry
- [ ] 6.2 Tests: all §1 regression tests pass; add a boundary test per site (largest in-range
  value succeeds, one past it errors)
- [ ] 6.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-cypher-unbounded-alloc-oom` — sibling eval-robustness defects found in the same
  audit
- `phase0_fix-cypher-oom-process-abort` — prior crash-hardening work in this area
