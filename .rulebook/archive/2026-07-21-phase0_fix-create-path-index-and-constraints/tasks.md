# Tasks: phase0_fix-create-path-index-and-constraints

Two related CREATE-path defects, both "a CREATE variant skips index/constraint
maintenance its sibling variant performs":

- **C-5**: the `MATCH…CREATE` branch (`engine/query_pipeline.rs:704-725`) syncs
  storage back but never calls `index_typed_properties_for_new_nodes`, unlike the
  standalone-CREATE branch (`:776-799`, call at `:799`). A node created this way is
  durable and label-indexed but absent from `Engine::indexes.property_index`, so a
  later `MERGE`'s existence check (`crud/lookup.rs:191-224`, which resolves
  indexed filters via `property_index.find_exact` before falling back to a scan)
  cannot find it and creates a duplicate. Trigger (with `CREATE INDEX ON :Person(id)`):
  `MATCH (s:Seed) CREATE (n:Person {id:42}); MERGE (m:Person {id:42}); MATCH
  (p:Person {id:42}) RETURN count(p);` → returns 2.
- **M-2**: the executor's bare-CREATE operator (`executor/operators/create.rs`)
  only runs its own local `check_constraints` (`:641`, called at `:253`) — a
  different function from the engine's `check_constraints`
  (`engine/constraints.rs:567`) and `enforce_extended_node_constraints`
  (`engine/constraints.rs:304`), and never calls `index_composite_tuples`
  (`engine/crud/index_maintenance.rs:90`). A bare `CREATE` therefore silently
  bypasses `NODE KEY`/composite constraint enforcement and never populates the
  composite B-tree.

Order matters: reproduce both defects (§1) and confirm each mechanism (§2) before
touching any code, because the fix for C-5 (typed-property-index maintenance on
MATCH…CREATE) and the fix for M-2 (constraint/composite maintenance on bare
CREATE) land in different files and must not be conflated — verifying the
mechanism first prevents a partial fix that closes one CREATE entry point while
leaving the other's index/constraint gap open.

## 1. Reproduce both losses first
- [x] 1.1 C-5: write a failing integration test —
  `CREATE INDEX ON :Person(id)`; `CREATE (:Seed)`; `MATCH (s:Seed) CREATE (n:Person
  {id:42})`; `MERGE (m:Person {id:42})`; `MATCH (p:Person {id:42}) RETURN
  count(p)`. Confirm it returns 2 today (the MERGE could not find the
  MATCH…CREATE-created node and duplicated it)
- [x] 1.2 M-2: write a failing integration test —
  `CREATE CONSTRAINT FOR (p:Person) REQUIRE (p.tenantId,p.id) IS NODE KEY`; two
  bare `CREATE (:Person {tenantId:'t1', id:1})` statements; `MATCH (p:Person
  {tenantId:'t1', id:1}) RETURN count(p)`. Confirm it returns 2 today (the
  second CREATE should have been rejected by NODE KEY but silently succeeded)
- [x] 1.3 Run each test in isolation and confirm both reproduce against current
  `main` with the documented symptom (silent duplication, no error raised) — not
  flaky and not masked by an unrelated planner defect

## 2. Confirm the mechanism
- [x] 2.1 Trace C-5: confirm `query_pipeline.rs:704-725` (MATCH…CREATE branch)
  never calls `index_typed_properties_for_new_nodes`, contrasted with the call at
  `:799` in the standalone-CREATE branch (`:776-799`); confirm
  `find_nodes_by_node_pattern` (`crud/lookup.rs:191-224`) resolves indexed
  filters via `self.indexes.property_index.find_exact` (`:193-207`) BEFORE any
  fallback scan, so a node missing from that index is invisible to the MERGE
  existence check regardless of being present in storage
- [x] 2.2 Trace M-2: confirm `executor/operators/create.rs`'s `check_constraints`
  (defined `:641`, called `:253` inside `execute_create_pattern_internal`) is a
  distinct function from `engine::constraints::check_constraints` (`:567`) and
  `enforce_extended_node_constraints` (`:304`); confirm via search that
  `index_composite_tuples` (`engine/crud/index_maintenance.rs:90`) has zero call
  sites reachable from `executor/operators/create.rs`
- [x] 2.3 Record in the task notes that C-5 and M-2 are independent failure modes
  on different CREATE entry points (MATCH-prefixed vs. bare) so a fix to one does
  not close the other — both branches must call into the engine's index/constraint
  maintenance for CREATE, in any form, to preserve the invariant

## 3. Implement the fix
### C-5 — MATCH…CREATE typed-index maintenance
- [x] 3.1 In `query_pipeline.rs`'s MATCH…CREATE branch (`:704-725`), take a
  `pre_create_node_count` watermark before `execute_match_create_query` runs and
  call `self.index_typed_properties_for_new_nodes(pre_create_node_count)` after
  the storage sync at `:708`, mirroring the standalone branch's
  `:776,:799`
- [x] 3.2 Sync the engine's label index (`Engine::indexes`, not only the
  executor's cloned copy) for nodes created via MATCH…CREATE, per the gap the
  standalone branch's own comment documents at `:764-770` ("the executor CREATE
  path writes storage + the label index but NOT the typed property B-tree") —
  confirm the label-index side is not separately diverging for this branch

### M-2 — bare CREATE constraint + composite-index maintenance
- [x] 3.3 Route `executor/operators/create.rs`'s node-creation path
  (`execute_create_pattern_internal`, `:97-256`) through the engine's
  `enforce_extended_node_constraints` (`engine/constraints.rs:304`) and
  `index_composite_tuples` (`engine/crud/index_maintenance.rs:90`) — either by
  making the executor CREATE operator call back into the engine for
  newly-created node ids, or by exposing an equivalent maintenance entry point
  callable from the executor, so `NODE KEY`/composite constraints are enforced
  and the composite B-tree is populated for every CREATE, not only
  MATCH…CREATE/MERGE paths that already reach engine-level helpers
- [x] 3.4 Keep the existing local `check_constraints` (`create.rs:641`, call
  `:253`) as the fast local rejection it already is; verify it does not now
  double-reject (conflicting error) or double-insert into the composite tree
  once the engine-level enforcement from §3.3 also runs on the same CREATE

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation:
  `docs/specs/cypher-subset.md` now states that both MATCH-prefixed and bare
  CREATE fully maintain typed property indexes, composite indexes, and extended
  (`NODE KEY`) constraints; CHANGELOG entry added.
- [x] 4.2 Write tests covering the new behavior: made the §1 tests pass and added
  the inverse pairing (bare CREATE followed by MERGE for the C-5 shape;
  MATCH…CREATE violating a NODE KEY constraint for the M-2 shape) plus full
  statement rollback — 6 cases in
  `tests/executor/create_path_index_and_constraints_test.rs`, both CREATE entry
  points verified against both invariants.
- [x] 4.3 Run tests and confirm they pass: `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
  `cargo +nightly test --workspace` all green (51 test-result groups ok; the sole
  hiccup was a transient nexus-cli doctest linker race that passes in isolation).

## Related
- `phase0_fix-merge-relationship-dropped` — adjacent MERGE/CREATE write-path
  defect in the same subsystem
- `phase0_fix-delete-path-index-cleanup` — composite B-tree is also never
  evicted on delete; the write-side (this task) and delete-side gaps in
  composite-index maintenance should be read together
