# Proposal: phase22_tck-skip-limit-semantics

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase B — F-133 (RC13) (04-read-clauses.md)

## Why
Four independent SKIP/LIMIT defects:

```
MATCH (n) RETURN n LIMIT 0                            -> 3 rows   (want 0; LIMIT 0 read as "no limit")
MATCH (n) RETURN n ORDER BY n.name SKIP $skipAmount    -> 5 rows   (want 3; parameter ignored)
MATCH (p:Person) RETURN p.name SKIP 1.5                -> succeeds (want an error)
UNWIND [1,2,3,4] AS i WITH i LIMIT 2 RETURN sum(i)     -> 10       (want 3; LIMIT dropped before an aggregation)
MATCH (a) WITH a, a.bool AS b WITH a, b ORDER BY b LIMIT 3 RETURN a, b -> 5 rows (want 3)
```

`LIMIT 0` and the parameter form are one-liners. The "dropped before an aggregation"
case is the 58 with-orderBy rows and **overlaps
phase22_tck-aggregation-grouping-key** — a collapsed grouping key also collapses the
row count, so re-measure after that task before sizing this one.

## What Changes
- `LIMIT 0` returns zero rows.
- `SKIP $p` / `LIMIT $p` resolve parameters.
- A float or negative argument raises the spec's error kind (`InvalidArgumentType`,
  `NegativeIntegerArgument`), as does a non-constant expression.
- A `WITH ... LIMIT`/`SKIP` is applied before the next clause consumes the rows,
  including when that clause aggregates.

## TCK Impact
89 fails (with-orderBy 58, return-skip-limit 12, create 8, delete 8) — re-measure after the grouping-key task.

## Impact
- Affected code: crates/nexus-core/src/executor/operators/project.rs, crates/nexus-core/src/executor/planner/queries/planner_core/
- Breaking change: NO
- Dependencies: Re-measure after phase22_tck-aggregation-grouping-key.
- User benefit: LIMIT and SKIP mean what they say, including with parameters and before aggregation
