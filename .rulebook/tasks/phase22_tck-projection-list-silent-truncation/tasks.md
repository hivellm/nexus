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
- [ ] 1.2 Support `IS NULL` / `IS NOT NULL` as a postfix operator inside a larger expression, not only as a whole predicate
- [ ] 1.3 Support `.prop` after a call result and after a parenthesised expression (`f(x).p`, `(expr).p`)
- [ ] 1.4 Support comparison chaining at the precedence the spec requires
- [ ] 1.5 Re-measure and record the new counts for RC18 and RC19 so their tasks can be sized

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass

## 3. Gates (every item, no exceptions)
- [ ] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
- [ ] 3.3 Neo4j differential suite still 300/300 (`scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`)
- [ ] 3.4 TCK re-run: this task's categories improved, no category regressed against an identical re-run
- [ ] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
