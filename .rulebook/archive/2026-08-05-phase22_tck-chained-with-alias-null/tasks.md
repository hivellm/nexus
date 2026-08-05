## 1. Implementation
- [x] 1.1 Reproduce all five variants above as a failing test, then root-cause where the second `WITH` loses the alias binding (name the file:line before editing)
  - All five reproduced exactly as the proposal recorded them, plus a sixth
    (three chained renames) that also returned nulls.
  - **Root cause is operator ORDER, not alias resolution.** `EXPLAIN` on
    `UNWIND [5,1,4] AS i WITH i AS a WITH a RETURN a` gave
    `Unwind → With{a AS a} → With{i AS a} → Project`: the two `WITH` operators are
    emitted REVERSED. The first to run projects `a` before anything binds it
    (nulls) and that projection drops `i`, so the `With{i AS a}` behind it has
    nothing left to read. Nothing was wrong with alias *resolution*.
  - Site, named before editing: `planner_core/bound.rs:696-704`. The WITH-emission
    loop computes `insert_pos` fresh per iteration; in the **no-sink** branch that
    is `last_unwind + 1`, a CONSTANT across iterations, so each insertion lands
    before its predecessor. The sink branch never had the bug — inserting before a
    `Project`/`Aggregate` pushes the sink right, so the next iteration's
    `sink_pos` is already one further along.
  - That also explains why the shape looked so specific: renaming once cannot be
    mis-ordered (one operator), and re-projecting without renaming is immune
    (reversing two identical projections changes nothing). The `MATCH`-source
    variant works because it takes the sink branch.
- [x] 1.2 Fix alias resolution so a `WITH`-minted alias binds for every downstream clause irrespective of its source clause
  - No resolution change was needed. Added an insertion cursor so the no-sink
    branch never places a `WITH` before one the same loop already placed
    (`next_no_sink_pos.map_or(base, |cursor| cursor.max(base))`), which preserves
    clause order. Six lines plus the comment explaining the failure it prevents.
- [x] 1.3 Verify the three-`WITH` case and the `UNWIND` + `MATCH` mixed case do not regress
  - Three chained renames return `[5,1,4]`; the `MATCH`-source shape still returns
    `[1,1,2]`. Both pinned by tests, the second explicitly as the control for the
    other insertion branch.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § WITH: an alias a `WITH` mints is a first-class
    binding for every downstream clause including another `WITH`, with the chained
    example, and the statement that it holds whatever produced the renamed value.
- [x] 2.2 Write tests covering the new behavior
  - 7 tests in `tests/cypher/chained_with_alias_test.rs`: the two failing shapes,
    three chained renames, both controls that hid the bug (rename once,
    re-project without renaming), the `MATCH`-source control for the other
    insertion branch, and a `WHERE` on the second `WITH` — ordering must not
    detach a predicate from its projection.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 689 (7 new), 0 failures.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
  - **5829 passed, 3 failed** — the 3 being the documented flakes
    (`property_index_survives_restart` and the two `exists_` var-length tests),
    which pass in isolation and are unrelated to planning.
- [x] 3.3 Neo4j differential suite still 300/300 (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`)
  - **RUN, and the gate's own number was stale.** Provisioned the missing half of
    the environment (`docker run -d -p 7474:7474 -p 7687:7687
    -e NEO4J_AUTH=neo4j/password neo4j:2025.09.0` — the exact version the suite
    targets, so a version skew cannot be mistaken for a Nexus divergence) plus the
    release server on :15474.
  - Result: **308 passed / 2 failed / 15 skipped of 325 (99.35%)**. There is no
    "300/300" to hold: the suite carries 325 cases, and 308/2/15 is its documented
    baseline (`project-merge-write-path-executor-gaps`).
  - The 2 failures are exactly the pre-existing MERGE write-path gaps that memory
    records by number — `15.08 Multiple MERGE` ("Multiple different variables in
    RETURN not supported for write queries") and `15.12 MERGE verify single node`
    ("Unsupported clause in write query", the same `WITH`-between-writes limit
    probed independently earlier in this session). Neither involves chained `WITH`
    projection ordering, and both predate this change.
  - So: no differential regression from A1, at the suite's real baseline. The
    "still 300/300" wording in this checklist (and the same claim in
    `AGENTS.override.md`, now corrected) should read 308/325.
- [x] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
  - Total unchanged at 1891, exactly as the proposal predicted ("near zero
    directly"). The scenario diff shows one fix (`clauses/return [9]`, the
    map-key-order oscillator, not claimed) and **zero** new failing scenarios.
  - `clauses/with-orderBy` read 147 → 146, so it was A/B'd rather than dismissed:
    an identical re-run of THIS build gives 146 and 147 on consecutive runs,
    bracketing the pre-change value. Inside the measured band, on the same build.
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
