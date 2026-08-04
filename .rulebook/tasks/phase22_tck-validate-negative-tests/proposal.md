# Proposal: phase22_tck-validate-negative-tests

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase D — F-151 (RC20) (05-write-clauses.md)

## Why
96 heterogeneous but mechanical failures — each is "the engine accepts something the
spec rejects", each needs a check plus a detail token. Grouped by fix site:

**Function and projection validation** (return 4, graph 2, quantifier 12):
```
MATCH (a) RETURN foo(a)                      -> succeeds  (want an error: unknown function)
MATCH (r) RETURN type(r)                     -> succeeds  (want an error: node passed to type())
RETURN none(x IN ['Clara'] WHERE x % 2 = 0)  -> succeeds  (want an error: modulo on a string)
```

**Literal bounds** (literals 24):
```
RETURN -9223372036854775808 AS literal -> syntax error  (this is i64::MIN — a valid literal)
RETURN  9223372036854775808 AS literal -> wrong message (want token IntegerOverflow)
RETURN -9223372036854775809 AS literal -> wrong message (want token IntegerOverflow)
```
The first is a real bug: the magnitude is parsed before the sign is applied.

**Write-side validation** (delete 4, merge 6, create 3):
```
MATCH (a) DELETE x         -> succeeds       (want an error: undefined variable)
MATCH (n) DELETE n:Person  -> succeeds       (want an error: DELETE takes no label)
MATCH (a) CREATE (a)       -> succeeds       (want an error: already bound)
CREATE ()-->()             -> Uncategorized  (want SyntaxError kind)
CREATE (a)<-[:FOO]->(b)    -> wrong message  (want token RequiresDirectedRelationship)
```

**List/map argument validation** (list 15, boolean 8, map 6): argument counts and kinds
for `range`, `reduce`, list slices, and map access.

## What Changes
- Unknown-function and wrong-entity-kind checks in the projection validator.
- `i64::MIN` accepted; out-of-range integer literals carry the `IntegerOverflow` token.
- Write-side checks: undefined variable in DELETE, label in DELETE, already-bound
  CREATE, untyped/undirected/bi-directed CREATE relationships with the right kind and
  token.
- Argument-count and -kind checks for the list and map builtins.

## TCK Impact
96 fails (literals 24, list 15, quantifier 12, boolean 8, merge 6, map 6, ...).

## Impact
- Affected code: crates/nexus-core/src/executor/semantic_validation/mod.rs, crates/nexus-core/src/executor/parser/expressions/literal parsing, crates/nexus-core/src/error.rs
- Breaking change: NO
- Dependencies: **Blocked by** phase22_tck-runtime-type-guards (needs the error-kind table).
- User benefit: invalid queries are rejected with the right error instead of quietly returning nothing
