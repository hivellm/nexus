# Tasks: phase0_fix-shortestpath-multi-reltype-dropped

`find_shortest_path`, `find_all_shortest_paths`, and `find_paths_dfs`
(`crates/nexus-core/src/executor/operators/path.rs`) still take
`type_id: Option<u32>` and narrow it to an at-most-one-element slice
(`let type_ids_slice: Vec<u32> = type_id.into_iter().collect();`) before calling
`find_relationships`, so `shortestPath()`/`allShortestPaths()` over a multi-type
pattern traverse only the first named type. Only callers are the
`shortestPath()`/`allShortestPaths()` functions in
`crates/nexus-core/src/executor/eval/projection/fn_graph.rs`. Same fix shape as
`phase0_fix-varlength-multi-reltype-dropped`.

## 1. Reproduce the loss first
- [x] 1.1 Write a failing test: a graph where the shortest path between `a` and
  `b` requires a non-first relationship type (`a-[:R2]->b` with the decoy `:R1`
  registered, query `RETURN shortestPath((a)-[:R1|R2*1..5]->(b))`); assert the
  path is found. Confirmed it returns no path pre-fix (verified by temporarily
  reintroducing the first-type-only behavior: 3 multi-type tests fail, control
  passes). NOTE: `MATCH p = shortestPath(...)` does NOT parse in Nexus — the
  working form binds endpoints first: `MATCH (a),(b) RETURN shortestPath(...)`.
  ALSO: the decoy first type MUST be registered in the catalog or the bug is
  masked (unresolved type -> empty filter -> match-all).
- [x] 1.2 Add an `allShortestPaths` multi-type test and an unqualified
  `[*1..5]` control (must still match every type)

## 2. Confirm the mechanism
- [x] 2.1 Confirmed `find_shortest_path`/`find_all_shortest_paths`/`find_paths_dfs`
  took `type_id: Option<u32>` and did `type_id.into_iter().collect()` before
  every `find_relationships`, capping matching at one type
- [x] 2.2 Confirmed the only callers are `shortestPath`/`allShortestPaths` in
  `fn_graph.rs` (+ the internal find_paths_dfs call/recursion), and that those
  extracted only `r.types.first()` from the pattern

## 3. Fix the type handling
- [x] 3.1 Changed the three path functions to take `type_ids: &[u32]` and pass
  the full slice into `find_relationships`, removing the single-element
  reconstruction; empty = match all preserved
- [x] 3.2 Updated both `fn_graph.rs` call sites to map the full `types` list to
  type ids (`r.types.iter().filter_map(get_type_id).collect()`) and pass
  `&type_ids`
- [x] 3.3 §1 tests pass (4/4 green)

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation
  (CHANGELOG [3.0.0]; `docs/specs/cypher-subset.md` shortestPath section — multi
  type + MATCH-binding note)
- [x] 4.2 Write tests covering the new behavior
  (`tests/executor/shortestpath_multi_reltype_test.rs` — 4 tests incl. 3-type union)
- [x] 4.3 Run tests and confirm they pass (fmt, clippy, workspace)
