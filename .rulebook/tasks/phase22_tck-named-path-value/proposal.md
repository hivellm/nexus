# Proposal: phase22_tck-named-path-value

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-138 (RC09) (04-read-clauses.md)

## Why
A named path variable is never bound for a fixed-length or zero-length pattern:

```
MATCH p = (a) RETURN p                          -> null
MATCH p = (a:Q)-->(b) RETURN p                  -> null
MATCH p = (a:Q)-->(b) RETURN nodes(p), relationships(p), length(p) -> null, null, null
```

**Confirmed in source:** `pattern.path_variable` is consumed at exactly one place in the
planner — `planner/queries/relationships.rs:220`, which builds
`Operator::VariableLengthPath`. The fixed-length branch below it emits
`Operator::Expand` and drops the path variable; the zero-length case never reaches a
relationship operator at all. A path value therefore exists only for `*`-quantified
patterns.

This is also why `expressions/path` at 100% is not evidence of path support — those 7
scenarios never bind a path from a pattern.

## What Changes
- Bind a path value for `p = pattern` in the zero-length, fixed-length single-hop, and
  multi-hop fixed-length cases, alongside the existing variable-length case.
- The path must carry nodes and relationships in **pattern-written order**, including
  alternating directions, so `nodes()`, `relationships()`, and `length()` work.
- Cover `OPTIONAL MATCH p = ...` (null path when unmatched) and `MERGE p = ...`.

## TCK Impact
14 fails directly, and all 20 positive scenarios of `Match6.feature`.

## Impact
- Affected code: crates/nexus-core/src/executor/planner/queries/relationships.rs, crates/nexus-core/src/executor/operators/path.rs
- Breaking change: NO
- Dependencies: None; unblocks the `MERGE p = ...` step of phase22_tck-write-path-general-pipeline.
- User benefit: named paths work for ordinary fixed-length patterns, not only variable-length ones
