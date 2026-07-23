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

> **STALE TASK (verified 2026-07-23):** both defects were already fixed by
> sibling tasks before this task ran — reproduce-first tests PASS on current
> code, so there is nothing to fix. H-2: `apply_merge_rel_set` was replaced by
> `apply_merge_relationship_set`, which delegates to the general
> `apply_set_clause` (commit a047eade), so node-property / label / map-merge SET
> items on a relationship MERGE now dispatch correctly. M-4: `DELETE r` was
> fixed under `phase0_fix-cypher-relationship-delete-noop` (its regression suite
> `tests/executor/relationship_delete_test.rs` already covers relationship-only
> DELETE and the mixed `DELETE r, a` clause). This task's residual value is the
> H-2 regression coverage that was still missing, added below.

## 1. Reproduce H-2 first
- [x] 1.1 Wrote the H-2 test (`ON CREATE SET a.createdAt=1, r.since=2`) — PASSES
  on current code (a.createdAt == 1, r.since == 2). Not a failing test: H-2 is
  already fixed
- [x] 1.2 Added `SetItem::Label` (`ON CREATE SET a:Extra`) and `SetItem::MapMerge`
  (`ON CREATE SET a += {x:9}`) cases targeting the node — both PASS (applied, not
  dropped)
- [x] 1.3 Confirmed by inspection: `apply_merge_relationship_set`
  (`write_exec.rs:1106`) is shared by both the ON CREATE and ON MATCH call sites
  and delegates to `apply_set_clause`, which dispatches every item by target

## 2. Fix H-2 — already fixed (no change needed)
- [x] 2.1 No rewrite needed — the delegation to `apply_set_clause` already routes
  each `SetItem` by its actual target (node vs relationship), covering
  `SetItem::Label` and `SetItem::MapMerge`
- [x] 2.2 §1.1/§1.2 tests pass for ON CREATE; added an ON MATCH variant
  (`ON MATCH SET a.updatedAt=7`) which also passes

## 3. Reproduce M-4
- [x] 3.1 `MATCH (a)-[r]->(b) DELETE r` removes the edge (`count(r)` → 0) on
  current code — already fixed; covered by
  `tests/executor/relationship_delete_test.rs::delete_relationship_soft_deletes_the_edge`
- [x] 3.2 Confirmed by inspection: `match_exec.rs` now collects `rel_variables`
  (not only `PatternElement::Node`), projects them, and deletes them in Pass 1
  (the task's `:54` line reference is stale)

## 4. Fix M-4 — already fixed (no change needed)
- [x] 4.1 Delete-target collection already includes bound relationship variables
- [x] 4.2 §3.1 passes; the mixed `DELETE r, a` case is already covered by
  `relationship_delete_test.rs::delete_node_and_relationship_in_one_clause`

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 Update or create documentation covering the implementation —
  `docs/specs/cypher-subset.md` already documents `DELETE r` and MERGE
  `ON CREATE/ON MATCH SET`; CHANGELOG entry added recording the verification and
  cross-referencing the sibling fixes
- [x] 5.2 Write tests covering the new behavior — added
  `tests/executor/relationship_merge_set_dispatch_test.rs` (H-2: node property,
  label, map-merge on ON CREATE + ON MATCH); M-4 coverage already present in
  `relationship_delete_test.rs`
- [x] 5.3 Run tests and confirm they pass — H-2 tests 4/4; `cargo +nightly fmt
  --all` + `cargo clippy -p nexus-core --tests -- -D warnings` green; full
  `cargo +nightly test --workspace` run to confirm

## Related
- `phase0_fix-merge-relationship-dropped` — same MERGE relationship code path, a more severe
  sibling defect where the edge and second endpoint are dropped entirely for unnamed patterns
