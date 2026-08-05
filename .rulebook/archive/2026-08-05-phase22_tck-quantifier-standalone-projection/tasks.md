## 1. Implementation
- [x] 1.1 Prove the mechanism: instrument the no-pattern planning path and record, at a `file:line`, exactly which operator drops the row for `RETURN all(x IN [1,2] WHERE x > 0)`
  - **The proposal's leading hypothesis is REFUTED.** It expected the quantifier's
    inner `WHERE` to be lifted to a query-level predicate in `preparse.rs` /
    `planner_core/bound.rs`. `EXPLAIN` shows the plan for the failing query is a
    SINGLE `Project` operator — no `Filter`, no scan, nothing to lift into. The
    planner never sees the predicate at all.
  - **Actual site: `eval/helpers/core.rs:806-810`**, the `FunctionCall` arm of
    `can_evaluate_without_variables`, which requires EVERY argument to be evaluable
    without variables. The parser lowers `all(x IN list WHERE pred)` to three
    discrete args — bound-variable name as a STRING LITERAL, list, predicate
    (`parser/expressions/identifier.rs:60-63`) — so that walk sees a free
    `Variable("x")` inside the predicate and answers false. `execute_project` (and
    `execute_with`) then refuse to seed the synthetic unit row a standalone
    projection needs, and the query yields zero rows.
  - That is exactly why the loss needed both of the proposal's conditions: no
    upstream reading clause (or rows already exist and the gate is never consulted)
    AND a predicate mentioning the bound variable (or there is no `Variable` to
    trip over).
- [x] 1.2 Fix it so the quantifier's bound variable is scoped to the quantifier and never becomes a query-level filter or a planned scan
  - Followed the precedent already in the same function: the
    `ListComprehension` arm (`core.rs:851-857`) checks ONLY its list expression and
    documents that the `WHERE` may reference the loop variable because the
    comprehension binds it during execution. The quantifier now does the same via
    `quantifier_list_arg()`, which recognises the shape (name in
    `all|any|none|single` AND a string-literal first arg) and returns the list
    argument. A call that merely shares one of those names without the shape falls
    through to the generic rule.
  - No planner change was needed, since the planner was never involved.
- [x] 1.3 Cover all four quantifiers plus the `filter()` legacy form, with and without an upstream clause
  - Probed all seven list-predicate forms before writing the fix. Broken:
    `all`, `any`, `none`, `single` (0 rows each). **`filter()` was never broken** —
    the parser folds it into a real `ListComprehension`
    (`parser/expressions/identifier.rs:121`), which the existing arm already
    covered; pinned by test to prove the two forms now agree rather than "fixed".
  - `extract(x IN l | e)` and `reduce(acc = i, x IN l | e)` do not parse in that
    syntax at all (syntax error, both). The evaluator implements them
    (`fn_list.rs:255,297`) but the surface form is missing — a separate parser gap,
    recorded in the spec, deliberately not folded into this task. Also why
    `quantifier_list_arg` does not list them: there is no shape to recognise yet.
- [x] 1.4 Confirm `expressions/quantifier` moved out of the single digits; if it did not, stop and re-root-cause rather than layering a second fix
  - **`expressions/quantifier` 49/604 (8.1%) → 509/604 (84.3%)**, +460 scenarios in
    that one category. The plan's exit criterion said "high 80s"; the measured
    figure is 84.3%, stated as measured rather than rounded up.
  - Total **1892 → 2352 of 3868 (48.9% → 60.8%)**. The proposal predicted "479
    fails, roughly +12pp on the total"; measured +11.9pp.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § Predicate Functions: the bound variable is local
    to the quantifier, which makes the four predicates valid in a standalone
    projection and nested in a larger expression (both shown); the empty-list
    vacuous-truth rules; `filter()`/list-comprehension scope their variable the same
    way; and the `extract`/`reduce` surface-syntax gap as a known gap.
- [x] 2.2 Write tests covering the new behavior
  - 10 tests in `tests/cypher/quantifier_standalone_projection_test.rs`: each of the
    four quantifiers with a true and a false case; the empty-list vacuous rules for
    all four; both controls that hid the bug (`WHERE true`, and an upstream
    `UNWIND`); a standalone `WITH`, since `execute_with` carries the same gate; a
    quantifier nested in `AND` and under `NOT`; and the `filter()`/comprehension
    equivalence.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 705 (10 new), 0 failures.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
  - **5839 passed / 3 failed** — the 3 documented flakes
    (`property_index_survives_restart`, the two `exists_` var-length tests), which
    pass in isolation and do not touch projection seeding.
- [x] 3.3 Neo4j differential suite still 300/300 (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`)
  - **308 passed / 2 failed / 15 skipped of 325**, byte-identical to the baseline
    measured for A1 — the same two pre-existing MERGE write-path gaps (15.08,
    15.12). No differential regression.
  - As recorded in A1: the "still 300/300" wording is stale; the suite carries 325
    cases and 308/2/15 is its real baseline.
- [x] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
  - Two identical runs of this build both total 2352. The only categories that
    differ between them are `clauses/return` (29↔30) and `clauses/with-orderBy`
    (147↔148) — the two documented oscillators, and each brackets its pre-change
    value, so neither counts as a regression. Every other category is stable, which
    makes the +460 in `expressions/quantifier` fully attributable.
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
