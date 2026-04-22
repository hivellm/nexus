# Proposal: Quantified Path Patterns (QPP — Cypher 25 / GQL)

## Why

Quantified Path Patterns are the marquee feature of Cypher 25 (the
GQL-aligned release) and already shipped in Neo4j 5.9+. They let a
user quantify whole path segments — not just individual relationships
— so traversals that previously required UNWIND + recursion become
single pattern expressions.

Example: "every transitive `REPORTS_TO` chain up to length 5 where
each intermediate manager has `role: 'manager'`":

```cypher
MATCH (e:Employee)
      ( (x)-[:REPORTS_TO]->(m:Manager {role: 'manager'}) ){1,5}
      (ceo:CEO)
RETURN e, ceo
```

Today Nexus only supports the Cypher 9 shorthand
`-[:TYPE*1..5]->` which quantifies a single relationship. QPP
quantifies a whole parenthesised group, letting intermediate nodes be
matched, filtered, and even produce path variables.

Without QPP:

- Nexus cannot execute Cypher 25 / GQL test suites.
- Users must rewrite modern Neo4j queries into verbose `UNWIND` +
  `CALL { }` shapes.
- The openCypher TCK advertises ~140 QPP scenarios that remain
  red for Nexus.

This is the single largest syntactic gap between Nexus and current
Cypher.

## What Changes

- Parser: new production `quantifiedPathPattern := '(' pathFragment ')' quantifier`
- Quantifier: `{m,n}`, `{m,}`, `{,n}`, `{n}`, `+`, `*`, `?`.
- AST: new `PatternPart::Quantified(Box<PathFragment>, Quantifier)`.
- Planner: new operator `QuantifiedExpand` that iteratively
  applies a sub-pattern `m..n` times and emits bindings for inner
  variables as lists (`x:LIST<NODE>`, `m:LIST<NODE>`).
- Executor: introduces per-iteration binding contexts, backtracks
  on dead-ends, enforces cycle policy (same as variable-length paths).
- Type system: inner variables of a quantified pattern are promoted
  to LIST types in the outer scope, per GQL spec.
- `shortestPath(...)` and `allShortestPaths(...)` extended to accept
  quantified patterns.

**BREAKING**: none. The new grammar is a strict superset; existing
queries parse identically. Variable-length paths (`*m..n`) continue
to work and are internally rewritten to QPP as a unification step.

## Impact

### Affected Specs

- NEW capability: `cypher-quantified-path-patterns`
- MODIFIED capability: `cypher-pattern-matching` (adds QPP clause)
- MODIFIED capability: `cypher-shortest-path` (quantified-path inputs)

### Affected Code

- `nexus-core/src/executor/parser/patterns.rs` (~300 lines added)
- `nexus-core/src/executor/parser/ast.rs` (~60 lines added)
- `nexus-core/src/executor/plan/mod.rs` (~400 lines added, new op)
- `nexus-core/src/executor/operators/quantified_expand.rs` (NEW, ~700 lines)
- `nexus-core/src/executor/eval/types.rs` (~80 lines, list-promotion)
- `nexus-core/tests/qpp_tck.rs` (NEW, ~1200 lines TCK port)

### Dependencies

- Requires: `phase6_opencypher-quickwins` for dynamic property access
  (QPP often uses `{prop: $var}` inside inner patterns).
- Unblocks: `phase6_opencypher-apoc-ecosystem` (APOC path procedures
  expect QPP-friendly AST).

### Timeline

- **Duration**: 4–6 weeks
- **Complexity**: High — grammar reshape + new planner op
- **Risk**: Medium — must preserve existing variable-length path
  semantics exactly while extending the pattern grammar
