## 1. Implementation
- [x] 1.1 Group by every non-aggregate projection item regardless of expression shape, and keep its column in the output
      — The proposal's attribution was WRONG. It is not "only a property access on a
      graph variable works": the *pattern-driven* planner path always promoted every
      non-aggregate alias to a grouping key (`strategy/pattern_lowering.rs:1082-1090`).
      The defect was confined to the other path — `planner_core/bound.rs:811` declared
      `let group_by_columns = Vec::new()` **without `mut`**, which is a compiler-checked
      proof that nothing ever pushed to it. Every query planned there (`UNWIND …`, a bare
      `RETURN`, `WITH`) therefore aggregated with an empty grouping-key list, collapsing
      all input into one group and emitting only the aggregate column. Fixed by
      collecting `non_aggregate_aliases` in the two non-aggregate arms (guarded by
      `contains_aggregation` so an expression that merely *wraps* an aggregate never
      becomes a key) and promoting them under `has_aggregation`, mirroring the pattern
      path. Measured before/after on a live server: `UNWIND [1,1,2] AS v WITH v AS k,
      count(*) AS c RETURN k, c` went from columns `["c"]` / `[[3]]` to `["k","c"]` /
      k=1:c=2, k=2:c=1; the literal-key form `WITH 9 AS k` to `[[9,3]]`.
- [x] 1.2 Yield one row for an aggregate nested in an expression over empty input (`RETURN count(a) > 0`)
      — Scope as written UNDERSTATES the defect, confirmed by measurement: a nested
      aggregate returned `null` on **any** input, not only empty. `MATCH (n:P) RETURN
      count(n) > 0` → `[[null]]` over three matching nodes; `count(*) + 1` → null;
      `[count(*)]` → `[null]`; `CASE WHEN count(*) > 0 …` took the wrong branch. Root
      cause: an item was only recognised as an aggregation when the aggregate call was
      the *entire* projection expression, so a wrapping expression was planned as a plain
      projection and `count(...)` evaluated to null in the row evaluator. Fixing only the
      empty-input row count would have produced one row of `null`, so the honest fix is
      the general one.
      Mechanism: a new `lift_aggregations` (`queries/expressions.rs`) rewrites each
      aggregate call inside an expression to a synthetic `__agg_lift_N` column and returns
      the lifted calls. Both planner paths run it as a pre-pass, feeding each lifted call
      back through the *existing* per-function-name classification as a bare aggregate
      `ReturnItem` — so that 300-line match needed no edit — and emit a post-aggregation
      `Project` that computes the enclosing expression against the synthetic column. The
      empty-input row falls out for free: the synthetic bare aggregate hits the existing
      virtual-row path, and the wrapper is computed over it, so
      `MATCH (a:ZZZ) RETURN count(a) > 0` yields one row `[false]`.
      The narrow predecessor of this mechanism — a `collect`-only rewrite reachable only
      when the wrapper was a `FunctionCall` — was DELETED (`replace_nested_aggregations`
      plus its `has_nested_agg`/`__collect_arg_*` call site), not left beside the general
      one. A second divergent implementation of the same thing is the failure mode this
      epic has paid for repeatedly.
- [x] 1.3 Verify the property-access case and DISTINCT interaction did not regress
      — `MATCH (n:P) RETURN n.x AS k, count(*) AS c` and `RETURN DISTINCT n.x AS k,
      count(*) AS c` both still group correctly; `head(collect(...))`, `size(collect(...))`,
      bare `collect`, and bare `count` are unchanged. Suites: `--test cypher` 736 passed /
      0 failed; workspace 5888 passed / 0 failed.
      Additionally fixed, and NOT in the proposal: a grouping key was dropped by the
      post-aggregation projection itself — `MATCH (n:P) RETURN n.x AS k,
      head(collect(n.x)) AS h` returned only column `h`, on the pattern path that was
      otherwise correct. The post-aggregation `Project` now reproduces the written
      `RETURN` list position by position (pass-through by name for items that were not
      lifted), which restores both the key columns and the declared column order.
- [x] 1.4 Re-measure RC13's with-orderBy rows — part of that 58 is this defect
      — MEASURED, AND THE PREDICTION DOES NOT HOLD. `clauses/with-orderBy` did not
      improve by a single scenario (146 → 145). None of RC13's with-orderBy rows was
      this defect. This repeats the A3 lesson: the plan's per-root-cause scenario
      attributions are estimates, not carve-outs, and must be re-derived from the failure
      log rather than trusted. See §4 for the −1 and for where the category's aggregate
      scenarios actually fail.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
      — `docs/specs/cypher-subset.md` §Aggregations: new "Grouping keys" and "Aggregates
      nested in an expression" subsections, with the empty-input rule stated explicitly.
      `CHANGELOG.md` under `[3.0.0] — Unreleased`: "Fixed — Grouping keys, and aggregates
      nested inside an expression".
