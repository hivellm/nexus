# Tasks: phase0_fix-varlength-multi-reltype-dropped

The default (non-QPP) lowering for variable-length relationship patterns kept
only the first relationship type (`type_ids.first().copied()`), and
`Operator::VariableLengthPath.type_id` was a single `Option<u32>`, so the
operator could not structurally represent more than one type. Trigger:
`MATCH (a:Person {name:'Alice'})-[:KNOWS|FOLLOWS*1..3]->(b:Person) RETURN
b.name` only traversed `KNOWS`; any `b` reachable solely via `FOLLOWS` was
silently dropped. Fixed by retyping the field to `type_ids: Vec<u32>` and
threading the full list through the BFS. Landed in commit c76e41c5.

## 1. Reproduce the loss first
- [x] 1.1 Failing test written and confirmed failing pre-fix: `Alice -[:FOLLOWS]-> Carol`
  (no KNOWS edge), `[:KNOWS|FOLLOWS*1..3]` returned `[]` (Carol absent) before
  the fix (`variable_length_multi_type_reaches_node_via_second_type_only`)
- [x] 1.2 Mixed-type path `Alice-[:KNOWS]->Bob-[:FOLLOWS]->Dave`: pre-fix only
  `Bob` returned, `Dave` dropped; now both returned
  (`variable_length_multi_type_returns_nodes_reached_via_either_type`)
- [x] 1.3 Unqualified `[*1..3]` control test: still returns every reachable
  node (guards "empty type_ids = match all"); passed pre- and post-fix
  (`variable_length_unqualified_pattern_still_matches_every_type`)

## 2. Confirm the mechanism
- [x] 2.1 Confirmed the parser produces the full `type_ids: Vec<u32>` for
  `[:R1|R2*m..n]` — the same list `Expand` and `QppHopSpec` already consume in
  the sibling lowering branches
- [x] 2.2 Confirmed the default (non-QPP) branch discarded all but the first
  type via `type_ids.first().copied()`, while the single-hop `Expand` lowering
  just below kept the full list
- [x] 2.3 Enumerated the narrowing sites: `VariableLengthPathVisitor` type
  filter, `execute_variable_length_path`, the BFS-fallback
  `type_id.into_iter().collect()`, and both `VariableLengthPath` dispatch arms
  (executor/dispatch.rs + operators/dispatch.rs). (The optimized-traversal
  branch is dead code — `use_optimized_traversal = false` — but was updated for
  regression hygiene.)
- [x] 2.4 Confirmed `find_relationships` matches a multi-element slice via
  `type_ids.is_empty() || type_ids.contains(&record_type_id)` — the BFS needed
  the full list threaded in, not new matching logic

## 3. Fix the type handling
- [x] 3.1 Changed `Operator::VariableLengthPath.type_id: Option<u32>` to
  `type_ids: Vec<u32>` (types.rs) and updated the lowering to pass
  `type_ids.clone()` (relationships.rs), preserving "empty vec = match all"
- [x] 3.2 Updated every consumer in path.rs to pass the full `type_ids` slice
  directly into `find_relationships`/the visitor filter, removing the
  single-element reconstruction
- [x] 3.3 Fixed both dispatch match arms; `cargo +nightly check --workspace`
  compiles clean (cost.rs/strategy.rs/planner tests all use `{ .. }` wildcards,
  unaffected)
- [x] 3.4 The §1 tests pass; both `Carol` and `Dave` now returned, unqualified
  control unaffected. Added a 3-way `[:A|B|C*1..2]` union test for off-by-N
  coverage

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — CHANGELOG
  entry added; `docs/specs/cypher-subset.md` Type production clarified to
  `Type ::= Identifier ( '|' Identifier )*` (single type or union)
- [x] 4.2 Write tests covering the new behavior —
  `variable_length_multi_reltype_test.rs` (4 tests) plus a QPP-rewrite parity
  test in `engine/tests/query.rs` asserting the legacy `VariableLengthPath` path
  and the `QuantifiedExpand` rewrite return identical rows (via the sanctioned
  `set_qpp_legacy_rewrite_enabled` toggle under `#[serial_test::serial]`)
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all` clean;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean;
  `cargo +nightly test --workspace` green (5083 passed / 0 failed)

## Related
- `phase0_fix-plan-reorder-drops-predicates` — another `VariableLengthPath`-
  adjacent planner correctness defect (operator ordering), landed in 3b13f530
- `phase0_fix-shortestpath-multi-reltype-dropped` — same first-type-only bug in
  the sibling `shortestPath()`/`allShortestPaths()` path functions, filed as a
  follow-up
