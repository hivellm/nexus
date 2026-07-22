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
- [ ] 1.1 Write a failing test: a graph where the shortest path between `a` and
  `b` requires a non-first relationship type (e.g. `a-[:R2]->b` with the query
  `shortestPath((a)-[:R1|R2*..5]-(b))`); assert the path is found. Confirm it
  returns no path (or a wrong longer path) today
- [ ] 1.2 Add an `allShortestPaths` multi-type test and an unqualified
  `shortestPath((a)-[*..5]-(b))` control (must still match every type)

## 2. Confirm the mechanism
- [ ] 2.1 Confirm `find_shortest_path`/`find_all_shortest_paths`/`find_paths_dfs`
  take `type_id: Option<u32>` and do `type_id.into_iter().collect()` before
  `find_relationships`, capping matching at one type
- [ ] 2.2 Confirm the only callers are `shortestPath`/`allShortestPaths` in
  `fn_graph.rs`, and that those extract only a single type from the pattern

## 3. Fix the type handling
- [ ] 3.1 Change the three path functions to take `type_ids: &[u32]` (or
  `Vec<u32>`) and pass the full slice into `find_relationships`, removing the
  single-element reconstruction; preserve "empty = match all"
- [ ] 3.2 Update the `fn_graph.rs` call sites to extract and pass the full
  parsed `type_ids` list
- [ ] 3.3 Make the §1 tests pass

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation
  (CHANGELOG; `docs/specs/cypher-subset.md` shortestPath section if present)
- [ ] 4.2 Write tests covering the new behavior (the §1 regression tests plus a
  3+ type union case)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)
