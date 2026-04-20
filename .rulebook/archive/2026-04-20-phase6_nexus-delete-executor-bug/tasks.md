## 1. Diagnosis

- [x] 1.1 Minimal executor-layer test that seeds 100 nodes, runs `MATCH (n) DETACH DELETE n`, counts — reproduced via a `tests/probe.rs` (since deleted) against a live RPC listener; confirmed DELETE was a no-op
- [x] 1.2 Run the same statement via the REST `/cypher` endpoint against a live server; confirm whether the bug is executor-wide or RPC-specific — REST works (routes through `engine.execute_cypher`), RPC was the only broken transport
- [x] 1.3 Walk the DELETE path through `crates/nexus-core/src/executor` and `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`; name the layer that drops the mutation — RPC dispatch (`crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs:231`) called `server.executor.execute(&q)` directly, bypassing `engine.execute_cypher_with_context`'s DELETE interception; `executor/operators/expand.rs:561`'s `execute_delete` is an explicit no-op that relies on the upstream interception that RPC was skipping

## 2. Fix

- [x] 2.1 Apply the fix at whichever layer §1.3 identified — `needs_engine_interception` router added to RPC dispatch; queries with `Match` / `Create` / `Delete` / `Merge` / `Set` / `Remove` / `Foreach` now route through `engine.execute_cypher` (commit `d46e2cfc`)
- [x] 2.2 `MATCH (n) DELETE n` on a graph with relationships returns a clear error (or a full DETACH if that is the documented Nexus semantic); no silent no-op — engine path already surfaces `"Cannot DELETE node with existing relationships; use DETACH DELETE"` (nexus-core engine/mod.rs:494)
- [x] 2.3 `MATCH ()-[r]->() DELETE r` drops edges without needing a DETACH clause — verified during the §1.1 probe

## 3. Regression coverage

- [x] 3.1 Executor-layer integration test (no server) asserts post-delete counts for nodes, relationships, properties — `detach_delete_actually_clears_nodes_via_execute_cypher` in `crates/nexus-core/src/engine/tests.rs`
- [x] 3.2 RPC-layer test exercises `NexusRpcClient::reset` (from `nexus-bench`) against a live server and asserts the post-state — `tests/live_rpc::isolation_between_loads_works` + `tests/live_compare::isolation_between_tests_works` assert `count == 0` after reset
- [x] 3.3 Re-run `phase6_bench-live-test-state-isolation`'s `isolation_between_loads_works` + `isolation_between_tests_works` against the patched server — both pass, plus all 9 `#[ignore]` tests run cleanly as a single `cargo test --ignored` parallel batch

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 4.1 Update or create documentation covering the implementation — `CHANGELOG.md` under `1.0.0 → Fixed — RPC DELETE / DETACH DELETE no-op (2026-04-20)`
- [x] 4.2 Write tests covering the new behavior — §3 above (all three items ticked)
- [x] 4.3 Run tests and confirm they pass — 9/9 `#[ignore]` tests pass against a live Nexus + docker Neo4j as a single `cargo test -p nexus-bench --features live-bench,neo4j -- --ignored` invocation
