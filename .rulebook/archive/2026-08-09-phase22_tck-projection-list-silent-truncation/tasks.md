## 1. Implementation
- [x] 1.1 Make an unconsumed projection item a hard parse error — no truncation, no dropped clause (regression test asserting the error, not the truncated shape)
  - Implemented as ONE guard at the top-level clause loop
    (`parser/clauses/mod.rs`): breaking out with input remaining is now an error
    naming what was left unread. That covers every truncation shape at once,
    because each ends the same way — the expression parser stops, the item loop
    sees no `,` and breaks, the clause loop finds no boundary and breaks, and the
    query is returned short. A single trailing `;` is accepted as a terminator.
  - **This does NOT make the truncating queries work** — the missing postfix forms
    are 1.2/1.3/1.4. It makes them FAIL instead of silently answering a different
    question, which is the whole point of the item.
  - Three tests encoded the old lenient behaviour and were corrected deliberately,
    each keeping its original intent:
    - `qpp_bare_parens_without_quantifier_is_not_qpp` asserted that
      `MATCH (a)(b) RETURN a` PARSES, with the pattern ending at `(a)` and the
      RETURN silently dropped. Juxtaposed node patterns are not Cypher (Neo4j
      rejects them); renamed to `…_is_rejected` and it now asserts the error plus
      the leftover text. The original intent — no QPP misparse — is what the error
      proves, since a QuantifiedGroup would have consumed `(b)`.
    - `test_group_by` passed `… RETURN a.name, count(*) AS count GROUP BY a.name`
      and asserted 2 columns. **Cypher has no `GROUP BY`** — grouping is implicit —
      so the assertion only held because the parser truncated there. Now asserts
      both halves: the implicit form returns 2 columns, and the `GROUP BY` form is
      rejected.
    - `cypher_wraps_executor_error` (nexus-server RPC) sent `NOT CYPHER` and
      expected "Cypher error"; that only reached the executor because the parser
      accepted it as a ZERO-clause query. Now split in two: the executor-error test
      uses `RETURN 1 / 0` (which parses), and a new `cypher_wraps_parse_error`
      covers the parse path — strictly more coverage than before.
  - **TCK cost measured, not assumed: −2, entirely in `clauses/with-orderBy`
    (147 → 145), with no other category moving at all.** So no accidental pass was
    lost anywhere else. An identical re-run of this build gives 145 and 146, and
    the category's measured band today spans 145–148, so the −2 sits inside it —
    at the low end, which is stated rather than rounded away.
  - Workspace gate: **5853 passed, 0 failed** (exit 0).
- [x] 1.2 Support `IS NULL` / `IS NOT NULL` as a postfix operator inside a larger expression, not only as a whole predicate
  - The check ran BEFORE the comparison operators, against the LEFT operand alone:
    in `false = true IS NULL`, `false` was tested for `IS`, `= true` was then
    consumed as a comparison, and the trailing `IS NULL` was left unread. Moved to
    after the comparison level (`parse_comparison_core` + a wrapper that loops
    trailing `IS [NOT] NULL`), which is both the correct openCypher precedence —
    `IS NULL` binds LOOSER than comparison — and what makes the item consumable.
  - 5 tests: the TCK three-spelling case asserting `a == c` and `b` differing (so
    the unparenthesised form reads as `(false = true) IS NULL`); the bare-operand
    control the old early check handled; `IS NOT NULL` over a comparison; the
    `IS` -without-`NULL` error; and comparison chaining, pinned because it shares
    this precedence level.
  - **TCK: neutral.** Total 2355 either way; only the two documented oscillators
    moved (`clauses/return` 29↔30, `with-orderBy` 145↔146). `expressions/precedence`
    did NOT move, which says its ~55 scenarios need 1.3's forms as well — worth
    knowing before that item is sized.
