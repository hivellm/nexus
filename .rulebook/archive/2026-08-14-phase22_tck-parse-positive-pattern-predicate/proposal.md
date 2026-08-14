# Proposal: phase22_tck-parse-positive-pattern-predicate

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase B — F-132 (RC08) (04-read-clauses.md)

## Why
A pattern is accepted as a `WHERE` predicate only after `NOT`, or inside
`exists { ... }`. The bare positive form and the function-style `exists(pattern)` both
fail to parse:

```
MATCH (n) WHERE (n)-[:T]->()             RETURN n -> syntax error at the '-'  WRONG
MATCH (n) WHERE NOT (n)-[:T]->()         RETURN n -> works                    OK
MATCH (n) WHERE exists { (n)-[:T]->() }  RETURN n -> works                    OK
MATCH (n) WHERE exists((n)-[:T]->())     RETURN n -> syntax error             WRONG
```

Because the `NOT` path works, the evaluation machinery is already wired — this is a
grammar entry point, not a feature.

## What Changes
- Parse a pattern as a predicate wherever a boolean expression is legal: bare in
  `WHERE`, inside `exists(...)`, and composed with `AND`/`OR`/`XOR`.
- Route all forms to the same evaluation path the `NOT` form already uses.

## TCK Impact
34 fails — `expressions/pattern` 31 of 50 scenarios, plus match-where, with-where, existentialSubqueries.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/expressions/precedence.rs, crates/nexus-core/src/executor/parser/clauses/pattern.rs
- Breaking change: NO
- Dependencies: None — independent.
- User benefit: pattern predicates can be written the way the spec and Neo4j docs show them
