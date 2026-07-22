# Proposal: phase0_fix-where-predicate-reparse-precedence

**Priority: CRITICAL — a WHERE clause whose author used parentheses to
override default operator precedence is silently re-evaluated with DEFAULT
precedence, flipping boolean results and arithmetic values; a WHERE
containing CASE, a list comprehension, or certain operators is silently
replaced with the literal predicate `"?"`.** Found during a query-planner
correctness audit; not previously reported.

## Why

The planner never carries the parsed WHERE `Expression` AST through to
execution. Instead it serializes it to a plain string via
`expression_to_string` (`crates/nexus-core/src/executor/planner/queries/expressions.rs:33-59`),
and the `Filter`/`OptionalFilter` operators later **re-parse that string**
(`executor/operators/filter.rs:96-97`):

```rust
let mut parser = parser::CypherParser::new(predicate.to_string());
let expr = parser.parse_expression()?;
```

`Operator::Filter { predicate: String }` and the WHERE-lowering site
(`strategy.rs:424-441`) confirm the predicate is stored and passed around as a
bare `String`, not the structured `Expression`.

`expression_to_string` renders `BinaryOp` as `"{left} {op} {right}"` with
**no parentheses**, regardless of how the original AST was grouped
(expressions.rs:33-59):

```rust
Expression::BinaryOp { left, op, right } => {
    let left_str = self.expression_to_string(left)?;
    let right_str = self.expression_to_string(right)?;
    let op_str = match op { /* ... */ };
    Ok(format!("{} {} {}", left_str, op_str, right_str))
}
```

The re-parse (`filter.rs:96-97`) applies the parser's normal precedence
climb — `OR < AND < comparison < additive < multiplicative`
(`executor/parser/expressions/precedence.rs:11-44`) — to the flattened
string. Any AST whose original grouping **overrode** default precedence
(i.e. any explicit parenthesization) is silently reparsed with the default
grouping instead, changing the predicate's meaning without any error.

### Triggers (confirmed by code inspection)

- `MATCH (n:Flag) WHERE (n.a OR n.b) AND n.c RETURN n` parses to
  `And(Or(a,b), c)`, serializes to the string `"n.a OR n.b AND n.c"`, and
  re-parses (default `AND` binds tighter than `OR`) to `Or(a, And(b,c))`. For
  `a=true, b=false, c=false`: the original predicate is `false`; the
  re-parsed one is `true` — the query returns a row it should exclude.
- `MATCH (n:M) WHERE (n.x - n.y) * n.z > 0 RETURN n` serializes to
  `"n.x - n.y * n.z > 0"`, which re-parses as `n.x - (n.y * n.z) > 0` instead
  of the intended `(n.x - n.y) * n.z > 0` — silently wrong arithmetic
  comparison.

A second, independent hazard on the same round-trip: `expression_to_string`
renders `Case`, list/pattern comprehensions, and any operator that reaches
its catch-all as the literal string `"?"` (expressions.rs:56, catch-all
:133). A WHERE clause containing any of these becomes the predicate string
`"?"`, which re-parses to something other than the intended expression (or
fails to parse as intended) — an independent correctness gap on the same
lossy path.

Projections (`RETURN`/`WITH` expressions that are not filters) are
unaffected — they carry the `Expression` AST structurally through execution.
Only `Filter`, `OptionalFilter`, and WHERE clauses attached to `WITH` take
this lossy string round-trip.

## What Changes

- Immediate, minimal fix: make `expression_to_string` parenthesize nested
  `BinaryOp`/`UnaryOp` operands whenever the child operator's precedence is
  lower than (or equal-but-non-associative with) the parent's, so the
  serialized string re-parses to the same tree under the grammar's default
  precedence rules.
- Preferred fix: stop round-tripping through a string entirely — change
  `Operator::Filter`/`Operator::OptionalFilter` to carry the parsed
  `Expression` AST directly, and delete the re-parse in
  `operators/filter.rs:96-97`. This removes both the precedence hazard and
  the `Case`/comprehension `"?"` hazard at the source, and removes the
  serialize/re-parse cost on every filtered row batch.
- If the AST-carrying fix is chosen, `expression_to_string`'s catch-all `"?"`
  rendering (expressions.rs:56,133) still needs auditing for any other
  consumer (e.g. diagnostic logging, EXPLAIN output) that legitimately wants
  a lossy display string — that usage should remain string-based and
  unaffected by this fix.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (WHERE clause / expression
  evaluation semantics)
- Affected code: `crates/nexus-core/src/executor/planner/queries/expressions.rs`
  (`expression_to_string` `:33-59`, catch-all `:133`),
  `crates/nexus-core/src/executor/planner/queries/strategy.rs` (WHERE
  lowering `:424-441`), `crates/nexus-core/src/executor/operators/filter.rs`
  (re-parse `:96-97`), `crates/nexus-core/src/executor/types.rs`
  (`Operator::Filter`/`OptionalFilter` predicate field type, if the
  AST-carrying fix is chosen)
- Breaking change: NO for query semantics — this is a correctness fix; if
  `Operator::Filter.predicate` changes type from `String` to `Expression`,
  that is an internal API change with no external surface
- User benefit: parenthesized WHERE clauses evaluate with the precedence the
  user wrote, not a silently different one; CASE/comprehension predicates in
  WHERE are evaluated correctly instead of degrading to `"?"`
- Related: `phase0_fix-plan-reorder-drops-predicates`,
  `phase0_fix-varlength-multi-reltype-dropped` — other CRITICAL planner
  wrong-result defects found in the same audit
