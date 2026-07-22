# Tasks: phase0_fix-where-predicate-reparse-precedence

WHERE clauses are lowered by serializing the parsed `Expression` to a plain
string (`expression_to_string`) which `Filter`/`OptionalFilter` later re-parse
(`operators/filter.rs`). `expression_to_string` emitted `BinaryOp` as
`"{left} {op} {right}"` with no parentheses, so explicit grouping was lost; the
re-parse applied default precedence to the flattened string. Trigger:
`MATCH (n:Flag) WHERE (n.a OR n.b) AND n.c` with `a=true,b=false,c=false` —
original is `false`, re-parsed is `true`, returning a row it should exclude.
Fixed by faithful (fully-parenthesizing) predicate serialization. Landed in
commit e9280c9b.

DECISION (§2.3): took the MINIMAL fix (parenthesize compound operands) rather
than the structural AST-carrying fix. Rationale: the precedence bug's root cause
IS the lossy serialization of grouping, so making serialization faithful is a
root-cause fix for THIS bug — provably correct (wrapping every compound child
removes all precedence ambiguity, no precedence table needed), near-zero blast
radius (one function + repointing 5 call sites; no field-type change, no touching
the 13 Filter construction sites / 14 string readers / cost estimator). The
independent `CASE`/list-comprehension `"?"` hazard genuinely CANNOT be fixed at
the string layer (a string can't represent those nodes) and needs the structural
AST-carrying change — filed separately as
`phase0_fix-where-predicate-case-comprehension-lost`.

## 1. Reproduce the loss first
- [x] 1.1 Failing boolean test written + confirmed failing pre-fix:
  `(n.a OR n.b) AND n.c` returned the excluded row before the fix
  (`parenthesised_or_then_and_respects_grouping_over_booleans`)
- [x] 1.2 Failing arithmetic test: `(n.x - n.y) * n.z > 0` — wrong grouping
  pre-fix (`parenthesised_subtract_then_multiply_respects_grouping_over_arithmetic`)
- [x] 1.3 The `"?"` (CASE/comprehension) hazard is OUT OF SCOPE for this fix —
  it needs the structural AST-carrying change; filed as
  `phase0_fix-where-predicate-case-comprehension-lost`
- [x] 1.4 Confirmed projections are unaffected — `RETURN (n.a OR n.b) AND n.c AS
  flag` already returns the correct grouped result (projections carry the AST);
  covered by the passing control test
  `projected_grouped_boolean_expression_already_evaluates_correctly`

## 2. Confirm the mechanism and choose the fix
- [x] 2.1 Confirmed `expression_to_string`'s `BinaryOp` arm never emitted parens
  and `filter.rs` re-parses the string under default precedence
- [x] 2.2 Confirmed WHERE predicates flow into `Operator::Filter`/`OptionalFilter`
  from strategy.rs / planner_core.rs (5 live lowering sites; the dispatch.rs
  `ast_to_operators` path is dead — a different `Executor::expression_to_string`,
  zero callers)
- [x] 2.3 Decided the MINIMAL fix (see DECISION above); recorded rationale
- [x] 2.4 N/A — minimal fix keeps the `String` field; no consumer type change

## 3. Implement the fix
- [x] 3.1 Split `expression_to_string` into `expr_to_string_impl(expr, parenthesize)`;
  when `parenthesize`, wrap every compound (`BinaryOp`/`UnaryOp`) child operand
  in `(...)` via a `render_operand` closure; propagate the flag through every
  nested position (function args, list/map elements, array index, IS NULL,
  EXISTS inner WHERE). Added `predicate_to_string` (parenthesize=true) and kept
  `expression_to_string` (parenthesize=false) for all display callers
- [x] 3.2 N/A (minimal fix chosen)
- [x] 3.3 Repointed the 5 live WHERE lowering sites to `predicate_to_string`
  (strategy.rs:425/1051, planner_core.rs:759/825/1197); synthetic inline-property
  predicates and dead paths correctly untouched; workspace compiles clean
- [x] 3.4 The §1 tests pass; boolean, arithmetic, OPTIONAL MATCH WHERE, and
  NOT-nesting all evaluate correctly now

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — CHANGELOG
  entry added ("WHERE clause parentheses now preserve operator precedence").
  `docs/specs/cypher-subset.md` documents no predicate-lowering internals, so no
  spec change warranted
- [x] 4.2 Write tests covering the new behavior —
  `where_predicate_precedence_test.rs`: boolean + arithmetic precedence flips, an
  `OPTIONAL MATCH ... WHERE` (OptionalFilter) case, a NOT/AND nesting case, and a
  RETURN-projection control
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all` + clippy
  clean (pre-commit hook); full workspace `cargo +nightly test --workspace` green
  (5092 passed / 0 failed)

## Related
- `phase0_fix-where-predicate-case-comprehension-lost` — the CASE/comprehension
  `"?"` hazard on the same round-trip (needs the structural AST-carrying fix)
- `phase0_fix-plan-reorder-drops-predicates`, `phase0_fix-varlength-multi-reltype-dropped`
  — other CRITICAL planner wrong-result defects from the same audit
