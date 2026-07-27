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

**Independently re-confirmed 2026-07-21** while writing
`tests/executor/write_refresh_visibility_test.rs`: with both endpoints
pre-existing and pre-matched (`MATCH (a:C {n:'c'}), (b:D {n:'d'}) MERGE
(a)-[:MMERGED]->(b)`), the relationship count afterwards is 0 — and stays 0
even after a manual `engine.refresh_executor()`, ruling out executor
staleness. The identical shape with an explicit relationship variable
(`MERGE (a)-[r:GMERGED]->(b)`) works, as does `CREATE (a)-[:CMERGED]->(b)`,
pinning the defect to the variable-less MERGE form exactly as described
above. The visibility test suite deliberately uses the explicit-variable
form (test `merge_relationship_between_existing_nodes_is_immediately_visible`)
until this task lands — revert it to the variable-less form as part of §1.

Order matters: prove the drop with a failing test (§1) before touching
`process_merge_relationship`, because the fix must be verified against the exact
anonymous-variable combinations that trigger the early-return — fixing the
variable case without a green test for the relationship-and-both-endpoints-anonymous
case would leave the most common form still broken.

## 1. Reproduce the loss first
- [x] 1.1 Write a failing integration test: `MERGE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})`
  against an empty graph, then `MATCH (b:Person{name:'Bob'}) RETURN b` and
  `MATCH (a:Person{name:'Alice'})-[r:KNOWS]->() RETURN r`. Confirm both return 0
  rows today (Bob and the edge are silently never created) and the MERGE itself
  reports success with no error — done:
  `crates/nexus-core/tests/cypher/merge_relationship_anonymous_variable_test.rs::variable_less_relationship_merge_creates_both_endpoints_and_edge`,
  confirmed 0 rows / no error against unmodified `main` before the fix landed
- [x] 1.2 Add the same case with an anonymous destination only:
  `MERGE (a:Person{name:'Alice'})-[r:KNOWS]->(:Person{name:'Bob'})` (rel var
  present, dst var absent) — confirm it also hits the `Ok(None)` bailout at
  `write_exec.rs:881-884` and drops `Bob`/the edge today — done:
  `...::relationship_merge_with_anonymous_destination_creates_destination_and_edge`,
  confirmed 0 rows / no error against unmodified `main`
- [x] 1.3 Add the anonymous-source case:
  `MERGE (:Person{name:'Alice'})-[r:KNOWS]->(b:Person{name:'Bob'})` — confirm it
  hits `write_exec.rs:877-880` and today merges only whichever node
  `process_merge_clause`'s `find_map` happens to select first, dropping the other
  endpoint and the edge — done:
  `...::relationship_merge_with_anonymous_source_creates_source_and_edge`.
  **Deviation from the documented expectation**: against unmodified `main` this
  case does NOT silently drop — `find_map` picks the anonymous `Alice` node
  first (it is `elements[0]`), and `process_merge_clause` then hard-errors with
  `CypherExecution("MERGE requires a variable alias")` because that node has no
  variable. So this specific sub-case fails loudly, not silently; the other two
  §1 cases (1.1, 1.2) do match the documented silent-drop symptom exactly. Root
  mechanism (the src-var `Ok(None)` bailout) is unchanged either way.
- [x] 1.4 Confirm all three tests fail against current `main` with the documented
  symptom (0 rows / silent drop, not a panic or error) before proceeding — done:
  ran `cargo +nightly test -p nexus-core --test cypher merge_relationship_anonymous_variable`
  against the unmodified `process_merge_relationship`/`process_merge_clause`
  (write_exec.rs edits reverted for this run, then reapplied): 1.1 and 1.2 fail
  with 0 rows as documented; 1.3 fails with the `CypherExecution` error noted
  above (not a panic — a returned `Err`, so the "not a panic" half of the claim
  holds; the "silent drop" half does not for this one sub-case)

## 2. Confirm the mechanism
- [x] 2.1 Trace and record in the task notes exactly which of the three `Ok(None)`
  returns (`write_exec.rs:877-880` src var, `:881-884` dst var, `:887-890` rel var)
  each §1 test hits, confirming the `elements.len() != 3` check at `:858` is NOT
  the trigger (the pattern shape is always the correct 3-element
  Node-Relationship-Node — only the variable-presence checks are the problem)
  — done (line numbers below are from the pre-fix source, which had moved from
  the stale numbers in this file's header but kept the same three-bailout
  shape): 1.1 (rel var absent) hits the relationship-variable check (`let
  rel_var = match &rel_pattern.variable { Some(v) => ..., None => return
  Ok(None) }`); 1.2 (dst var absent) hits the destination-variable check (`let
  dst_var = match &dst_node.variable { ... None => return Ok(None) }`) before
  ever reaching the rel-var check; 1.3 (src var absent) hits the
  source-variable check (`let src_var = match &src_node.variable { ... None =>
  return Ok(None) }`), the FIRST of the three, before dst or rel are inspected.
  `elements.len() != 3` never triggers for any of the three — all three
  patterns are the correct `Node, Relationship, Node` shape
- [x] 2.2 Confirm `process_merge_clause` (`:558-573`) has no relationship or
  second-node handling at all — it is a pure node-pattern merge reused as a
  fallback that silently discards the rest of the pattern when invoked on a
  relationship MERGE, so the fix belongs in `process_merge_relationship`
  (making it always succeed for a genuine 3-element pattern), not in the fallback
  — done: confirmed by reading `process_merge_clause` — it `find_map`s the
  FIRST `PatternElement::Node` in `merge_clause.pattern.elements` and requires
  that node to carry a `variable` (erroring `"MERGE requires a variable alias"`
  if not); it never looks at `PatternElement::Relationship` or a second `Node`
  at all. This is exactly why 1.1/1.2 silently drop the second node+edge (the
  first node it finds happens to have a variable) while 1.3 hard-errors (the
  first node it finds is the anonymous one). Fix implemented entirely inside
  `process_merge_relationship`; `process_merge_clause` untouched.

