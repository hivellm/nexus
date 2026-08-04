## 1. Implementation

> Order matters. 1.1 is diagnosis-only and must land its findings before 1.2
> chooses a mechanism; 1.3 cannot be split from 1.4 (fixing D1 alone would expose
> D3's over-strictness and produce wrong counts). See proposal.md for the
> reproductions and observed values of D1/D2/D3.

- [x] 1.1 Root-cause D1 (diagnosis only, no code change)
  - `EXPLAIN` shows no scan is emitted for the second clause's leading variable, so
    `execute_expand` finds no `source_var` in the row, `extract_entity_id` fails and
    every row is dropped. Full plan and reasoning in proposal.md § D1.
  - Precise site: the additional-pattern loop in
    `executor/planner/queries/strategy/pattern_lowering.rs` emits the driving scan
    **only inside `if !node.labels.is_empty()`** with no `else`, so an unlabeled node
    in a second/comma pattern is never bound. The first pattern's lowering does emit
    `AllNodesScan` for an unlabeled node; the two paths disagree.
  - Correction to the proposal's original table: `MATCH … MATCH (p)` was recorded as
    "works" on a `count(*)` of 1. It does not — `p` is unbound and projects as
    `Null`; the correct result is 2 rows. A `count(*)` probe masked it.
  - **No dependency on the comma task after all.** This item first claimed the fix
    would decorrelate the rows, because `AllNodesScan`'s dispatch rebuilds
    `result_set.rows` from `context.variables`. That was wrong — read from the code
    instead of run. `context.variables` is columnar and row-aligned, and
    `apply_cartesian_product` expands every column in lockstep, so the rebuild is a
    zip and correlation survives. Emitting the scan was sufficient; see 1.3.
- [x] 1.2 Implement relationship isomorphism in the `MATCH`/`Expand` chain (D0)
      with an **explicit** pattern scope.

      **The recorded double-lowering lead was WRONG — refuted before writing any
      code.** `select_start_pattern` (`strategy/start_pattern.rs:66-68`) returns
      `patterns[0]` unconditionally ("For MVP, just return the first pattern"), and
      the additional-pattern loop skips exactly index 0. No pattern is ever lowered
      twice, and no clause ever gets two scopes. The lead had been derived by
      reading one file and reasoning about another; one grep closed it.

      **Real root cause of the recurring `clauses/match-where` regression —
      `execute_optional_filter`, not `execute_expand`.**
      `operators/filter.rs` built its grouping key from *every* row key not listed
      in `optional_vars`:

      ```rust
      let all_vars = rows.first().map(|r| r.keys().collect())…;   // RAW row keys
      let mandatory_vars = all_vars.difference(&optional_set)…;   // any extra key groups
      ```

      An internal accumulator key is therefore treated as a mandatory grouping
      variable, and its value differs per candidate row (each candidate consumed a
      different relationship). Every candidate becomes its OWN group, and each
      group with no passing row emits its own NULL-padded row. That reproduces both
      earlier attempts' numbers exactly: `MatchWhere6` [2] → 2 padded rows instead
      of 1, [3] → 1 real + 1 padded instead of 1. It is also why both attempts'
      hand-written `OPTIONAL MATCH` control passed while the corpus broke — a
      one-edge graph with a directed slot yields ONE candidate, hence one group.
      The padding branch in `execute_expand` was a symptom, not the cause.

      **Shipped design — the scope is the `Pattern`, which the parser already makes
      one-per-clause.** `parse_pattern` (`parser/clauses/pattern.rs:33-41`) flattens
      comma-separated parts into a single `Pattern`, and `bound.rs:304` pushes one
      `Pattern` per `MATCH` clause. So `iso_scope: Option<u32>` on
      `Operator::Expand`, assigned in `add_relationship_operators`, gives D2 and 1.4
      by construction: comma parts share the scope because they share the pattern,
      separate clauses cannot share it because they are separate patterns. No
      positional marker (which `cost.rs`'s bucket-sort would reorder), and no reset
      step — the accumulator key `__nexus_iso_rels_<scope>` includes the scope.
      - `None` when the clause has fewer than two **single-hop** slots, so every
        single-hop pattern keeps a byte-identical row shape. That is what makes the
        three `MatchWhere6` shapes structurally immune rather than merely repaired:
        they have one hop per clause, so no bookkeeping key exists to fragment
        `execute_optional_filter`'s groups. A quantified hop is excluded from the
        count — it lowers to `VariableLengthPath`/`QuantifiedExpand`, which carry
        their own trail semantics.
      - Enforced in BOTH `execute_expand` paths: the source-less relationship scan
        only **seeds** (it builds rows from nothing, so it is always its clause's
        first hop — every later hop has a bound source, since the planner names an
        anonymous intermediate node `__tmp_N`), and the per-source loop **checks and
        extends**. Anonymous slots are covered because the id comes from the
        candidate, not from a bound variable.
      - Rejection happens inside the candidate loop, before the row is pushed, so
        `matched_for_this_source` stays honest and an OPTIONAL clause whose every
        candidate is rejected NULL-pads once, like any other no-match.
      - **The landmine itself is fixed, not avoided:** `mandatory_vars` now skips
        `__`-prefixed keys. The pre-existing `__nexus_anon_rel_identity` had the
        same latent hazard, and any future internal key would have inherited it.
      - Cost was again far below the proposal's "11 sites across 7 files": one
        construction site (`queries/relationships.rs`), two dispatch destructures,
        one planner test literal. `cost.rs` and the other planner tests match with
        `{ .. }`.
- [x] 1.3 Fix D1 using 1.1's diagnosis
  - `pattern_lowering.rs`'s additional-pattern loop now emits `AllNodesScan` for an
    **unlabelled** node too, guarded so a variable an earlier pattern already bound
    is never rescanned (which would discard its binding and re-drive the query from
    every node).
  - `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN count(*)` returns 1, and
    `RETURN x, y, p, q` returns one correlated row (`x = p`, `y = q`).
    `MATCH (x)-[r1]->(y) MATCH (p)` now returns the 2-row cartesian with `p` bound
    instead of one NULL-padded row.
  - 6 tests in `tests/cypher/multi_clause_match_binding_test.rs`, including the
    correlation assertion, the not-rescanned control and its positive two-hop
    counterpart, and a control that the labelled path is unchanged.
  - **TCK: net zero, with one open verification.** Attributable via A/B on an
    identical build (only `with-orderBy` moved between the two runs, 91↔92):
    `clauses/create` +1 and `clauses/match` +1 gained, `clauses/merge` −1 and
    `clauses/return` −1 lost; total 1820 before and after. `clauses/match-where` —
    the category the reverted isomorphism attempt regressed — stayed at 28.
  - **Both −1s resolved by revert-A/B — neither is a regression. Net: +2, zero
    regressions.** With the lowering change disabled in place: `create` 46 and
    `match` 139 (so both +1 are genuinely this change's), `merge` **24 either way**
    (the −1 was noise), `return` 30 vs 29.
  - The `return` −1 was then root-caused rather than filed as noise. Diffing the
    failing scenario names across the two runs identifies exactly one newcomer:
    `[9] Returning a projected map`, whose query is `RETURN {a: 1, b: 'foo'}` — **no
    `MATCH` at all**, so this lowering change cannot reach it. It fails on column-name
    key order: `"{b: 'foo', a: 1}"` vs `"{a: 1, b: 'foo'}"`.
  - **Newly identified noise source, worth its own fix:** `Expression::Map` is a
    `HashMap<String, Expression>`, so `expression_to_string` renders a map literal's
    keys in arbitrary order, and the unaliased column name is nondeterministic
    run-to-run. That is why `clauses/return` oscillates 29↔30. The map literal's
    column name should preserve source order — it needs an order-preserving map in
    the AST (or a sorted render, which would be deterministic but still not match the
    source). Not fixed here; it is unrelated to this task's subject.
- [x] 1.4 Confirm D3 is closed by 1.2 now that D1 no longer masks it: isomorphism
      must NOT apply across separate `MATCH` clauses.
  - Holds by construction (separate clauses are separate `Pattern`s, hence separate
    scopes), and asserted:
    `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q)` returns 1 on the one-relationship
    fixture while `MATCH (x)-[r1]->(y)-[r2]->(z)` returns 0 on the same fixture
    (`isomorphism_does_not_span_separate_match_clauses`).
  - The `OPTIONAL MATCH`-after-`MATCH` shapes that D3 predicted would break are
    green and pinned by three tests — see 2.2.
- [x] 1.5 Close D2: isomorphism spans comma-separated parts of one `MATCH`.
  - `MATCH (x)-[r1]->(y), (p)-[r2]->(q) RETURN count(*)` returns 0 on the
    one-relationship fixture, and 2 on a two-relationship fixture (the two ordered
    pairs of distinct relationships) — so the rule spans the comma without
    over-rejecting.
- [x] 1.6 Decide and implement the contract for repeating one relationship variable
      in a single pattern (`MATCH (x)-[r]-(y)-[r]-(z)`).
  - **Contract: reject at compile time.** Source, quoted rather than recalled:
    openCypher TCK `clauses/match/Match3.feature` scenario **[29] "Fail when
    re-using a relationship in the same pattern"** — `MATCH (a)-[r]->()-[r]->(a)
    RETURN r` must raise `a SyntaxError should be raised at compile time:
    RelationshipUniquenessViolation`. Zero rows would have been silently wrong.
  - Implemented as a semantic-validation check in a new
    `executor/semantic_validation/relationship_uniqueness.rs`. It runs BEFORE the
    pass's `is_fully_modeled` gate, because the rule is purely syntactic per pattern
    and no cross-clause scoping can make it ambiguous.
  - Deliberately narrow, per the pass's conservative-over-collection principle:
    per-`MATCH`-pattern only (re-using a relationship variable in a LATER clause is
    legal and is exactly what `MatchWhere6` [5] does), `MATCH` only
    (`CREATE`/`MERGE` rebinding is `VariableAlreadyBound`, already covered), and
    top-level relationship slots only (a QPP group's interior is left alone — a
    missed violation is a safe false negative, an invented one is not).
  - `semantic_validation.rs` was 1536 lines, so it became
    `semantic_validation/mod.rs` (pure `git mv`, no content change) and the new
    check lives in its own submodule rather than growing the offender.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md`: replace the implicit-scope isomorphism note with
    the explicit per-clause scope, state that the rule spans comma parts but not
    separate `MATCH` clauses, and document 1.6's contract.
  - The "Known gap: relationship isomorphism is NOT enforced in `MATCH`" block is
    gone, replaced by the enforced rule with worked examples for both sides of the
    clause boundary, the `RelationshipUniquenessViolation` contract with its source,
    and a narrowed residual gap: isomorphism is still not enforced *between* a
    single-hop slot and a variable-length/quantified segment of the same clause,
    because each carries its own rule and the shared scope stops at the operator
    boundary.
- [x] 2.2 Write tests covering the new behavior
  - New `tests/cypher/relationship_isomorphism_test.rs`; the sibling
    `undirected_self_loop_counting_test.rs` already covers the shipped self-loop fix
    and is not duplicated.
  - 12 tests. D0's single-pattern counts, named and anonymous; the over-rejection
    control (a genuine two-hop path over two DIFFERENT relationships still matches,
    directed and undirected); both TCK `countingSubgraphMatches` fixtures
    (`MATCH (:A)-->()--()` = 2, `MATCH ()-[]-()-[]-()` = 6); 1.4's cross-clause
    control; 1.5's comma cases, reused and distinct; 1.6's rejection asserting the
    detail token.
  - The three `OPTIONAL MATCH` shapes that both earlier attempts regressed are
    pinned with the TCK fixtures reproduced verbatim — the point being that the
    earlier controls used a one-edge graph and a directed slot, which cannot expose
    the group-fragmentation bug. Plus `MatchWhere6` [7], a TWO-hop `OPTIONAL MATCH`
    where the bookkeeping key IS present on rows reaching
    `execute_optional_filter`, which is what actually guards the landmine fix.
  - 5 more unit tests next to the new semantic check, including the legal
    cross-clause re-use and repeated anonymous slots.
- [x] 2.3 Run tests and confirm they pass.
  - `--test cypher` 648, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245 — 0 failures. `fmt` clean, `clippy --all-targets
    --all-features -D warnings` clean. Workspace `--no-fail-fast`: **5783 passed, 3
    failed**, those 3 being the flakes below.
  - Lib run reported 3 failures, all three passing in isolation and all three
    matching documented flaky families (`property_index_survives_restart`; the two
    `exists_` var-length tests that only flake in full-parallel runs). Not this
    change: the `exists` path does not go through `Operator::Expand`.
  - **TCK: `useCases/countingSubgraphMatches` 9/11 → 11/11 (100%)**,
    `clauses/match` 140 → 144, total 1819 → 1825. **`clauses/match-where` stayed at
    28** — the regression that killed both earlier attempts did not occur.
  - Noise measured, not assumed: two runs of the IDENTICAL build differ in exactly
    one category, `clauses/with-orderBy` (89 ↔ 91), which alone accounts for the
    whole total spread (1825 ↔ 1827). Everything else is stable across identical
    runs, so the per-category deltas above are attributable. The −2 in
    `with-orderBy` against the baseline's 91 is therefore noise, consistent with its
    documented 90→91→92 history; nothing here touches `ORDER BY`.
  - **Attribution against the failed attempt's own artifact.** Attempt three's
    failure log was still on disk (`crates/nexus-core/target/tck-failures.jsonl`,
    the run with `countingSubgraphMatches` at 0 failures and `match-where` at 9).
    Diffing failing scenario NAMES against this build: the three scenarios it broke
    — `MatchWhere6` [1], [2], [3] — are gone, with **zero** new failures in that
    category, and `clauses/match` additionally loses `[29] Fail when re-using a
    relationship in the same pattern`. So `clauses/match` +4 = the same 3
    isomorphism gains attempt three had, plus 1.6's uniqueness check.
  - Not claimed: `clauses/merge` +1 and `clauses/return` +1. Both are documented
    oscillators (`merge` measured "24 either way" under revert-A/B in 1.3; `return`
    oscillates 29↔30 on the map-literal key-order nondeterminism root-caused in
    1.3), and nothing in this change touches `MERGE` or column naming. Attributable
    gain is +6: `countingSubgraphMatches` +2 and `clauses/match` +4.
