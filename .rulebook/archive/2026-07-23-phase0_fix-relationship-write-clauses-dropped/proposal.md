# Proposal: phase0_fix-relationship-write-clauses-dropped

**Priority: HIGH — two independent write clauses targeting relationship patterns are silently
ignored: MERGE's ON CREATE/ON MATCH SET loses node/label/map-merge items, and a bare relationship
DELETE is a total no-op.** Found during a write-path/index corruption audit; not previously
reported.

## Why

**H-2 — relationship-pattern MERGE drops node-targeted / label / map-merge SET items.**
`apply_merge_rel_set` (`engine/write_exec.rs:986-999`), used by both the ON CREATE branch (`:933`)
and the ON MATCH branch (`:956`), filters every SET item down to only those whose target is the
relationship variable:

```rust
if target != rel_var { continue }
```

Only `SetItem::Property` entries addressed to `rel_var` survive; any `SetItem::Property` addressed
to a node variable, any `SetItem::Label`, and any `SetItem::MapMerge` are silently discarded — no
error, no partial-apply warning.

```
MERGE (a)-[r:KNOWS]->(b) ON CREATE SET a.createdAt=1, r.since=1
-- a.createdAt is null afterward; r.since is correctly set
```

**M-4 — `MATCH (a)-[r]->(b) DELETE r` is a silent no-op.**
`match_exec.rs:54` builds the DELETE target list only from `PatternElement::Node`, so a
relationship variable bound by the MATCH is never added to `match_results`'s delete targets.
Relationship-only DELETE therefore never runs: `deleted_count` is unaffected, the edge remains, and
no error is raised — the query reports success while doing nothing.

Both defects share the same shape: a write clause silently drops the relationship-adjacent part of
its target set instead of applying it or erroring, in the executor's write-clause dispatch layer.

## What Changes

- Rewrite `apply_merge_rel_set` to dispatch each `SetItem` to the correct entity state (node
  property vs. relationship property) instead of filtering everything to `target == rel_var`;
  apply `SetItem::Label` to the node's label set and `SetItem::MapMerge` to the correct entity's
  property map, mirroring how a plain `SET` clause already dispatches. Both the ON CREATE and ON
  MATCH callers share the function, so both benefit from one fix.
- Extend the delete-target collection in `match_exec.rs` to also collect bound relationship
  variables from the pattern, not only `PatternElement::Node`, so `DELETE r` actually deletes the
  relationship record.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (MERGE ON CREATE/ON MATCH SET semantics, DELETE
  semantics)
- Affected code: `engine/write_exec.rs` (`apply_merge_rel_set` and its two call sites at `:933`,
  `:956`), `engine/match_exec.rs` (delete-target collection at `:54`)
- Breaking change: NO — both fixes make previously-silent-no-op clauses actually take effect; no
  currently-correct query changes behavior
- User benefit: `MERGE (a)-[r:KNOWS]->(b) ON CREATE SET a.prop=...` actually sets the node
  property/label/map-merge; `MATCH (a)-[r]->(b) DELETE r` actually deletes the relationship
  instead of silently doing nothing
- Related: `phase0_fix-merge-relationship-dropped` — same MERGE relationship code path, a more
  severe sibling defect where the edge and second endpoint are dropped entirely for unnamed
  patterns
