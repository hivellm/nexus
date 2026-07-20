# Tasks: phase0_perf-delete-node-relationship-check-full-scan

`node_has_live_relationship` (engine/crud/nodes.rs) scans the whole
relationship store (`0..relationship_count()`) on every non-DETACH delete,
making deletes O(total edges) instead of O(degree). Correct but slow on large
graphs.

## 1. Optimize the relationship-existence check
- [x] 1.1 Confirmed the cost empirically: a runtime probe showed the guard read
  every `RelationshipRecord` (`0..relationship_count()`) on each non-DETACH
  delete — pure O(total edges).
- [x] 1.2 `node_has_live_relationship` is now two-tier. Tier 1 is an
  O(out-degree) walk of the node's own outgoing chain (`first_rel_ptr` →
  `next_src_ptr`) that short-circuits to `true` on the first live outgoing edge.
  **Deviation from the brief:** incoming edges are NOT looked up via
  `relationship_index()` — research proved that index is a non-authoritative
  *hint* (the executor `CREATE` operator and the bulk loader write edges to the
  store WITHOUT maintaining it and WITHOUT setting the dirty flag), so trusting
  it for a correctness guard could wrongly permit deleting a node with a live
  incoming edge — reintroducing the very dangling-edge bug this guard prevents.
  The store has no reverse (incoming) adjacency, so incoming liveness stays the
  authoritative full scan (Tier 2). Safety is preserved *by construction*: Tier
  1 can only conclude `true`; every `false` is still decided by Tier 2.
- [x] 1.3 `delete_node_relationships` genuinely CANNOT be made O(degree) here: it
  must find EVERY connected edge and incoming edges have no reverse adjacency, so
  the full scan is mandatory. Left as-is with an explanatory comment. A true
  O(degree) version needs a store-maintained reverse index — filed as a separate
  task (see Related).
- [x] 1.4 Results identical: 137/137 executor tests green, incl. new coverage
  for outgoing / both-direction / soft-deleted-edge cases and the existing
  incoming-only guard.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Waived: the relationship-lookup contract did not change materially
  (same inputs/outputs, same correctness guarantee). Rationale is captured in
  the doc comments on `node_has_live_relationship` and `delete_node_relationships`.
- [x] 2.2 Added Tier-1 regression tests to
  `delete_node_dangling_relationships_test.rs`:
  `non_detach_delete_of_outgoing_only_node_errors`,
  `non_detach_delete_of_node_with_both_incoming_and_outgoing_edges_errors`,
  `non_detach_delete_allowed_after_outgoing_edge_soft_deleted` (soft-deletes the
  edge at the storage layer because Cypher `DELETE r` is a pre-existing no-op —
  see Related). Incoming-only guard already covered by the existing suite.
- [x] 2.3 Green: `cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core --test executor` (137 passed).

## Related
- `phase0_fix-delete-node-dangling-relationships` — introduced the correctness
  guard this task makes O(degree) on the outgoing side.
- **Follow-up filed:** authoritative store-maintained reverse (incoming)
  adjacency index — the only way to make incoming liveness (and DETACH DELETE's
  edge discovery) O(degree) safely.
- **Bug found while testing:** Cypher relationship `DELETE r` is a no-op stub
  (`executor/operators/expand.rs` `execute_delete` never calls
  `storage::delete_rel`); `relationships_deleted` is reported but nothing is
  soft-deleted. Filed as its own task.
