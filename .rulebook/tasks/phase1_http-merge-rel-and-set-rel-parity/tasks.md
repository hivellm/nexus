## 1. Parity harness (Step 0 — safety net, deletes nothing)
- [ ] 1.1 Create `crates/nexus-server/tests/write_path_parity.rs` with the query battery: CREATE node/rel (± RETURN n / n.prop / r / r.prop), MERGE node/rel (± ON CREATE/ON MATCH), SET all forms (n.p, n:L, n += {}, r.p, r += {}), REMOVE, DELETE/DETACH, `$param` values, UNWIND+MERGE, MATCH+CREATE, BEGIN/COMMIT/ROLLBACK section
- [ ] 1.2 Run each query through write_ops path AND `Engine::execute_cypher_with_params`; diff response (columns/rows/error) AND side-effects (fresh MATCH re-read)
- [ ] 1.3 Document every behavioral divergence found; resolve each in favor of engine/Neo4j semantics (record in the harness as expected outputs)

## 2. Route HTTP writes to the engine (Step 2)
- [ ] 2.1 Move audit logging (`audit_logger.log_write_operation`) into a wrapper around the engine call in handler.rs (BLOCKING: must land before write_ops deletion)
- [ ] 2.2 Replace the `is_create_query || is_merge_query → execute_create_or_merge` branch in handler.rs with the same `execute_cypher_with_params` call the MATCH/UNWIND branches use
- [ ] 2.3 Parity harness green on the engine-routed path; existing `api/cypher/tests.rs` green

## 3. AST-predicate routing (Step 3)
- [ ] 3.1 Lift RPC's `needs_engine_interception(&ast)` into shared `api/cypher/routing.rs`, used by HTTP + RPC
- [ ] 3.2 Delete the string-prefix heuristics (`query_upper.starts_with(...)`) from handler.rs
- [ ] 3.3 Routing unit-test table: mixed queries (MATCH+CREATE, UNWIND+MERGE, WITH+MERGE), leading comments, lowercase keywords, EXPLAIN/PROFILE-prefixed writes

## 4. Delete the fork (Step 4)
- [ ] 4.1 Remove `mod write_ops` and delete `crates/nexus-server/src/api/cypher/execute/write_ops.rs`
- [ ] 4.2 Convert the parity harness to assert engine-path behavior only
- [ ] 4.3 Verify original bugs closed: MERGE-rel creates edge idempotently, `SET r.k` persists, `CREATE...RETURN r.prop` projects the value (spec scenarios in specs/http-write-path/spec.md)

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update docs: `docs/nexus/04-write-path-unification.md` progress + CHANGELOG entries for behavioral fixes
- [ ] 5.2 Write tests covering the new behavior (harness + routing table + regression tests for B1/B2/B3/B8)
- [ ] 5.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-server`, clippy zero warnings)