- [x] 1.3 Support `.prop` after a call result and after a parenthesised expression (`f(x).p`, `(expr).p`)
  - **The AST blocker was real and is resolved additively.**
    `Expression::PropertyAccess` holds `{ variable: String, property: String }` —
    the base is a NAME — and 75 call sites across 20 files read that field
    directly, so widening it was never the cheap option. Added a sibling variant
    `PropertyOf { base: Box<Expression>, property: String }` instead; the common
    `n.prop` form is untouched.
  - The compiler found every exhaustive match — **five**, not seventy-five:
    `can_evaluate_without_variables` (evaluable iff the base is), the projection
    evaluator (evaluate base, then `extract_property`, NULL base short-circuits to
    NULL), and three semantic-validation walkers (`child_exprs`,
    `collect_expr_binders`, `check_expr_references` — the property NAME is not a
    reference, the base is).
  - Parser emits it from one shared suffix loop used after `)` and after a
    function call's own index loop, so `f(x)[0].p` composes. `..` is never
    consumed as a property.
  - 8 tests: the proposal's `(list[1]).missing, (list[1]).existing` two-column
    shape; `startNode(r).id, endNode(r).id`; map literal; nested `(m.a).b`;
    non-container and NULL bases reading as NULL; the untouched `n.prop` control;
    and the two adjacent gaps below.
  - **Two adjacent parser gaps found while testing, pinned as KNOWN gaps rather
    than asserted as desired:** a slice on a bare variable (`l[1..3]`) is rejected
    by the index parser, and indexing a PARENTHESISED expression (`(l)[1]`) has no
    index loop at all. Both now fail loudly through 1.1's guard instead of
    truncating. Sharing the index/slice loop across postfix positions is the
    follow-up; the tests say to delete themselves when that lands.
  - **TCK: neutral.** 2353/2354 across two runs of this build; the only category
    that moves between identical runs is `clauses/with-orderBy` (144↔145), and
    `clauses/return` oscillates 29↔30 as always. **No other category moved at
    all**, which is the signal that matters for an AST change.
- [x] 1.4 Support comparison chaining at the precedence the spec requires
  - **Already implemented** before this task: `parse_comparison_expression` desugars
    `a < b < c` to `a < b AND b < c` and extends to further links. Verified
    (`1 < 2 < 3` is true, `1 < 2 < 1` is false) and pinned by test so the 1.2 move
    could not disturb it. Nothing to write.
- [x] 1.5 Re-measure and record the new counts for RC18 and RC19 so their tasks can be sized
  - **The proposal's attribution was wrong, and this is the item that proves it.**
    It carved ~90 fails out of the column-count/column-name buckets for this task
    ("precedence 55, return 15, map 9, merge 7, string 3"). All three parser items
    landed and the TCK total is unchanged — `expressions/precedence` did not move by
    a single scenario across 1.1, 1.2 and 1.3.
  - Measured now, for whoever sizes RC18/RC19 next:

    | Category | Pass/Total | Fails |
    |---|---:|---:|
    | `expressions/precedence` | 39/121 | 82 |
    | `expressions/literals` | 102/131 | 29 |
    | `clauses/return` | 29/63 | 33 |
    | `expressions/map` | 18/44 | 17 (+9 skipped) |
    | `expressions/string` | 26/32 | 6 |

  - So the forms are now PARSEABLE but the scenarios still fail for other reasons.
    RC18 and RC19 must be re-root-caused from the failure log rather than sized
    from this task's predicted carve-out — the carve-out does not exist.
  - What this task actually bought is not conformance: it is the removal of a class
    of silently-wrong answers. A query the parser cannot read in full now fails
    instead of being answered as a shorter, different query.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § Expression Forms and Parse Strictness: a query
    that cannot be read in full is rejected (with the two shapes that used to be
    accepted); `IS NULL` precedence with both readings shown; property access on a
    computed base including the call form; comparison chaining; and the two known
    gaps (slice on a bare variable, indexing a parenthesised expression) stated as
    gaps that now fail loudly.
- [x] 2.2 Write tests covering the new behavior
  - 13 across three files: `is_null_precedence_test.rs` (5),
    `property_of_computed_base_test.rs` (8, including the two known gaps pinned as
    gaps), plus the three pre-existing tests corrected in 1.1 — each keeping its
    original intent while asserting the corrected behaviour.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 727, 0 failures.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
  - **5865 passed, 0 failed** (exit 0).
- [x] 3.3 Neo4j differential suite still 300/300 (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`)
  - **310 passed / 0 failed / 15 skipped of 325 — 100%**, run against a live Neo4j
    2025.09.0 after all three parser changes. Worth running precisely because a
    parser that now REJECTS input is the change most likely to break a suite of
    real queries; nothing broke.
  - (The checklist's "300/300" wording is stale; the suite carries 325 cases.)
- [x] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
  - This task's categories did NOT improve — see 1.5, which is the honest finding
    rather than the predicted one. No category regressed either: every measurement
    was A/B'd against an identical re-run of the same build, and the only movement
    in any of them is the documented `clauses/with-orderBy` (144↔148 across the
    session) and `clauses/return` (29↔30) oscillators. Total across the three
    commits: 2355 → 2353/2354, i.e. flat within those two.
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
