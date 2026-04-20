## 1. Diagnosis

- [ ] 1.1 Minimal executor-layer test that seeds 100 nodes, runs `MATCH (n) DETACH DELETE n`, counts — reproduces the bug without the bench crate
- [ ] 1.2 Run the same statement via the REST `/cypher` endpoint against a live server; confirm whether the bug is executor-wide or RPC-specific
- [ ] 1.3 Walk the DELETE path through `crates/nexus-core/src/executor` and `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`; name the layer that drops the mutation

## 2. Fix

- [ ] 2.1 Apply the fix at whichever layer §1.3 identified
- [ ] 2.2 `MATCH (n) DELETE n` on a graph with relationships returns a clear error (or a full DETACH if that is the documented Nexus semantic); no silent no-op
- [ ] 2.3 `MATCH ()-[r]->() DELETE r` drops edges without needing a DETACH clause

## 3. Regression coverage

- [ ] 3.1 Executor-layer integration test (no server) asserts post-delete counts for nodes, relationships, properties
- [ ] 3.2 RPC-layer test exercises `NexusRpcClient::reset` (from `nexus-bench`) against an in-process test harness and asserts the post-state
- [ ] 3.3 Re-run `phase6_bench-live-test-state-isolation`'s `isolation_between_loads_works` + `isolation_between_tests_works` against the patched server — both pass

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 4.1 Update or create documentation covering the implementation — mention the DELETE-no-op incident in `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` history, patch note in `CHANGELOG.md`
- [ ] 4.2 Write tests covering the new behavior — §3 above
- [ ] 4.3 Run tests and confirm they pass — `cargo +nightly test --workspace` + the comparative bench `#[ignore]` suite in a single invocation
