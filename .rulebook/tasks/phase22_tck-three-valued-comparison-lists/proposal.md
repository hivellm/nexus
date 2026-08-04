# Proposal: phase22_tck-three-valued-comparison-lists

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase B — F-112 (RC15) (02-projection-and-expressions.md)

## Why
Top-level three-valued logic landed (`expressions/null` 95.5%), but list and
nested-list comparison still collapse an incomparable element to `false` instead of
propagating null:

```
RETURN [null] = [1] AS result              -> false   (want null)
RETURN [[1],[2]] = [[1],[null]] AS result  -> false   (want null)
```

Same shape for `IN` over a list containing nulls, and for `<>`.

## What Changes
- Element-wise list equality and inequality propagate null when any pairwise
  comparison is null, recursively for nested lists.
- `IN` over a list containing null follows the same Kleene rules.

## TCK Impact
24 fails (comparison 13, list 8, graph 1, null 1, precedence 1).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/predicate.rs, crates/nexus-core/src/executor/eval/projection/core.rs
- Breaking change: NO
- Dependencies: None — independent.
- User benefit: null propagates through list comparison instead of silently becoming false
