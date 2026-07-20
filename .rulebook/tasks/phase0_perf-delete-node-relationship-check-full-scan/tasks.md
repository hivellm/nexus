# Tasks: phase0_perf-delete-node-relationship-check-full-scan

`node_has_live_relationship` (engine/crud/nodes.rs) scans the whole
relationship store (`0..relationship_count()`) on every non-DETACH delete,
making deletes O(total edges) instead of O(degree). Correct but slow on large
graphs.

## 1. Optimize the relationship-existence check
- [ ] 1.1 Benchmark/confirm the current cost: a delete on a graph with many
  relationships scans all of them. Add a micro-measure or reason from the loop
- [ ] 1.2 Replace the full-store scan in `node_has_live_relationship` with a
  bounded lookup: outgoing via `first_rel_ptr`/`next_src_ptr`, incoming via the
  existing relationship index (`self.cache.relationship_index()`) or a dst-keyed
  lookup. Return true on the first live edge found
- [ ] 1.3 Check whether `delete_node_relationships` shares the full-scan cost;
  if so, apply the same O(degree) lookup there
- [ ] 1.4 Confirm results are identical: the guard still finds every live edge
  (outgoing + incoming), still ignores soft-deleted edges

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation (if the
  relationship-lookup contract changes materially; otherwise waive)
- [ ] 2.2 Write tests covering the new behavior: the guard still refuses a
  non-DETACH delete of a node with an incoming-only edge, and still allows
  delete after the edge is removed — reuse
  `delete_node_dangling_relationships_test.rs` shapes
- [ ] 2.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core` — all green)

## Related
- `phase0_fix-delete-node-dangling-relationships` — introduced the correctness
  guard this task makes O(degree)
