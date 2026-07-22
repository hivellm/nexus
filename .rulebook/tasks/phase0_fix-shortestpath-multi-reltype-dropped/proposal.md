# Proposal: phase0_fix-shortestpath-multi-reltype-dropped

**Priority: HIGH — `shortestPath()` / `allShortestPaths()` over a multi-type
relationship pattern (`shortestPath((a)-[:R1|R2*..5]-(b))`) traverse ONLY the
first named type; any path that requires a non-first type is missed, returning
a wrong (or empty) shortest path with no error.** Same first-type-only bug
class as the just-fixed `phase0_fix-varlength-multi-reltype-dropped`, but in the
sibling shortest-path functions, which were confirmed as a separate, untouched
code path during that fix's code review.

## Why

`phase0_fix-varlength-multi-reltype-dropped` fixed the general
`Operator::VariableLengthPath` traversal (`execute_variable_length_path`), but
the shortest-path helpers in `crates/nexus-core/src/executor/operators/path.rs`
— `find_shortest_path`, `find_all_shortest_paths`, and `find_paths_dfs` — still
take `type_id: Option<u32>` and still narrow it to an at-most-one-element slice
before calling `find_relationships`:

```rust
let type_ids_slice: Vec<u32> = type_id.into_iter().collect();
```

so their BFS/DFS can only ever match a single relationship type. Their only
callers are the `shortestPath()` / `allShortestPaths()` Cypher functions in
`crates/nexus-core/src/executor/eval/projection/fn_graph.rs` (around lines 402
and 476). `execute_variable_length_path` (fixed) never calls these three
functions, which is why the earlier fix did not cover them.

## What Changes

- Change `find_shortest_path`, `find_all_shortest_paths`, and `find_paths_dfs`
  to take `type_ids: &[u32]` (or `Vec<u32>`) instead of `type_id: Option<u32>`,
  and pass the full slice straight into `find_relationships`, removing the
  `type_id.into_iter().collect()` single-element reconstruction — mirroring the
  `execute_variable_length_path` fix.
- Update the `fn_graph.rs` `shortestPath()` / `allShortestPaths()` call sites to
  extract the full parsed `type_ids` list from the pattern and pass it through,
  instead of collapsing to a single type.
- Preserve "empty = match all types" semantics (an unqualified
  `shortestPath((a)-[*]-(b))` must still traverse every type).

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (shortestPath / allShortestPaths
  semantics, if documented)
- Affected code: `crates/nexus-core/src/executor/operators/path.rs`
  (`find_shortest_path`, `find_all_shortest_paths`, `find_paths_dfs`),
  `crates/nexus-core/src/executor/eval/projection/fn_graph.rs`
  (`shortestPath` / `allShortestPaths` extraction + call sites)
- Breaking change: NO — correctness fix; multi-type shortest-path queries return
  the correct path instead of missing paths that use non-first types
- User benefit: `shortestPath((a)-[:R1|R2*..n]-(b))` and multi-type
  `allShortestPaths` return correct results
- Related: `phase0_fix-varlength-multi-reltype-dropped` (same bug class in the
  general variable-length operator, already fixed in c76e41c5)
