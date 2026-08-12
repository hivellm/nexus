# Proposal: phase22_tck-aggregation-grouping-key

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase A — F-103 (RC02) + F-140 (02-projection-and-expressions.md)

## Why
An aggregating projection **discards** its non-aggregate column unless that column is
a property access on a graph variable, collapsing every input row into one group:

```
MATCH (n:P) RETURN n.x AS k, count(*) AS c            -> [[2,1],[1,2]]        OK
UNWIND [1,1,2] AS v WITH v AS k, count(*) AS c RETURN k, c
  -> columns ["c"], [[3]]                                                     WRONG
UNWIND [1,1,2] AS v WITH 9 AS k, count(*) AS c RETURN k, c
  -> columns ["c"], [[3]]                                                     WRONG
```

Both the column and the aggregate value are wrong, so the 64 attributed failures
understate the damage — it also inflates the with-orderBy row counts attributed to
RC13.

Same fix site, second defect (F-140): a bare aggregate over empty input correctly
yields one row, but wrapping it in any expression loses the row.

```
MATCH (a:ZZZ) RETURN count(a)      -> 1 row [0]     OK
MATCH (a:ZZZ) RETURN count(a) > 0  -> 0 rows         WRONG (want 1 row [false])
```

## What Changes
- Group by **every** non-aggregate projection item, whatever its expression shape —
  variable, literal, function call, arithmetic — not only property access.
- Keep the grouping-key column in the output so downstream clauses can name it.
- An aggregate nested inside an expression over empty input yields one row, like the
  bare aggregate does.

## TCK Impact
64 fails directly (Quantifier10..12 column names) plus a correctness share of RC13's 58 with-orderBy rows and F-140.

## Impact
- Affected code: crates/nexus-core/src/executor/operators/project.rs (aggregating-projection path)
- Breaking change: NO
- Dependencies: Runs after phase22_tck-chained-with-alias-null (same projection path).
- User benefit: GROUP BY works for computed keys, not just node properties
