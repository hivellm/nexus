## 1. Implementation
- [x] 1.1 ORDER BY expression re-evaluation
  - **The proposal's prescription would not have worked, and reading the pipeline
    order is what showed it.** It said to "re-evaluate ORDER BY expressions
    against bindings (not column-name only)". Re-evaluation at sort time cannot
    work: `Sort` runs AFTER the projection and reads `result_set` columns, so for
    `RETURN n.name ORDER BY n.age` there is no `n` left in the row to read `age`
    from. The data is gone by then, not merely unresolved.
  - The key has to be CARRIED through the projection instead. New
    `resolve_order_by_columns` projects a hidden `__order_by_key_<i>` column for
    any sort key the `RETURN` does not already produce, points the `Sort` at that
    alias, and `execute_sort` drops such columns once the rows are ordered — so
    they never reach the client (asserted by its own test).
  - The key's expression is recovered by re-parsing the string the planner made
    with `expression_to_string`, the same round-trip `Operator::Filter` relies on
    when it carries no AST. That avoided threading a new field through
    `Operator::Sort`'s 6 construction and 3 destructuring sites. A key that fails
    to re-parse keeps the old behaviour (used as-is, skipped) rather than failing
    the query.
  - **Deliberately NOT applied after `DISTINCT` or an aggregating projection.**
    Cypher puts the pre-projection variables out of scope there, and a hidden
    column would silently change what `DISTINCT` dedups on or what the
    aggregation groups by — a wrong answer traded for a wrong answer.
  - Wired at both planner paths that emit `Sort` from a `RETURN`: the pattern path
    (`strategy/pattern_lowering.rs`) and the no-pattern path
    (`planner_core/bound.rs`, which is what `UNWIND … RETURN … ORDER BY` takes —
    found because the expression probe still failed after the first site was
    fixed). The post-`UNION` site is untouched: an `ORDER BY` there names union
    output columns, which are projected by definition.
- [x] 1.2 cross-type sort order
  - Implemented as a type rank consulted before the value comparator, per
    openCypher TCK `clauses/return-orderby/ReturnOrderBy1.feature` [11]/[12]:
    `MAP < NODE < RELATIONSHIP < LIST < PATH < STRING < BOOLEAN < NUMBER < NaN <
    null`, with `DESC` the exact reverse (so `null` first descending — the
    existing null wrapper already did that half).
  - **Kept on the sort path, NOT in `compare_values_for_sort`, and that mattered.**
    That comparator is shared with the comparison OPERATORS (`<`, `<=`, `>`,
    `>=`); ranking types inside it would have changed `1 < 'text'` from whatever
    it returns today into a verdict derived from the sort table. The null rule was
    already special-cased in the sort wrapper for exactly this reason, so the rank
    joins it there.
  - Before: mixed-type pairs fell through to a stringified comparison, so `1.5`
    sorted before `'text'` and booleans ordered against numbers by the letters of
    `"false"`.
  - `NaN`'s slot is unreachable: the value type is `serde_json::Number`, which
    cannot hold a non-finite float (`phase21_tck-non-finite-floats`). Stated in
    the rank function's doc and in the test that omits it from its fixture.
  - Noted, not fixed: a PATH value does not survive an `UNWIND` list (it projects
    as `null`), so the TCK's own [11]/[12] fixtures cannot pass in full even with
    the ordering correct. Separate defect, outside this task.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § ORDER BY: a sort key need not be projected
    (with both worked examples, and the `DISTINCT`/aggregation exclusion stated);
    the cross-type total order written out, with `DESC` as its exact reverse and
    an explicit note that the comparison operators do not derive from it; and the
    `NaN` gap.
  - `CHANGELOG.md`: both halves, with the measured TCK movement.
- [x] 2.2 Write tests covering the new behavior
  - 9 tests in `tests/cypher/order_by_expression_test.rs`: the full cross-type
    order ASC and DESC against the TCK fixture (minus NaN), the scalar
    string/boolean/number core, a control that same-type ordering is unchanged,
    the unprojected-property key, the expression key, a test that the hidden
    column never reaches the client (columns AND per-row value count), and two
    controls for the resolution paths that already worked (alias and projected
    expression).
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 666, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245 — 0 failures. Workspace `--no-fail-fast`:
    **5814 passed, 3 failed**, the 3 being the documented flakes. `fmt` and
    `clippy --workspace --all-targets --all-features -D warnings` clean.
  - **TCK: +58 attributable, ZERO new failures in any category.**
    `clauses/with-orderBy` **90 → 148 of 292 (30.8% → 50.7%)**, total
    **1830 → 1889 (47.3% → 48.8%)** — the largest single movement of this epic so
    far.
  - Attribution: diffing every failing scenario across the two runs yields 9
    distinct newly-passing (feature, scenario) pairs and **no** newcomers. Eight
    are `clauses/with-orderBy` and name the fix directly — `[1] Sort by a
    projected expression`, `[5] An expression without explicit sort direction…`,
    `[10] Sort by a non-projected expression containing an alias…`, `[15] Sort by
    an aliased aggregate projection…`, and others. The distinct-pair count is 9
    while the report moves 59 because the runner counts outline-EXPANDED
    scenarios: one Gherkin scenario outline covers many. The ninth pair is
    `clauses/return [9] Returning a projected map`, the map-key-order oscillator,
    which is not claimed.
  - This category is the one that had been oscillating 89↔92 all day and was
    treated as noise on that basis. The oscillation was real but was a small band
    inside a category dominated by genuine ORDER BY failures — worth recording so
    a future session does not read "known oscillator" as "nothing to fix here".
