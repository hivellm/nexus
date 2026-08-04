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
  - **Confirmed dependency (was a suspicion):** emitting the missing scan is not
    sufficient. `AllNodesScan`'s dispatch clears `result_set.rows` and rebuilds via
    `materialize_rows_from_variables`, a cartesian over independent per-variable
    lists, which decorrelates the `(x, y, r1)` tuples an `Expand` produced. Fixing
    D1 therefore requires the correlation-preserving materialization owned by
    `phase21_tck-comma-pattern-binding-materialization`. Do them together, or that
    one first — otherwise D1 trades zero rows for wrong rows.
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
- [ ] 1.3 Fix D1 using 1.1's diagnosis.
      **Done when:** `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN count(*)`
      returns 1 on the one-relationship fixture.
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
