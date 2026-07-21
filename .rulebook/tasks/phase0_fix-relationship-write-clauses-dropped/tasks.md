# Tasks: phase0_fix-relationship-write-clauses-dropped

> **Status update 2026-07-21 (commit a047eade):** H-2's `apply_merge_rel_set`
> no longer exists — the variable-less-relationship-MERGE fix replaced it with
> `apply_merge_relationship_set`, which delegates to the general
> `apply_set_clause`, so node-targeted SET items on relationship MERGE now
> apply instead of being filtered to `target == rel_var`. Re-verify H-2's
> remaining scope before implementing (SetItem::Label / MapMerge coverage may
> already be handled by the delegation — confirm with tests, don't assume).
> M-4 (`DELETE r` collection only looking at `PatternElement::Node`) is
> untouched and remains the active defect.

Two independent write-clause bugs share one root cause: the executor's write-clause dispatch for
relationship patterns silently drops anything that isn't the exact case it was written for. H-2's
`apply_merge_rel_set` (`engine/write_exec.rs:986-999`) filters every SET item to
`target == rel_var`, discarding node-targeted SET, `SetItem::Label`, and `SetItem::MapMerge`. M-4's
delete-target collection (`engine/match_exec.rs:54`) only looks at `PatternElement::Node`, so a
relationship-only `DELETE r` never runs. Neither raises an error — both report success while doing
nothing (or doing less than requested).

Order matters: fix H-2 first because MERGE ON CREATE/ON MATCH SET is the far more common query
shape and its partial-apply is subtler to detect (some SET items DO take effect, masking the bug);
M-4's DELETE no-op is total and simpler to isolate once the H-2 reproduction harness exists.

## 1. Reproduce H-2 first
- [ ] 1.1 Write a failing integration test: `MERGE (a)-[r:KNOWS]->(b) ON CREATE SET a.createdAt=1,
  r.since=1`, then assert `a.createdAt == 1`. Confirm it fails today (property is null) while
  `r.since` is correctly set — the partial-apply is the specific defect, not total failure
- [ ] 1.2 Add cases for `SetItem::Label` (`ON CREATE SET a:Extra`) and `SetItem::MapMerge`
  (`ON CREATE SET a += {x:1}`) targeting the node, confirming both are silently dropped
- [ ] 1.3 Confirm via code inspection that `apply_merge_rel_set`
  (`engine/write_exec.rs:986-999`) is shared by both the ON CREATE branch (`:933`) and ON MATCH
  branch (`:956`), so the fix must be verified against both

## 2. Fix H-2: dispatch SET items to the correct entity
- [ ] 2.1 Rewrite `apply_merge_rel_set` to route each `SetItem` by its actual target (node vs.
  relationship) instead of filtering to `target == rel_var`; apply `SetItem::Label` to the node's
  label set and `SetItem::MapMerge` to the correct entity's property map
- [ ] 2.2 Make the §1.1 and §1.2 tests pass for the ON CREATE branch, then add matching ON MATCH
  variants (`ON MATCH SET a.updatedAt=1`) and confirm they now apply too

## 3. Reproduce M-4
- [ ] 3.1 Write a failing integration test: create a relationship, run
  `MATCH (a)-[r]->(b) DELETE r`, then assert the relationship no longer exists (e.g. via
  `MATCH (a)-[r]->(b) RETURN count(r)` returning 0). Confirm it fails today — the relationship
  still exists and `deleted_count` is 0
- [ ] 3.2 Confirm via code inspection that `match_exec.rs:54` builds delete targets only from
  `PatternElement::Node`, so the relationship variable `r` is structurally never added to
  `match_results`'s delete set

## 4. Fix M-4: include relationship variables in delete targets
- [ ] 4.1 Extend the delete-target collection in `match_exec.rs` to also collect bound
  relationship variables from the pattern, not only nodes
- [ ] 4.2 Make the §3.1 test pass; add a case mixing node and relationship DELETE in the same
  clause (`DELETE r, a`) to confirm both target types are collected together without regressing
  the existing node-only path

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 5.1 Update `docs/specs/cypher-subset.md` with the corrected MERGE ON CREATE/ON MATCH SET
  dispatch semantics and DELETE-on-relationship-variable semantics; add a CHANGELOG entry
- [ ] 5.2 Tests: MERGE ON CREATE/ON MATCH SET applies node property, label, and map-merge items
  (§1/§2 regressions); relationship-only DELETE actually deletes the edge (§3/§4 regression);
  mixed node+relationship DELETE in one clause
- [ ] 5.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-merge-relationship-dropped` — same MERGE relationship code path, a more severe
  sibling defect where the edge and second endpoint are dropped entirely for unnamed patterns
