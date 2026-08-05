# Proposal: phase22_tck-chained-with-alias-null

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase A — F-113 (RC17) (02-projection-and-expressions.md)

## Why
An `UNWIND`-bound variable renamed by a `WITH` and then re-projected by a second
`WITH` loses its value — the query succeeds and returns nulls.

```
UNWIND [5,1,4] AS i WITH i AS a RETURN a              -> [5,1,4]            OK
UNWIND [5,1,4] AS i WITH i AS a WITH a RETURN a       -> [null,null,null]   WRONG
UNWIND [5,1,4] AS i WITH i AS a WITH a AS b RETURN b  -> [null,null,null]   WRONG
UNWIND [5,1,4] AS i WITH i WITH i RETURN i            -> [5,1,4]            OK
MATCH (n:P) WITH n.x AS a WITH a RETURN a             -> [1,1,2]            OK
```

Renaming once is fine. Re-projecting without renaming is fine. Renaming *and then*
re-projecting is not. Its TCK footprint is small, but silent data loss in a common
clause shape outranks scenario count.

## What Changes
- Root-cause why the second `WITH` cannot resolve an alias minted by the first over an
  `UNWIND` source, while the same shape over a `MATCH` source resolves.
- Fix the resolution so an alias is a first-class binding for every downstream clause,
  regardless of what produced the value it renames.

## TCK Impact
Near zero directly; unblocks honest measurement of RC13 (58 with-orderBy rows).

## Impact
- Affected code: crates/nexus-core/src/executor/planner/queries/planner_core/ (bound.rs / segments.rs), crates/nexus-core/src/executor/operators/project.rs
- Breaking change: NO
- Dependencies: None — do first.
- User benefit: chained WITH projections stop silently returning null
