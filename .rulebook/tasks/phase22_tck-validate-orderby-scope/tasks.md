## 1. Implementation
- [x] 1.1 Reuse/extend the scope helper from the variable-reuse task to model
      WITH scope narrowing — `semantic_validation/order_by_scope.rs`. Note the
      visible set is the WITH's output **plus its input**: sorting by a name the
      WITH consumed but did not forward is legal, and checking only the output
      rejects valid Cypher (found via a TCK regression).
- [x] 1.2 Reject an aggregation in ORDER BY where the spec forbids it —
      `InvalidAggregation`, judged per sub-expression so a projected aggregate
      wrapped in arithmetic (`ORDER BY $x + avg(p.age) - 1000`) stays legal.
- [~] 1.3 Reject out-of-scope variables in ORDER BY, SKIP, LIMIT, and downstream
      clauses — ORDER BY done (`UndefinedVariable`). SKIP/LIMIT cannot reference
      a variable at all (rejected earlier as `NonConstantExpression` by the
      skip-limit task), so there is nothing to add there. Downstream clauses
      still use the query-wide binder set and are not scope-checked.
- [x] 1.4 Confirm legal ORDER BY over a projected alias and over a hidden
      carried key still work — both pinned in
      `tests/executor/order_by_scope_test.rs`.

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
