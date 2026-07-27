# Tasks: phase0_fix-executor-create-node-count-gap

`execute_create_with_context` (the `MATCH …`/`UNWIND … CREATE` executor path in
`crates/nexus-core/src/executor/operators/create.rs`) never batches node counts,
so `UNWIND … CREATE (n:Label)` leaves `catalog.node_counts` a lower bound. The
standalone `execute_create_pattern_internal` path already does this correctly;
mirror it. A `rel_count_updates` accumulator already sits in this function (added
by `phase0_perf-ingest-relationship-fsync-per-edge`) — add the node-count twin
next to it.

## 1. Implementation
- [x] 1.1 In `execute_create_with_context`, add a
  `label_count_updates: HashMap<LabelId, u32>` accumulator (mirroring the one in
  `execute_create_pattern_internal` at ~create.rs:117 and the `rel_count_updates`
  already in this function). Increment `(label_id, +1)` for every label of every
  node this path creates.
- [x] 1.2 After the transaction commit, flush once via
  `self.catalog().batch_increment_node_counts(&updates)` — in the SAME post-commit
  spot the `rel_count_updates` flush already lives, non-fatal on error
  (`tracing::warn!`), exactly like the rel-count and standalone-node-count flushes.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update the `engine/mod.rs` count docs / any stats spec if they still
  imply this path under-counts nodes.
- [x] 2.2 Test: after `UNWIND range(1, K) AS i CREATE (n:Label)` the catalog node
  total for `Label` == K (not a lower bound). IMPORTANT: `cargo test` shares one
  process-wide LMDB catalog, so assert via `setup_isolated_test_engine()` or a
  UNIQUE label name — see the sibling test
  `crates/nexus-core/tests/executor/create_relationship_count_test.rs`.
- [x] 2.3 Run tests — `cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green.

## Related
- `phase0_perf-ingest-relationship-fsync-per-edge` — closed the rel-count half
  of this same function; this closes the node-count half.
