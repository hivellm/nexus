## 1. Implementation
- [x] 1.1 `LIMIT 0` returns zero rows — the dispatcher's final assembly read an
      empty result set as "no operator produced rows" and fell back to the
      pre-limit rows; a `row_count_is_final` flag now distinguishes a
      deliberately emptied set from an unpopulated one
      (`executor/dispatch/operator_loop.rs`).
- [x] 1.2 Resolve parameters in SKIP and LIMIT — the request's parameters never
      reached the planner (`CypherQuery::params` was always empty), so
      `parse_and_plan_with_params` now attaches them and `resolve_row_count`
      accepts an integer literal or a `$param` bound to one.
- [x] 1.3 Raise the spec's error kind for float, negative, and non-constant
      SKIP/LIMIT arguments — `NegativeIntegerArgument`, `InvalidArgumentType`,
      `NonConstantExpression` in `semantic_validation`.
- [x] 1.4 Apply a `WITH ... SKIP/LIMIT` before the next clause consumes rows,
      including an aggregating one — each WITH now carries its own
      ORDER BY/SKIP/LIMIT tail (`planner_core/with_tail.rs`), lowered directly
      after that WITH instead of into the query-wide slots the final RETURN
      writes to.
- [x] 1.5 Re-measure the with-orderBy rows against the grouping-key fix and record
      what belonged to which cause — measured with the grouping-key task already
      archived (2026-08-10). Of the 58 with-orderBy rows the proposal attributed
      here, SKIP/LIMIT owned **3**. The remaining 145 failure rows are dominated by
      causes this task does not touch: 64 the parser rejecting `ASCENDING`/
      `DESCENDING` spelled out, 39 the missing `ORDER BY` validation (F-134), 19
      ordering semantics, 23 other. Recorded in
      `docs/analysis/tck-rebaseline/04-read-clauses.md` (F-133).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation —
      `docs/specs/cypher-subset.md`: accepted argument forms, the error kind each
      illegal form raises, and how a tail binds to the `WITH` it follows.
- [x] 2.2 Write tests covering the new behavior —
      `crates/nexus-core/tests/executor/skip_limit_semantics_test.rs`, 21 cases
      across the four defects plus the two regressions the first TCK run exposed.
- [x] 2.3 Run tests and confirm they pass — 21/21; `executor` group 293/293,
      `cypher` group 757/757.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green — 5949 passed /
      0 failed / 97 ignored across 60 targets.
- [x] 3.3 Neo4j differential suite unchanged — 325 tests, 310 passed, 0 failed,
      15 skipped, identical to the baseline. (The checklist said 300/300; the
      suite's actual shape is 310/325.)
- [x] 3.4 TCK re-run: this task's categories improved, no category regressed —
      `return-skip-limit` 58.1% → 77.4%, `with-skip-limit` 55.6% → 66.7%,
      `with-orderBy` 49.3% → 50.3%, `aggregation` 48.6% → 51.4%, total 62.2% →
      62.4% (2404 → 2415). Per-scenario faillog diff: 10 newly passing, 0 newly
      failing. An earlier run of the same fix had 2 newly failing with-orderBy
      scenarios; both were real regressions, fixed, and pinned by tests.
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md` — regenerated
      by the harness on the final run.
