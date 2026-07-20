# Tasks: phase0_fix-merge-relationship-dropped

`process_merge_relationship` (`crates/nexus-core/src/engine/write_exec.rs:851-962`)
bails to `Ok(None)` whenever the relationship pattern's variable is absent
(`:887-890`) or either endpoint's variable is absent (`:877-884`). The caller
(`:219-233`) treats `Ok(None)` as "not a relationship MERGE" and falls back to
`process_merge_clause` (`:558-573`), which merges only the *first* `Node` found via
`find_map` — with no awareness that a relationship and second node exist in the
pattern. Trigger sequence:
```
MERGE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})
MATCH (b:Person{name:'Bob'}) RETURN b                   -- 0 rows (Bob never created)
MATCH (a:Person{name:'Alice'})-[r:KNOWS]->() RETURN r    -- 0 rows (edge never created)
```
`-[:KNOWS]->` (no relationship variable) is the ordinary, most common way to write
a relationship MERGE, so this is not an edge case.

Order matters: prove the drop with a failing test (§1) before touching
`process_merge_relationship`, because the fix must be verified against the exact
anonymous-variable combinations that trigger the early-return — fixing the
variable case without a green test for the relationship-and-both-endpoints-anonymous
case would leave the most common form still broken.

## 1. Reproduce the loss first
- [ ] 1.1 Write a failing integration test: `MERGE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})`
  against an empty graph, then `MATCH (b:Person{name:'Bob'}) RETURN b` and
  `MATCH (a:Person{name:'Alice'})-[r:KNOWS]->() RETURN r`. Confirm both return 0
  rows today (Bob and the edge are silently never created) and the MERGE itself
  reports success with no error
- [ ] 1.2 Add the same case with an anonymous destination only:
  `MERGE (a:Person{name:'Alice'})-[r:KNOWS]->(:Person{name:'Bob'})` (rel var
  present, dst var absent) — confirm it also hits the `Ok(None)` bailout at
  `write_exec.rs:881-884` and drops `Bob`/the edge today
- [ ] 1.3 Add the anonymous-source case:
  `MERGE (:Person{name:'Alice'})-[r:KNOWS]->(b:Person{name:'Bob'})` — confirm it
  hits `write_exec.rs:877-880` and today merges only whichever node
  `process_merge_clause`'s `find_map` happens to select first, dropping the other
  endpoint and the edge
- [ ] 1.4 Confirm all three tests fail against current `main` with the documented
  symptom (0 rows / silent drop, not a panic or error) before proceeding

## 2. Confirm the mechanism
- [ ] 2.1 Trace and record in the task notes exactly which of the three `Ok(None)`
  returns (`write_exec.rs:877-880` src var, `:881-884` dst var, `:887-890` rel var)
  each §1 test hits, confirming the `elements.len() != 3` check at `:858` is NOT
  the trigger (the pattern shape is always the correct 3-element
  Node-Relationship-Node — only the variable-presence checks are the problem)
- [ ] 2.2 Confirm `process_merge_clause` (`:558-573`) has no relationship or
  second-node handling at all — it is a pure node-pattern merge reused as a
  fallback that silently discards the rest of the pattern when invoked on a
  relationship MERGE, so the fix belongs in `process_merge_relationship`
  (making it always succeed for a genuine 3-element pattern), not in the fallback

## 3. Implement the fix
- [ ] 3.1 In `process_merge_relationship`, synthesize an internal variable name for
  the source node when `src_node.variable` is `None` (replacing the `Ok(None)`
  bailout at `write_exec.rs:877-880`), and use it to resolve/create the node via
  the existing `context`-lookup-or-`merge_single_node` logic at `:908-916`
- [ ] 3.2 Do the same for the destination node (`:881-884`, `:917-925`) and for the
  relationship variable (`:887-890`) — a synthesized rel-var name is sufficient
  since `apply_merge_rel_set` (`:970-999`) only needs `rel_var` to match
  `SetItem::Property` targets, and an anonymous relationship has no
  user-referenceable ON CREATE/ON MATCH SET target
  by construction
- [ ] 3.3 Ensure synthesized variables are never inserted into the query's
  user-visible `context`/`rel_context` under a name that could collide with or
  leak into `RETURN`/`WITH` projections — scope them internally (e.g. a
  recognizable prefix, or keep them local to this function's stack) rather than
  polluting the shared binding maps
- [ ] 3.4 Verify the relationship-type requirement (`rel_pattern.types.first()`,
  `:891-894`) is untouched — `MERGE ()-[r]->()` with no type at all remains
  correctly rejected/handled by its existing path, this task only removes the
  variable-presence bailouts

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md` MERGE section to state that
  relationship patterns with anonymous endpoints and/or an anonymous
  relationship are fully supported (create-or-match the whole pattern); add a
  CHANGELOG entry
- [ ] 4.2 Make the §1 tests pass; add coverage for the fully-anonymous form
  `MERGE (:Person{name:'Alice'})-[:KNOWS]->(:Person{name:'Bob'})` and for
  ON CREATE/ON MATCH SET still applying correctly when the relationship
  variable is synthesized internally but the pattern used `ON CREATE SET`
  without referencing the (absent) rel alias
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-create-path-index-and-constraints`, `phase0_fix-relationship-write-clauses-dropped`
  — other write-path defects in the same MERGE/CREATE execution code
