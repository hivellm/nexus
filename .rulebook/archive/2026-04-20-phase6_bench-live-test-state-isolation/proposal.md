# Proposal: per-test state isolation for nexus-bench live integration tests

## Why

The `#[ignore]` integration tests under
`crates/nexus-bench/tests/{live_rpc,live_compare}.rs` each load the
100-node `TinyDataset` with a single `CREATE` statement before
running scenarios. They were validated individually in commit
`c4000512` but fail when run as a batch: `cargo test --ignored`
executes test functions against the same long-running Nexus + Neo4j
servers, the `CREATE` stacks cumulatively, and scenarios that assume
a single node at `id: 42` (`point_read.by_id`) trip
`ERR_BENCH_OUTPUT_DIVERGENCE(expected 1, got 2)` on the second run.

The divergence guard is behaving correctly. What is missing is
per-test fixture isolation — each test should start from a clean
database so the suite can be run unattended in CI and by a developer
doing a `cargo test --ignored` pass without manual server resets
between iterations.

## What Changes

### Reset hook on `BenchClient`

- Add a narrow `reset()` method to the `BenchClient` trait: issues a
  `MATCH (n) DETACH DELETE n` (or equivalent transport-specific
  wipe) and returns `Ok(())` when the database is empty afterward.
- `NexusRpcClient::reset` speaks CYPHER over the existing
  connection.
- `Neo4jBoltClient::reset` issues the same statement via `neo4rs`.
- Both implementations wrap the call in the same
  `tokio::time::timeout` guard rail the other methods use.

### Shared test helper

- Promote the `both_endpoints` / `nexus_rpc_credentials` /
  `bolt_credentials` helpers out of the two integration test files
  into a `tests/common/mod.rs` module so they are shared.
- Add a `reset_both(client_a, client_b)` helper in that module that
  every test at the top of its body invokes before loading the
  dataset.

### Test refactor

- Every `#[ignore]` test in `tests/live_rpc.rs` and
  `tests/live_compare.rs` calls `reset_both` (or the single-engine
  equivalent) first, then loads `TinyDataset`, then runs its
  scenario. Tests stay independent — order no longer matters and
  `cargo test --ignored` runs cleanly against long-running servers.

### Tests

- Unit tests for each client's `reset` method verify the correct
  Cypher is emitted. No live server needed — the harness's existing
  mock test patterns cover this.
- A new `#[ignore]` test `isolation_between_tests_works` runs the
  load-then-wipe-then-load cycle twice against live servers and
  asserts the row count is the same each time.

## Impact

- Affected code:
  - `crates/nexus-bench/src/client/mod.rs` — new trait method on
    `BenchClient`.
  - `crates/nexus-bench/src/client/rpc.rs` — `reset` impl.
  - `crates/nexus-bench/src/client/neo4j.rs` — `reset` impl.
  - `crates/nexus-bench/tests/common/mod.rs` — new shared helpers.
  - `crates/nexus-bench/tests/live_rpc.rs`,
    `tests/live_compare.rs` — refactor every `#[ignore]` test to
    call `reset_both` first.
- Breaking change: NO — additive trait method + test refactor.
- User benefit: the `#[ignore]` suite can finally run unattended
  against a long-running server pair; CI gate + nightly workflow
  stop depending on fresh-state prerequisites baked into the
  trigger scripts.

Source: surfaced during `phase6_bench-neo4j-docker-harness` §5.3
verification (commit `c4000512`).
