# Proposal: phase22_tck-validate-variable-reuse

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-131 (RC06) (04-read-clauses.md)

## Why
Reusing one variable for two different entity kinds, in the same pattern or across
clauses, must fail at compile time. Nexus accepts all of it and returns 0 rows:

```
MATCH r = ()-[]-()  MATCH (r) RETURN r          -> succeeds, 0 rows   (want an error)
MATCH (p)-[]-()     MATCH p = ()-[]-() RETURN p -> succeeds, 0 rows   (want an error)
MATCH r = ()-[]-(), (r) RETURN r                -> succeeds, 0 rows   (want an error)
MATCH (a) CREATE (a)                            -> succeeds           (want an error)
MATCH (n $param) RETURN n                       -> parse error        (want InvalidParameterUse)
```

Required detail tokens: `VariableTypeConflict` when the kinds differ (node vs
relationship vs path), `VariableAlreadyBound` when a bound variable is re-bound by
`CREATE`/`MERGE`, `InvalidParameterUse` for a parameter used as a node predicate.

The semantic-validation stage already exists and already reads
`pattern.path_variable` at three sites (`semantic_validation/mod.rs:349,390,859`) —
this is a new check inside an existing stage.

## What Changes
- Track each variable's entity kind (node / relationship / path) across the clauses of
  a query and raise `VariableTypeConflict` on a conflicting re-bind.
- Raise `VariableAlreadyBound` when `CREATE`/`MERGE` re-binds an already-bound variable.
- Raise `InvalidParameterUse` for a parameter in a node-predicate position.

## TCK Impact
96 fails directly (match 91) **plus** the 64 unblocked by phase22_tck-parse-path-assignment-in-pattern-list.

## Impact
- Affected code: crates/nexus-core/src/executor/semantic_validation/mod.rs
- Breaking change: NO
- Dependencies: **Blocked by** phase22_tck-parse-path-assignment-in-pattern-list (its queries must parse before they can be validated).
- User benefit: a query that reuses a variable for two kinds of thing fails fast instead of returning nothing
