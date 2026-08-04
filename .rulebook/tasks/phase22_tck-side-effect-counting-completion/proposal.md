# Proposal: phase22_tck-side-effect-counting-completion

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase B — F-155 (RC21) (05-write-clauses.md)

## Why
Side-effect counters diverge from the spec in the cases the earlier counting task did
not cover:

```
MERGE (a:TheLabel {num: 43}) RETURN a.num
  got  labels_added: 1
  want labels_added: 0   -- a label applied while creating a node is not a separate label add
```

Split: merge 12, delete 11, create 3, remove 1, set 1, plus one `expected no side
effects, but the query reported ...`. Assertions are exact, so verification is
unambiguous.

## What Changes
- MERGE-on-create must not count a label applied as part of the creation as a separate
  `labels_added`.
- Align DELETE counters (nodes/relationships deleted, and the detached relationships of
  a `DETACH DELETE`) with the spec.
- Ensure a query the spec says has no side effects reports none.

## TCK Impact
28 fails (merge 12, delete 11, create 3, remove 1, set 1).

## Impact
- Affected code: crates/nexus-core/src/executor/operators/ (write operators), the SideEffects accumulator
- Breaking change: NO
- Dependencies: None — independent.
- User benefit: reported statistics match what the query actually did
