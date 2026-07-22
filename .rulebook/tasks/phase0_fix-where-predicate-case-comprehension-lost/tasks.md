# Tasks: phase0_fix-where-predicate-case-comprehension-lost

`expression_to_string` (`crates/nexus-core/src/executor/planner/queries/expressions.rs`)
has no arm for `Expression::Case` (and list/pattern comprehensions); they fall
through its `_ => Ok("?".to_string())` catch-all. WHERE predicates are lowered
to that string and re-parsed in `operators/filter.rs`, so a WHERE containing a
`CASE`/comprehension becomes the predicate `"?"` and evaluates wrongly. Only
carrying the AST fixes it (a string cannot represent these nodes). The
`&Expression`->bool evaluator already exists (`evaluate_predicate_on_row`) and
is what projections use, which is why projected `CASE` works but WHERE `CASE`
does not.

## 1. Reproduce the loss first
- [ ] 1.1 Write a failing test: `MATCH (n:M) WHERE CASE WHEN n.x > 0 THEN true
  ELSE false END RETURN n` over nodes with mixed `x` signs — assert only the
  `x > 0` nodes return. Confirm it fails today (predicate becomes `"?"`)
- [ ] 1.2 Write a failing test for a list comprehension / `any(... WHERE ...)`
  predicate in WHERE; assert correct evaluation. Confirm today's `"?"` result
- [ ] 1.3 Control: the SAME `CASE` in a projection
  (`RETURN CASE WHEN n.x > 0 THEN 'pos' ELSE 'neg' END AS c`) already works —
  establishes the defect is the WHERE string round-trip

## 2. Confirm the mechanism and choose the shape
- [ ] 2.1 Confirm `expression_to_string`'s catch-all renders `Case`/comprehension
  as `"?"`, and that `filter.rs` re-parses the predicate string
- [ ] 2.2 Confirm `evaluate_predicate_on_row`/`evaluate_projection_expression`
  evaluate an `&Expression` (incl. `Case`) directly and are reusable for filters
- [ ] 2.3 Decide the field shape (recommended: enum
  `FilterPredicate { Ast(Box<Expression>), Raw(String) }` — WHERE uses `Ast`,
  synthetic predicates keep `Raw`); record the decision

## 3. Implement the fix
- [ ] 3.1 Add the predicate enum + a `to_display_string()`; change
  `Operator::Filter`/`OptionalFilter` predicate field to it (`types.rs`)
- [ ] 3.2 WHERE-lowering sites store `Ast(where_clause.clone())`; synthetic
  `format!` predicate sites store `Raw(..)`; update all ~13 construction sites
- [ ] 3.3 In `filter.rs`, evaluate the `Ast` variant directly via
  `evaluate_predicate_on_row` (delete the re-parse for that path); `Raw`
  re-parses as today
- [ ] 3.4 Repoint the ~14 `.predicate` string readers (cost.rs selectivity,
  plan-debug, tests) to `to_display_string()`; get `cargo check --workspace` clean
- [ ] 3.5 Make the §1 tests pass

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation (CHANGELOG;
  `docs/specs/cypher-subset.md` WHERE evaluation section if present)
- [ ] 4.2 Write tests covering the new behavior (the §1 regression tests plus an
  `OPTIONAL MATCH ... WHERE CASE` case)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)

## Related
- `phase0_fix-where-predicate-reparse-precedence` — precedence hazard on the same
  serialize/re-parse round-trip, fixed via faithful parenthesizing serialization
