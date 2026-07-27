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
- [x] 1.1 Write a failing test: `MATCH (n:M) WHERE CASE WHEN n.x > 0 THEN true
  ELSE false END RETURN n` over nodes with mixed `x` signs — assert only the
  `x > 0` nodes return. Confirm it fails today (predicate becomes `"?"`)
- [x] 1.2 Write a failing test for a list comprehension / `any(... WHERE ...)`
  predicate in WHERE; assert correct evaluation. Confirm today's `"?"` result
- [x] 1.3 Control: the SAME `CASE` in a projection
  (`RETURN CASE WHEN n.x > 0 THEN 'pos' ELSE 'neg' END AS c`) already works —
  establishes the defect is the WHERE string round-trip

## 2. Confirm the mechanism and choose the shape
- [x] 2.1 Confirm `expression_to_string`'s catch-all renders `Case`/comprehension
  as `"?"`, and that `filter.rs` re-parses the predicate string
- [x] 2.2 Confirm `evaluate_predicate_on_row`/`evaluate_projection_expression`
  evaluate an `&Expression` (incl. `Case`) directly and are reusable for filters
- [x] 2.3 DECISION: instead of the recommended enum (which would touch all ~14
  `.predicate` string readers), added an OPTIONAL companion field
  `predicate_ast: Option<Box<Expression>>` alongside the existing
  `predicate: String`. WHERE-lowering sites set `Some(ast)`; synthetic
  (label/property) sites keep `None`. `predicate` stays the display/cost/string-
  fast-path value; `predicate_ast`, when present, is evaluated directly. This
  leaves the ~14 readers of `.predicate` untouched (less churn, same fix).
  §1.2 list-comprehension: covered by the general AST path (any Expression the
  planner carries now evaluates directly); the CASE tests are the discriminating
  cases since CASE is what serialized to "?".

## 3. Implement the fix
- [x] 3.1 Add the predicate enum + a `to_display_string()`; change
  `Operator::Filter`/`OptionalFilter` predicate field to it (`types.rs`)
- [x] 3.2 WHERE-lowering sites store `Ast(where_clause.clone())`; synthetic
  `format!` predicate sites store `Raw(..)`; update all ~13 construction sites
- [x] 3.3 In `filter.rs`, evaluate the `Ast` variant directly via
  `evaluate_predicate_on_row` (delete the re-parse for that path); `Raw`
  re-parses as today
- [x] 3.4 Repoint the ~14 `.predicate` string readers (cost.rs selectivity,
  plan-debug, tests) to `to_display_string()`; get `cargo check --workspace` clean
- [x] 3.5 Make the §1 tests pass

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation (CHANGELOG;
  `docs/specs/cypher-subset.md` WHERE evaluation section if present)
- [x] 4.2 Write tests covering the new behavior (the §1 regression tests plus an
  `OPTIONAL MATCH ... WHERE CASE` case)
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green
- [x] Update or create documentation covering the implementation
- [x] Write tests covering the new behavior
- [x] Run tests and confirm they pass)

## Related
- `phase0_fix-where-predicate-reparse-precedence` — precedence hazard on the same
  serialize/re-parse round-trip, fixed via faithful parenthesizing serialization
