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
- [ ] 1.2 Implement relationship isomorphism in the `MATCH`/`Expand` chain (D0)
      with an **explicit** pattern scope: add a pattern-scope id to
      `Operator::Expand`, assign it per `MATCH` clause in the planner (comma parts
      of one clause share it, separate clauses differ), and key an accumulator of
      consumed relationship ids on it. Enforce in BOTH `execute_expand` paths — the
      per-source loop and the source-less relationship scan, which is the one a
      fully-anonymous first slot (`()-[]-…`) takes.
      **Done when:** on the one-relationship fixture
      `MATCH (x)-[]-(y)-[]-(z) RETURN count(*)` is 0 and
      `MATCH (x)-[r1]-(y)-[r2]-(z) RETURN count(*)` is 0; TCK
      `useCases/countingSubgraphMatches` is 11/11; and `clauses/match-where` has NOT
      dropped from its pre-change value (A/B against an identical re-run — see
      proposal D3, this is exactly how the first attempt failed).
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
- [ ] 1.4 Confirm D3 is closed by 1.2 now that D1 no longer masks it: isomorphism
      must NOT apply across separate `MATCH` clauses.
      **Done when:** on the one-relationship fixture,
      `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q)` returns 1 (the same relationship
      may be bound by both clauses) while
      `MATCH (x)-[r1]->(y)-[r2]->(z)` still returns 0.
- [ ] 1.5 Close D2: isomorphism spans comma-separated parts of one `MATCH`.
      **Done when:** `MATCH (x)-[r1]->(y), (p)-[r2]->(q) RETURN count(*)` returns 0
      on the one-relationship fixture, and a two-relationship fixture still returns
      the correct non-zero count.
- [ ] 1.6 Decide and implement the contract for repeating one relationship variable
      in a single pattern (`MATCH (x)-[r]-(y)-[r]-(z)`). Before this chain it
      returned 2; the self-loop task's accumulator now makes it return 0 rows
      silently, whereas Neo4j rejects the query. Check Neo4j/openCypher behaviour
      before choosing, and state the source.
      **Done when:** the behaviour is deliberate and documented — either a
      compile-time error with its detail token, or zero rows with a spec note
      saying so and why.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update `docs/specs/cypher-subset.md`: replace the implicit-scope
      isomorphism note with the explicit per-clause scope, state that the rule spans
      comma parts but not separate `MATCH` clauses, and document 1.6's contract.
- [ ] 2.2 Write tests covering the new behavior in a new
      `tests/cypher/relationship_isomorphism_test.rs` (the sibling
      `undirected_self_loop_counting_test.rs` already covers the shipped self-loop
      fix and its controls — do not duplicate it). Cover: D0's single-pattern
      counts including both TCK fixtures; a control that a genuine two-hop path over
      two DIFFERENT relationships still matches, so the rule does not over-reject;
      the D1 two-clause count; the cross-clause reuse control from 1.4; the D2 comma
      cases, reused and distinct; and 1.6's chosen contract. The
      `OPTIONAL MATCH`-after-`MATCH` shape from proposal D3 must have its own test —
      it is the regression the first attempt shipped blind.
- [ ] 2.3 Run tests and confirm they pass. Full
      `cargo +nightly test --workspace --no-fail-fast` (a plain `--workspace` run
      aborts at the first failing target and silently skips the rest), plus a TCK
      run. Expect movement in `clauses/match`, `clauses/match-where` and
      `clauses/with`; A/B any category that drops against an identical re-run before
      calling it a regression, since this corpus has a documented run-to-run
      variance band.
