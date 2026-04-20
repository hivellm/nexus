## 1. Reset hook on `BenchClient`

- [x] 1.1 Add `reset(&mut self, timeout: Duration) -> Result<(), ClientError>` to the `BenchClient` trait with a default impl that returns `ClientError::BadResponse("reset not supported")`
- [x] 1.2 `NexusRpcClient::reset` — `MATCH (n) DETACH DELETE n` over the existing RPC connection, wrapped in `tokio::time::timeout`
- [x] 1.3 `Neo4jBoltClient::reset` — same statement over `neo4rs`, same timeout discipline
- [x] 1.4 Surface the trait method through `nexus_bench::client::BenchClient` re-exports

## 2. Shared test helpers

- [x] 2.1 Extract `both_endpoints`, `nexus_rpc_credentials`, `bolt_credentials` from the two integration files into `tests/common/mod.rs`
- [x] 2.2 Add `reset_both(nexus, neo4j)` helper that calls `reset()` on both sides with the 30 s ceiling and surfaces errors with the engine label
- [x] 2.3 Add `reset_single(client)` for the live_rpc single-engine path

## 3. Test refactor

- [x] 3.1 Every `#[ignore]` test in `tests/live_rpc.rs` calls `reset_single` before `TinyDataset::load_statement`
- [x] 3.2 Every `#[ignore]` test in `tests/live_compare.rs` calls `reset_both` before `TinyDataset::load_statement`
- [x] 3.3 New `#[ignore]` test `isolation_between_tests_works` runs two load→wipe→load cycles and asserts row counts match

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 4.1 Update or create documentation covering the implementation — crate README's "Integration tests" section, `docs/benchmarks/README.md` Integration-tests block
- [x] 4.2 Write tests covering the new behavior — unit tests for each `reset` impl via the mock-client pattern already used by `harness.rs`, plus the `isolation_between_tests_works` ignored test
- [ ] 4.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-bench --features live-bench,neo4j -- --ignored` must pass in a single invocation against long-running servers **[BLOCKED on `phase6_nexus-delete-executor-bug` — Nexus's executor treats `MATCH (n) DETACH DELETE n` as a no-op, verified in commit `a6caa38e`. The reset infra + Neo4j side work; the Nexus side cannot pass until the underlying DELETE bug is fixed.]**
