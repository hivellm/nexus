# Proposal: phase22_tck-ordering-cross-type-and-instant

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase C — F-137 (RC16) + F-123 (04-read-clauses.md)

## Why
One comparator, three sub-problems:

```
UNWIND [[], ['a'], ['a',1], [1], [1,'a'], [1,null], [null,1], [null,2]] AS l
WITH l ORDER BY l LIMIT 4 RETURN l          -> row[1] = [1]      (want ['a'])

UNWIND [1,'a',null,[1,2],0.2,'b'] AS x RETURN min(x), max(x)
  -> 'b', [1,2]                             (want [1,2], 1)

WITH time({hour:10,minute:0,timezone:'+01:00'}) AS x,
     time({hour:9,minute:35,second:14,nanosecond:645876123,timezone:'+00:00'}) AS d
RETURN x > d, x < d                         -> true, false       (want false, true)
```

(a) the total order **across value types**; (b) element-wise ordering **inside** lists
including nulls; (c) **instant** ordering for offset-aware temporals — `09:00Z` precedes
`09:35:14Z` even though `10:00` reads later than `09:35`.

An earlier task fixed cross-type ordering for the scalar case; this extends the same
comparator to lists, temporals, and the aggregate functions, which currently disagree
with `ORDER BY` and with each other.

## What Changes
- One comparator used by `ORDER BY`, `min()`, `max()`, and comparison operators.
- Cross-type total order per the spec, with null last ascending.
- Element-wise list ordering including nulls and mixed element types.
- Offset-aware `time`/`datetime` compare and sort by instant, not by wall-clock fields.

## TCK Impact
49 fails (with-orderBy 33, return-orderby 12, aggregation 4) plus ~20 temporal ordering rows.

## Impact
- Affected code: crates/nexus-core/src/executor/operators/project.rs (sort), crates/nexus-core/src/executor/eval/predicate.rs, the aggregate min/max implementations
- Breaking change: NO
- Dependencies: Runs after the temporal component tasks (needs correct values to order).
- User benefit: ORDER BY, min, and max agree with each other and with the spec