## 3. Implement the fix
- [x] 3.1 In `process_merge_relationship`, synthesize an internal variable name for
  the source node when `src_node.variable` is `None` (replacing the `Ok(None)`
  bailout at `write_exec.rs:877-880`), and use it to resolve/create the node via
  the existing `context`-lookup-or-`merge_single_node` logic at `:908-916`
  — done, with one deliberate deviation from the literal proposal: anonymous
  endpoints do NOT get a synthesized variable written into the shared
  `context` map at all (not even under an internal name) — they resolve
  through `merge_single_node` directly and the id is used locally. Named
  endpoints keep the exact prior `context`-lookup-or-`merge_single_node`
  behaviour. See 3.3 for why.
- [x] 3.2 Do the same for the destination node (`:881-884`, `:917-925`) and for the
  relationship variable (`:887-890`) — a synthesized rel-var name is sufficient
  since `apply_merge_rel_set` (`:970-999`) only needs `rel_var` to match
  `SetItem::Property` targets, and an anonymous relationship has no
  user-referenceable ON CREATE/ON MATCH SET target
  by construction — done: destination node mirrors 3.1. The relationship
  variable uses the empty string `""` as its synthesized value (guaranteed
  never producible by the parser — `is_identifier_start` requires a
  letter/underscore — so it can never collide with or be typed as a real
  variable). `apply_merge_rel_set` was replaced by
  `apply_merge_relationship_set`, which delegates to the general
  `apply_set_clause` (see 4.2 rationale) instead of only matching
  `SetItem::Property { target == rel_var }`; the empty-string sentinel is
  simply omitted from the local rel-context passed to `apply_set_clause`, so
  it can never be a SET target.
- [x] 3.3 Ensure synthesized variables are never inserted into the query's
  user-visible `context`/`rel_context` under a name that could collide with or
  leak into `RETURN`/`WITH` projections — scope them internally (e.g. a
  recognizable prefix, or keep them local to this function's stack) rather than
  polluting the shared binding maps — done, via the "never write it at all"
  variant of this requirement rather than a prefix: anonymous node ids are
  never inserted into the shared `context: &mut HashMap<String, Vec<u64>>`
  (only named endpoints are, exactly as before); the anonymous relationship
  variable is the empty-string sentinel, which is explicitly filtered out by
  BOTH call sites before insertion into the caller's `rel_context`. Rationale
  for not even using a locally-scoped prefixed key in the shared maps: found
  during 3.1 research that `build_return_result_with_executor` calls
  `context.keys().next()` to pick "the" bound variable for certain RETURN
  paths — an extra synthesized key in `context` (even an unusual-looking one)
  could be picked over the real user variable non-deterministically (HashMap
  iteration order), so the safest fix is to never write anonymous bindings
  into the shared map in the first place.
- [x] 3.4 Verify the relationship-type requirement (`rel_pattern.types.first()`,
  `:891-894`) is untouched — `MERGE ()-[r]->()` with no type at all remains
  correctly rejected/handled by its existing path, this task only removes the
  variable-presence bailouts — done: the `rel_pattern.types.first() ... None
  => return Ok(None)` check is unchanged (only moved earlier in the function,
  before endpoint resolution, as a cheap-check-first reordering — behaviourally
  identical, still an `Ok(None)` bailout on no type)

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — update `docs/specs/cypher-subset.md` MERGE section to state that
  relationship patterns with anonymous endpoints and/or an anonymous
  relationship are fully supported (create-or-match the whole pattern); add a
  CHANGELOG entry. Updated MERGE section with examples and explanation of
  anonymous patterns; added Fixed entry to CHANGELOG.md under [3.0.0].
- [x] 4.2 Make the §1 tests pass; add coverage for the fully-anonymous form
  `MERGE (:Person{name:'Alice'})-[:KNOWS]->(:Person{name:'Bob'})` and for
  ON CREATE/ON MATCH SET still applying correctly when the relationship
  variable is synthesized internally but the pattern used `ON CREATE SET`
  without referencing the (absent) rel alias — done: all 3 §1 tests pass after
  the fix; added `fully_anonymous_relationship_merge_creates_both_endpoints_and_edge`
  (incl. a re-run to confirm no duplication),
  `on_create_set_targeting_node_applies_when_relationship_variable_is_absent`
  (previously silently dropped by `apply_merge_rel_set`, now applies via the
  `apply_merge_relationship_set` → `apply_set_clause` delegation), and
  `on_create_set_referencing_a_nonexistent_relationship_alias_behaves_sanely`
  (confirmed: rejected with a clear `CypherExecution("Unknown variable 'r' in
  SET clause")` error, not a panic, not silent corruption)
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green — done: fmt clean (0 diff on
  `--check` after running), clippy 0 warnings across the full workspace
  (`--all-targets --all-features -D warnings`), full `cargo +nightly test
  --workspace` (incl. doctests) 5028 passed / 0 failed / 96 ignored across
  every crate in the workspace

## Related
- `phase0_fix-create-path-index-and-constraints`, `phase0_fix-relationship-write-clauses-dropped`
  — other write-path defects in the same MERGE/CREATE execution code
