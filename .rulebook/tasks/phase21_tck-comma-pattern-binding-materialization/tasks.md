## 1. Implementation
- [x] 1.1 Materialize comma-pattern variables into driving rows (cartesian product across patterns) so downstream operators see them row-bound
  - **The proposal's symptom had already half-changed, and the mechanism it named
    was not the cause.** Re-running its probe first: it recorded
    `["a","b",false]` (`z` REBOUND to the `:B` node); today it returns
    `["a",Null]` — the rebinding is gone, because `pad_optional_row` learned to
    preserve an already-bound target (`!new_row.contains_key(target_var)`). Still
    wrong: openCypher keeps `z` bound and nulls only `r`.
  - **Root cause, from `EXPLAIN`, not from reasoning:** the plan for
    `MATCH (a:A), (z:Z) OPTIONAL MATCH (a)-[r:T]->(z)` was
    `NodeByLabel(a) → Expand(a→z, optional) → Project`. **No scan for `z` at all.**
    So `z` was never in a row, and the optional hop treated it as its own output
    slot, which is what the padding then nulled. Nothing to do with
    `materialize_rows_from_variables`, which the proposal pointed at.
  - The suppression came from `pattern_lowering.rs` pooling "nodes an `Expand`
    will populate" across **every** pattern of the query and skipping a driving
    scan for all of them. `z` is a relationship target in the OPTIONAL clause, so
    the earlier comma clause — which is what actually binds it — was denied its
    scan. Now computed per pattern via `Self::relationship_target_vars`.
  - **That exposed a second, older defect the pooled set had been masking.**
    `previously_bound_vars` was seeded from the start pattern and never grown, so
    from the third pattern onward every guard asking "did an earlier pattern bind
    this?" answered no. It never surfaced because the pooled target set skipped
    those variables for an unrelated reason. With the set scoped per pattern, the
    executor suite caught it immediately: the two `chained_optional_match_*` tests
    in `tests/executor/optional_match_var_scoping_test.rs` went to 2 rows, because
    `MATCH (a) OPTIONAL MATCH (a)-->(b) OPTIONAL MATCH (b)-->(c)` emitted an
    `AllNodesScan` for `b` in the third clause and decorrelated. Fixed by
    inserting each pattern's node variables into `previously_bound_vars` at the
    end of its loop iteration — the binding order the guards already claimed to
    model.
  - **Third change, forced by the second:** the additional-pattern loop's
    LABELLED branch had no already-bound guard (only the unlabelled one did). With
    the pooled set gone, `MATCH (a)-[:T]->(b) MATCH (b:B)` would have emitted
    `NodeByLabel(b)` and clobbered the traversal. An already-bound variable now
    gets its label as a `Filter` instead: the binding survives AND the predicate
    is enforced, where before it was silently DROPPED (that query matched a `b`
    with no `:B` label at all). Safe against the planner's operator sort, which
    recombines `filters` AFTER `expansions` (`queries/cost.rs`), so the filter
    sees the variable bound. Restricted to required patterns — in an `OPTIONAL`
    one a contradictory label should leave the row padded rather than drop it, and
    a filter cannot express that, so there the predicate stays dropped as before
    rather than trading one wrong answer for another.
- [x] 1.2 Verify OPTIONAL MATCH closing over a comma-pattern variable pads instead of rebinding (WithWhere1[3]/[4] pass; no regression in existing multi-pattern MATCH suites)
  - The probe now returns `('a', 'z', r IS NULL = true)`, and its control (an edge
    that DOES join the two comma variables) binds `r`.
  - **Correction to the proposal:** it claimed this capped `WithWhere1` [3] AND
    [4]. Only [3] was failing — the fail log shows `[4] Filter for an unbound node
    variable` already passing before this change. Both are pinned by tests now.
  - No regression in the multi-pattern suites: `--test executor` 272 and
    `--test cypher` 655, both 0 failures, after the binding-order fix above.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md`: comma-separated parts each get their own
    driving scan and bind independently; a variable bound that way survives a
    later clause that uses it as a relationship target (worked example with the
    `r IS NULL` idiom); and a label re-stated on an already-bound variable in a
    required clause is enforced as a filter.
  - Residual gap stated rather than left implicit: in an `OPTIONAL MATCH`, a label
    re-stated on an already-bound variable is not enforced, because expressing it
    would have to pad the row rather than drop it.
- [x] 2.2 Write tests covering the new behavior
  - 7 tests in `tests/cypher/comma_pattern_binding_test.rs`: the proposal's probe;
    the edge-exists control; the plain cartesian control (the scan the fix
    restored must not change it); `WithWhere1` [3] and [4] with their fixtures
    reproduced verbatim; and both directions of the already-bound label predicate
    — the negative case (`b` is `:Other`, must yield 0 rows) being the one that
    was silently wrong before.
  - The binding-order regression is already guarded by the two pre-existing
    `chained_optional_match_*` tests, which is how it was caught; not duplicated.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 655, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245 — 0 failures. `fmt` clean, `clippy --all-targets
    --all-features -D warnings` clean. Workspace `--no-fail-fast`: **5790 passed,
    3 failed**, those 3 being the flakes below.
  - Lib reported the same 3 documented flakes as the previous task
    (`property_index_survives_restart`, the two `exists_` var-length tests); all
    three re-verified passing in isolation.
  - **TCK: +2 attributable, zero regressions.** `clauses/with-where` 12 → 13 and
    `clauses/match` 144 → 145. Attributed by scenario name against the previous
    build's fail log: the newly passing scenarios are exactly
    `WithWhere1 [3] Filter for an unbound relationship variable` and
    `clauses/match [3] OPTIONAL MATCH and bound nodes` — both squarely this
    defect's class.
  - Noise measured, not assumed: two runs of the IDENTICAL build move
    `clauses/merge` (25↔24), `clauses/return` (29↔30) and `clauses/with-orderBy`
    (89↔90) and nothing else, which is exactly the documented oscillator set —
    `return` [9] is the map-literal key-order nondeterminism root-caused in the
    isomorphism task (its query has no `MATCH`, so this change cannot reach it),
    and `with-orderBy` had already measured 89↔91 across two identical runs
    earlier the same day. Total reads 1826 and 1827 across the two runs against a
    1827 baseline; the two real gains are visible per category, not in the total.
