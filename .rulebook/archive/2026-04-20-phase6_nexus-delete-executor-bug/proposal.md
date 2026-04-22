# Proposal: fix Nexus executor's silent no-op on DELETE / DETACH DELETE

## Why

Discovered during verification of `phase6_bench-live-test-state-isolation`
on 2026-04-20 (commit `a6caa38e` + the diagnostic probe it mentions):

Running any of the following statements against a live Nexus RPC
listener with 1000 pre-existing nodes:

```
MATCH (n) DETACH DELETE n
MATCH (n) DELETE n
MATCH ()-[r]->() DELETE r
```

All three:

- Parse without error.
- Return `Ok` with 0 rows (the expected envelope shape for a DELETE
  statement).
- **Leave the database completely unchanged.** A follow-up
  `MATCH (n) RETURN count(n)` still returns 1000.

Against a Neo4j 2025.09.0 community container with the same 100-node
TinyDataset plus the `MATCH (n) DETACH DELETE n` reset, the count
correctly drops from 100 → 0. This is a Nexus-specific executor bug.

This silently breaks every workflow that depends on per-test or
per-run state cleanup:

- `phase6_bench-live-test-state-isolation` §4.3 cannot run the
  `#[ignore]` integration tests in a single `cargo test --ignored`
  pass — the second test trips the row-count divergence guard.
- Production workflows that expect DELETE to persist will see
  silent data survival, which is worse than a parse error.

The compatibility report claims 300/300 diff-suite passes including
Section 17 (DELETE/SET). Either the diff suite doesn't exercise the
code path that fails here, or the bug surfaces only via RPC
transport, or it regressed after the suite was last run.

## What Changes

### Diagnosis

- Reproduce the failure via a minimal Rust test (no bench crate
  dependency — a `nexus-core` integration test). Start from an
  embedded engine with 100 nodes, run DETACH DELETE, assert the
  executor's post-state.
- Trace the DELETE path through `crates/nexus-core/src/executor`.
  Check that the operator actually reaches the storage layer, that
  the transaction commits, and that the node-store records are
  marked deleted.
- Check for a RPC-specific issue: does the same statement work via
  the HTTP / REST `/cypher` path? If so the bug lives in the RPC
  dispatch boundary, not the executor.

### Fix

- Drive the fix from whichever layer the diagnosis points at:
  parser, planner, executor operator, storage, or RPC dispatch.
  No speculation on the culprit here — the diagnosis step informs
  the plan.

### Tests

- Regression test at the executor layer: 100-node seed, DETACH
  DELETE, count == 0.
- Regression test at the RPC layer: same flow through
  `NexusRpcClient::reset`, count == 0.
- Re-run `phase6_bench-live-test-state-isolation`'s
  `isolation_between_loads_works` + `isolation_between_tests_works`
  ignored tests. Both already assert `count == 100` on the second
  load; with this bug fixed they pass.

## Impact

- Affected specs: `delete-executor`, `bench-state-isolation`.
- Affected code:
  - `crates/nexus-core/src/executor/**` — whichever operator
    handles DELETE / DETACH DELETE.
  - `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs` —
    possibly, if the bug is at the dispatch boundary.
- Breaking change: NO — this restores the documented semantics
  (DELETE removes nodes). Callers that relied on the silent no-op
  are, by definition, broken.
- User benefit: unblocks
  `phase6_bench-live-test-state-isolation` §4.3, closes the gap
  between the 300/300 compat claim and observed runtime
  behaviour, and stops silent data survival in production workflows
  that depend on DELETE.

Source: surfaced during `phase6_bench-live-test-state-isolation`
verification.
