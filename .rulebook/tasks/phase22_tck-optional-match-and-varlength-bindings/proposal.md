# Proposal: phase22_tck-optional-match-and-varlength-bindings

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase D — F-139 (RC22) (04-read-clauses.md)

## Why
The other `L`, and the one place where the failures are genuinely semantic rather than
surface-level. Three intertwined defects:

**(a) a failed OPTIONAL MATCH leaks partial bindings** (the known open issue, in its
OPTIONAL form):

```
MATCH (a:A), (c:C) OPTIONAL MATCH (a)-->(b)-->(c) RETURN b
  -> the C node                   (want null — the two-hop pattern has no match)
MATCH (a:Single) OPTIONAL MATCH (a)-[*]->(b) RETURN b
  -> includes the Single node itself                (want only the B nodes)
```

**(b) a var-length relationship variable must always be a list, even at `*1..1`:**

```
MATCH (a)-[r*1..1]->(b) RETURN r  -> a relationship  (want a one-element list)
```

**(c) var-length traversal counts and reuses relationships wrongly:**

```
MATCH (a:Blue)-[r*]->(b:Green) RETURN count(r)  -> 3   (want 1)
MATCH ()-->() WITH 1 AS x MATCH ()-[r1]->()<--() RETURN sum(r1.times) -> 97 (want 776)
MATCH ()-[r:EDGE]-() MATCH p = (n)-[*0..1]-()-[r]-()-[*0..1]-(m) RETURN count(p) -> 0 (want 32)
```

(c) needs the relationship-isomorphism scope from the earlier isomorphism task extended
across var-length segments and bound-relationship reuse. **Expect this task to need its
own decomposition;** do not attempt it in a single pass.

## What Changes
- A required or optional expand that fails must not leave a partial binding alive; the
  optional case yields null for every variable the pattern would have bound.
- A var-length relationship variable is always a list, at any quantifier including
  `*1..1` and `*0..n`.
- Relationship isomorphism holds across var-length segments and when a bound
  relationship variable is reused inside a later pattern.

## TCK Impact
57 fails (match 13, graph 10, delete 6, with 6, return 5, ...).

## Impact
- Affected code: crates/nexus-core/src/executor/operators/expand.rs and path.rs, crates/nexus-core/src/executor/planner/queries/relationships.rs
- Breaking change: NO
- Dependencies: Overlaps step 4 of phase22_tck-write-path-general-pipeline (null-row semantics) — coordinate.
- User benefit: OPTIONAL MATCH returns null instead of a leftover binding, and var-length matches count correctly
