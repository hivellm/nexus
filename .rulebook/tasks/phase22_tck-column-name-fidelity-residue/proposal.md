# Proposal: phase22_tck-column-name-fidelity-residue

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase D — F-114 (RC18) (02-projection-and-expressions.md)

## Why
What remains of column-name divergence once the projection-list truncation is fixed:

```
MATCH (n) RETURN (n:Foo)  -> column "n:Foo"            (want "(n:Foo)")
RETURN (a AND b) IS NULL = (b AND a) IS NULL AS result
                          -> column "a AND b IS NULL"  (want "result")
```

The second is the truncation defect leaking (the alias was lost), which is why this
task must not be sized or started before
phase22_tck-projection-list-silent-truncation lands. **The measured 47 will change.**

## What Changes
- An un-aliased projection item is named by the **verbatim source text** of its
  expression, parentheses included.
- An aliased item is named by its alias, always.

## TCK Impact
47 fails as measured today (boolean 11, with-orderBy 10, precedence 8, return 6, aggregation 6) — expected lower.

## Impact
- Affected code: crates/nexus-core/src/executor/operators/project.rs, the expression_to_string / column-naming path
- Breaking change: NO
- Dependencies: **Blocked by** phase22_tck-projection-list-silent-truncation.
- User benefit: result column headers match what the user wrote
