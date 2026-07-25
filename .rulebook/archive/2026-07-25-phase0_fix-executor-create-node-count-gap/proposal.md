# Proposal: phase0_fix-executor-create-node-count-gap

**Priority: MEDIUM (correctness). The MATCH/UNWIND-context CREATE path in the
executor never batches node counts, so `UNWIND … CREATE (n:Label)` under-counts
`catalog.node_counts` (a lower bound, often far below the real total). The
node-count twin of the relationship-count gap already fixed in
`phase0_perf-ingest-relationship-fsync-per-edge`.**

## Why

`crates/nexus-core/src/executor/operators/create.rs` has TWO edge/node creation
paths:

- **`execute_create_pattern_internal`** (standalone `CREATE`): accumulates
  `(label_id, +1)` into `label_count_updates` and flushes once after commit via
  `catalog.batch_increment_node_counts` — node counts correct.
- **`execute_create_with_context`** (the `MATCH …`/`UNWIND … CREATE` path): does
  NOT batch node counts at all (it only maintains the label index). So creating
  nodes through this path leaves `catalog.node_counts` a lower bound.

Discovered while adding relationship-count batching to this same function
(`phase0_perf-ingest-relationship-fsync-per-edge` §1.5): that task added a
`rel_count_updates` accumulator + flush to `execute_create_with_context`, but the
symmetric NODE-count accumulator was (deliberately, out of scope) left missing.
`catalog.node_counts` feeds planner cardinality and `get_node_count`, so an
under-count degrades planning and any count-dependent API.

## What Changes

Add a `label_count_updates` accumulator to `execute_create_with_context`,
incremented per node created in that path, flushed once after the transaction
commit via `catalog.batch_increment_node_counts` — exactly mirroring the
standalone-CREATE path and the rel-count accumulator that already sits beside it.

## Impact

- Affected specs: none (internal catalog-stats correctness).
- Affected code: `crates/nexus-core/src/executor/operators/create.rs`
  (`execute_create_with_context` only).
- Breaking change: NO — same query results; only `catalog.node_counts` becomes
  accurate through this path.
- User benefit: correct node cardinality after `UNWIND … CREATE (n:Label)`;
  better query plans; `get_node_count` / stats endpoints stop under-reporting.
- Related: `phase0_perf-ingest-relationship-fsync-per-edge` (added the rel-count
  accumulator to the same function; this closes the node-count half). Note the
  test gotcha: `cargo test` shares one process-wide LMDB catalog, so assert node
  counts via an isolated engine or a unique label name.
