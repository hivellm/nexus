# Proposal: phase22_tck-quantifier-standalone-projection

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase A — F-101 (RC01) (02-projection-and-expressions.md)

## Why
**The single biggest defect in the corpus: 479 of 1908 failures (25%).** A list
quantifier in a standalone projection returns zero rows when its predicate references
the quantifier's own bound variable.

```
RETURN all(x IN [1,2] WHERE x > 0) AS a           -> 0 rows          WRONG
RETURN all(x IN [1,2] WHERE true)  AS a           -> [true]          OK
RETURN none(x IN [] WHERE true)    AS a           -> [true]          OK
UNWIND [1] AS z RETURN all(x IN [1,2] WHERE x > 0) AS a -> [true]    OK
RETURN [x IN [1,2] WHERE x > 0] AS a              -> [[1,2]]         OK
```

The loss needs both conditions: no reading clause upstream, and a predicate that
references the bound variable. `eval/projection/fn_list.rs:311-470` evaluates the four
quantifiers correctly, and `operators/project.rs:393,505` propagates evaluation errors
with `?` rather than dropping rows — so the row is lost *upstream* of the projection.

**Leading hypothesis (must be confirmed, not assumed):** the quantifier's inner `WHERE`
is lifted to a query-level predicate on the no-pattern planning path, where `x` is
unbound, evaluates to null, and filters out the synthetic single row. Candidate sites:
`executor/planner/preparse.rs` and
`executor/planner/queries/planner_core/bound.rs`. If the mechanism turns out to be a
scan planned for the free variable instead, the fix site changes — hence item 1.1.

## What Changes
- Confirm the mechanism by which the row is dropped (do not edit before this is
  proven at a `file:line`).
- Make the quantifier's bound variable local to the quantifier: its predicate must
  never contribute a query-level filter or a scan.
- Same treatment for `all`, `any`, `none`, `single`, and for the `filter(x IN l WHERE p)`
  legacy form, since they share the parser special case
  (`parser/expressions/identifier.rs:62`).

## TCK Impact
479 fails. `expressions/quantifier` 8.1% -> expected high 80s. Roughly +12pp on the total.

## Impact
- Affected code: crates/nexus-core/src/executor/planner/preparse.rs, crates/nexus-core/src/executor/planner/queries/planner_core/bound.rs
- Breaking change: NO
- Dependencies: None — independent, do early.
- User benefit: list predicates work in a bare RETURN, the most common way they are written
