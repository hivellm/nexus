# Tasks: phase0_fix-where-predicate-reparse-precedence

WHERE clauses are lowered by serializing the parsed `Expression` to a plain
string (`expression_to_string`, `expressions.rs:33-59`) which
`Filter`/`OptionalFilter` later re-parse (`operators/filter.rs:96-97`).
`expression_to_string` emits `BinaryOp` as `"{left} {op} {right}"` with no
parentheses, so any explicit grouping in the original AST is lost; the
re-parse applies default precedence (`OR < AND < comparison < additive <
multiplicative`, `parser/expressions/precedence.rs:11-44`) to the flattened
string instead. Trigger: `MATCH (n:Flag) WHERE (n.a OR n.b) AND n.c RETURN n`
with `a=true,b=false,c=false` — original predicate is `false`, re-parsed
predicate is `true`, so the query returns a row it should exclude.

Order matters: reproduce the precedence flip and the `"?"` hazard with
failing tests first (§1), confirm the exact round-trip mechanism (§2) —
including which fix strategy to take, since the parenthesization fix and the
AST-carrying fix touch different code — before implementing (§3), so the
decision is made once with full context rather than discovered mid-edit.

## 1. Reproduce the loss first
- [ ] 1.1 Write a failing integration test: `MATCH (n:Flag) WHERE (n.a OR
  n.b) AND n.c RETURN n` against a node with `a=true, b=false, c=false` —
  assert the node is NOT returned. Confirm it fails today (the node IS
  returned, because the re-parsed predicate becomes `Or(a, And(b,c))`)
- [ ] 1.2 Write a failing test for the arithmetic case: `MATCH (n:M) WHERE
  (n.x - n.y) * n.z > 0 RETURN n` with values where `(x-y)*z > 0` is false
  but `x - (y*z) > 0` is true — assert the node is NOT returned. Confirm it
  is returned today
- [ ] 1.3 Write a failing test for the `"?"` hazard: a WHERE clause
  containing a CASE expression or list comprehension — assert it evaluates
  the CASE/comprehension correctly. Confirm today's behavior (the predicate
  string becomes literal `"?"` per `expressions.rs:56,133` and either fails
  to parse as intended or silently evaluates something else)
- [ ] 1.4 Confirm projections are unaffected: a `RETURN (n.a OR n.b) AND n.c
  AS flag` (not in WHERE) already returns the correct grouped result today,
  establishing the defect is specific to the Filter/OptionalFilter string
  round-trip, not expression evaluation in general

## 2. Confirm the mechanism and choose the fix
- [ ] 2.1 Trace `expression_to_string`'s `BinaryOp` arm (`expressions.rs:33-59`)
  and confirm it never emits parentheses regardless of the child expression's
  operator, and that `filter.rs:96-97` re-parses the resulting string with
  `CypherParser::new(predicate.to_string()).parse_expression()`
  using the grammar's default precedence (`precedence.rs:11-44`)
- [ ] 2.2 Confirm the WHERE-lowering site (`strategy.rs:424-441`) is the sole
  place that calls `expression_to_string` for a WHERE clause and stores the
  result in `Operator::Filter { predicate: String }` /
  `Operator::OptionalFilter { predicate: String, .. }`
- [ ] 2.3 Decide between the minimal fix (parenthesize nested BinaryOp/UnaryOp
  operands in `expression_to_string` so the string re-parses identically) and
  the structural fix (carry `Expression` in `Operator::Filter`/
  `OptionalFilter` and delete the re-parse at `filter.rs:96-97`). Record the
  decision and why in the proposal — note that only the structural fix also
  closes the independent `Case`/comprehension `"?"` hazard
  (`expressions.rs:56,133`) at the source
- [ ] 2.4 If the structural fix is chosen, enumerate every other consumer of
  `expression_to_string`'s output for a WHERE-derived predicate (diagnostic
  logging, EXPLAIN/plan-debug formatting, any serialization for caching) so
  none of them silently break when `Operator::Filter.predicate` changes type

## 3. Implement the fix
- [ ] 3.1 If the minimal fix: add parenthesization logic to
  `expression_to_string`'s `BinaryOp`/`UnaryOp` arms (`expressions.rs:33-59`,
  `:104-112`) — wrap a child operand in parens whenever its operator's
  precedence is lower than (or equal-but-non-associative with) the parent
  operator's, per the precedence order in `precedence.rs:11-44`
- [ ] 3.2 If the structural fix: change `Operator::Filter`/`OptionalFilter`'s
  `predicate` field from `String` to `Expression` (`executor/types.rs`),
  update the WHERE-lowering site (`strategy.rs:424-441`) to store the AST
  directly, and delete the re-parse in `operators/filter.rs:96-97` in favor
  of evaluating the carried `Expression` directly
- [ ] 3.3 Update every other construction site of `Operator::Filter`/
  `OptionalFilter` (and any exhaustive match on the predicate field) so the
  workspace compiles under the chosen fix
- [ ] 3.4 Make the §1 tests pass; re-run them to confirm the parenthesized
  boolean case, the arithmetic case, and (if the structural fix was chosen)
  the CASE/comprehension case all now evaluate correctly

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md` if it documents WHERE clause
  evaluation/lowering; add a CHANGELOG entry
- [ ] 4.2 Tests: the §1 regression tests pass; add nested-parenthesization
  cases for OR/AND/NOT and for the four arithmetic operator precedence
  levels; add an `OptionalFilter` (OPTIONAL MATCH ... WHERE) case since it
  takes the same lossy path
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-plan-reorder-drops-predicates`, `phase0_fix-varlength-multi-reltype-dropped`
  — other CRITICAL planner wrong-result defects found in the same audit