- [x] 2.2 Write tests covering the new behavior
      — 8 unit tests for `lift_aggregations` in `queries/expressions.rs` (bare call,
      `> 0`, non-zero starting index, two distinct aliases in a list, CASE when-result,
      map value, no-aggregate identity, `head(collect(…))`). 15 integration tests in
      `crates/nexus-core/tests/cypher/cypher_nested_aggregate_expression_test.rs` covering
      all four behavior groups; grouped results are compared order-insensitively and each
      test asserts on the column list *and* the row values — a column-only assertion would
      have missed the wrong counts, and a row-only assertion would have missed the dropped
      column.
- [x] 2.3 Run tests and confirm they pass
      — 8/8 unit, 15/15 integration, 736/736 in the `cypher` group.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green — 5888 passed, 0 failed,
      97 ignored (5865 before + 23 added here).
- [x] 3.3 Neo4j differential suite still 310/325 — re-run against a live Neo4j 2025.09.0
      (`docker start neo4j-diff`) with the release server on :15474: **325 total, 310
      passed, 0 failed, 15 skipped**, identical to the committed baseline. (The checklist
      line says 300/300; the suite has since grown to 325 with 15 environment-skipped
      spatial cases.)
- [x] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
      — Three measurements, not two: the post-change build twice (byte-identical reports)
      and a fresh HEAD baseline built in a `git worktree` on the same machine. The
      committed report's 2355 turned out to be a different sample — the same-machine HEAD
      baseline measures **2356** — so the honest delta is **2356 → 2364, +8**. The single
      apparent regression was isolated to one scenario and cleared; see §4.
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md` — regenerated by the
      harness (`NEXUS_TCK=1 cargo +nightly test -p nexus-core --test tck_opencypher
      --all-features`).

## 4. Measurements and residue

**TCK: 2356 → 2364 of 3868 (60.9% → 61.1%), +8 scenarios**, measured against a HEAD
baseline re-run on this machine in a `git worktree` — not against the committed report,
which recorded 2355 and is therefore a different sample of the same build.

| Category | Committed | HEAD baseline (re-measured) | After | Δ vs baseline |
|---|---:|---:|---:|---:|
| `clauses/return` | 29 | 30 | 37 | **+7** |
| `clauses/return-orderby` | 21 | 21 | 22 | +1 |
| `expressions/list` | 96 | 96 | 97 | +1 |
| `clauses/with-orderBy` | 146 | 146 | 145 | −1 → **cleared, see below** |

**The −1 is a coin-flip scenario, not a regression.** Two identical re-runs of the
post-change build produced byte-identical reports, so the usual category-noise defence
did not apply and the −1 could not be waved off. It was isolated with the harness's
JSONL failure log (`target/tck-failures.jsonl`, `NEXUS_TCK_FAILLOG`) — which records the
query and the actual result per failing scenario — down to a single one:

> `WithOrderBy2` `[23] Sort by an expression that is only partially orderable on a
> non-distinct binding table, but used in parts as a grouping key`
> `MATCH (a) WITH a.name AS name, count(*) AS cnt ORDER BY a.name + 'C' <dir> LIMIT 1
> RETURN name, cnt`

Baseline: the `ASC` example failed (returned `B,1`, wants `A,2`) and the `DESC` example
passed (`C,2`). After: `ASC` returns `C,2` and `DESC` returns `B,1` — the two coincidences
swapped. Neither build ever computed the ordering: run on four freshly started server
processes on the identical binary, `ASC LIMIT 1` answered **`A,2` / `B,1` / `A,2` /
`C,2`** — three different answers, so the scenario passes or fails by luck and the ±1 is
its own noise. Same class as the documented `Merge5[4]` coin-flip.

An intermediate hypothesis — that the promotion added in 1.1 was pulling the hidden
`__order_by_key_N` column into the grouping key and disturbing the sort — was REFUTED
before any code was touched: `pattern_lowering.rs:1463` sets
`may_project_hidden = !distinct && !has_aggregate`, so that hidden column is never
created for an aggregating query.

**Measured gap, uncovered by the above and NOT fixed here: `ORDER BY` over a non-projected
expression after an aggregation is silently not applied at all.** The sort key
`a.name + 'C'` never resolves to a column that exists after the Aggregate, so no ordering
happens and `LIMIT 1` returns an arbitrary group — a silently wrong answer that varies
between processes. This is the real content of RC13's with-orderBy bucket for these
scenarios, and it wants its own task alongside the `ORDER BY` hidden-key work.

**Measured gap, adjacent but out of scope — an aggregate whose ARGUMENT is an expression
returns null.** `MATCH (a:A) WITH a.num2 % 3 AS mod, sum(a.num + a.num2) AS sum RETURN
mod, sum` groups correctly (three groups) but every `sum` is `null`. The per-function-name
classification extracts the aggregated `column` only from `Expression::Variable` or
`Expression::PropertyAccess` and yields `None` for anything else. This is pre-existing —
the diff for this task does not touch that match — and it is what fails `WithOrderBy4 [11]
Sort by an aggregate projection` and `[12] Sort by an aliased aggregate projection`. It is
the mirror image of the defect fixed in 1.2 (there the aggregate was inside an expression;
here an expression is inside the aggregate) and it deserves its own task: the natural fix
is to project the argument expression into a synthetic column before aggregating, which is
the same lifting shape in the other direction.
