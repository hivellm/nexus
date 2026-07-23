# Tasks: phase0_fix-multi-hop-count-star-incorrect

`count(*)` over a multi-hop pattern returned 1 with both hops independently
at 0 (observed 2026-07-21; workaround note lives in
`tests/executor/write_refresh_visibility_test.rs` chained-targets test).
Details in proposal.md.

## 1. Root cause and fix
- [x] 1.1 Reproduced empirically: 2-hop `count(*)` = 1 with 0 matches (isolated
      nodes); = 0/1/N correct for the other shapes. Pinned the phantom to the
      case where the FIRST hop is empty (chained Expand on an emptied pipeline).
      Failing tests written first (`tests/executor/multi_hop_count_test.rs`)
- [x] 1.2 Root cause: `Expand`'s empty tail (`operators/expand.rs:549`) only
      preserved `result_set.columns` when the input rows were non-empty
      (`… && !rows.is_empty()`); a later hop on an already-empty pipeline fell to
      the else branch's `update_result_set_from_rows(&[])`, which wiped
      `columns` (`eval/helpers.rs:765`). `Aggregate` (`aggregate/core.rs:115-120`)
      treats empty `columns` as "no pattern" and mints a `count(*)=1` virtual
      row. Fixed by broadening the guard to `expanded_rows.is_empty()`
- [x] 1.3 Regression tests: `count(*)`/`count(var)` on 2-hop and 3-hop at 0/1/N
      plus a shared-intermediate fan-out (9 tests). The
      write_refresh_visibility_test.rs chained-targets test now asserts the
      direct 2-hop `count(*)`; per-hop asserts kept as extra coverage
- Note: a separate defect in the same operator (failed *required* Expand leaks a
      partial `[a, Null, Null]` projection row) is out of this `count(*)` scope —
      flagged for a follow-up task

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation —
      `docs/specs/cypher-subset.md` aggregations note + CHANGELOG entry
- [x] 2.2 Write tests covering the new behavior — `multi_hop_count_test.rs`
      (9 tests) + updated chained-targets assertion
- [x] 2.3 Run tests and confirm they pass — executor group 219/0; full
      `cargo +nightly test --workspace` run to confirm; fmt + clippy green
