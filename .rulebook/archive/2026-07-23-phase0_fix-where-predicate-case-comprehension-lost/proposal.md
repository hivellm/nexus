# Proposal: phase0_fix-where-predicate-case-comprehension-lost

**Priority: HIGH — a WHERE clause containing a `CASE` expression, a list/pattern
comprehension, or any expression node `expression_to_string` cannot render is
serialized to the literal string `"?"`, which re-parses to something other than
the intended predicate (or fails), silently producing wrong results.** This is
the second, independent hazard on the WHERE serialize/re-parse round-trip; the
precedence hazard on the same path was fixed separately in
`phase0_fix-where-predicate-reparse-precedence` (faithful parenthesizing
serialization). This one cannot be fixed at the string layer — a string simply
cannot represent these nodes — so it requires carrying the AST.

## Why

WHERE predicates are lowered by serializing the parsed `Expression` to a string
(`expression_to_string`, `crates/nexus-core/src/executor/planner/queries/expressions.rs`)
stored in `Operator::Filter { predicate: String }` / `Operator::OptionalFilter`,
and re-parsed at evaluation time (`operators/filter.rs`). `expression_to_string`
has no arm for `Expression::Case` (and other complex nodes); they fall through
its catch-all `_ => Ok("?".to_string())`. A WHERE clause like
`MATCH (n) WHERE CASE WHEN n.x > 0 THEN true ELSE false END RETURN n` therefore
becomes the predicate string `"?"`, which does not re-parse to the intended
CASE — the query silently returns the wrong rows.

The precedence-hazard fix deliberately scoped this out: parenthesization makes
the serialization faithful for representable nodes, but `CASE`/comprehensions
are not representable as a re-parseable string at all. The evaluator that would
fix this already exists and is proven: `evaluate_predicate_on_row` /
`evaluate_projection_expression` (`eval/helpers.rs`, `eval/mod.rs`) evaluate an
`&Expression` — including `CASE` — directly against a row, and projections
(`RETURN`/`WITH`) already use it, which is why projected CASE works but WHERE
CASE does not.

## What Changes

Carry the `Expression` AST through to the `Filter`/`OptionalFilter` operators
instead of a re-parsed string. Recommended shape (lowest risk given ~13
construction sites, several of which build synthetic `format!` string
predicates for inline label/property checks): make the predicate field an enum
`FilterPredicate { Ast(Box<Expression>), Raw(String) }`:

- WHERE-clause lowering stores `Ast(where_clause.clone())`; `filter.rs`
  evaluates it directly via `evaluate_predicate_on_row` (no re-parse) — closing
  both the `"?"` hazard and the residual re-parse cost for user WHERE clauses.
- Synthetic predicate constructions (label checks, property-equality; simple,
  no `CASE`/comprehension) keep `Raw(String)` and re-parse as today — avoiding
  the need to hand-build ASTs for them.
- String readers (cost-estimator selectivity heuristics in `cost.rs`,
  EXPLAIN/plan-debug formatting, test assertions) call a `to_display_string()`
  on the enum.

Then delete the re-parse in `operators/filter.rs` for the `Ast` path. Audit
`expression_to_string`'s `"?"` catch-all for any remaining consumer that
legitimately wants a lossy display string (diagnostic logging, EXPLAIN) — those
stay string-based.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (WHERE clause evaluation)
- Affected code: `crates/nexus-core/src/executor/types.rs`
  (`Operator::Filter`/`OptionalFilter` predicate field),
  `crates/nexus-core/src/executor/operators/filter.rs` (evaluate AST directly;
  delete re-parse), the ~13 `Filter`/`OptionalFilter` construction sites
  (`strategy.rs`, `planner_core.rs`, `dispatch.rs`), and the ~14 `.predicate`
  string readers (`cost.rs` selectivity, plan-debug formatting, tests)
- Breaking change: NO for query semantics — correctness fix; the field-type
  change is internal (Operator derives only Debug/Clone, no serde, so no wire
  or cache format is affected)
- User benefit: `WHERE CASE ... END` and `WHERE` clauses containing list/pattern
  comprehensions evaluate correctly instead of degrading to `"?"`
- Related: `phase0_fix-where-predicate-reparse-precedence` (precedence hazard on
  the same round-trip, fixed via faithful serialization);
  `phase0_fix-plan-reorder-drops-predicates`,
  `phase0_fix-varlength-multi-reltype-dropped` (same correctness audit)
