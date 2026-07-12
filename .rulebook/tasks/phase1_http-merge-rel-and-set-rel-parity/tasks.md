## 1. Parity harness (Step 0 — safety net, deletes nothing)
- [x] 1.1 Create `crates/nexus-server/src/api/cypher/write_path_parity.rs` (in-crate `#[cfg(test)] mod write_path_parity;`, wired from `api/cypher/mod.rs` — placement chosen to reuse the `TestContext`/`NexusServer::new` construction helper already established in `api/cypher/tests.rs`, and to call the public `execute_cypher` handler directly) with the query battery: CREATE node/rel (± RETURN n / n.prop / r / r.prop), MERGE node/rel (± ON CREATE/ON MATCH), SET all forms (n.p, n += {}, r.p, r += {}), REMOVE, DELETE/DETACH, `$param` values, UNWIND+MERGE, MATCH+CREATE, leading-comment/lowercase routing probes, BEGIN/COMMIT/ROLLBACK — 23 test functions covering the 17 battery items
- [x] 1.2 Each case executes through the PUBLIC `/cypher` HTTP handler (`execute_cypher`, the same entry point `write_ops.rs`/engine routing dispatches from) and asserts BOTH the response (columns/rows/error) AND side-effects via a fresh `MATCH` re-read — never response-only
- [x] 1.3 Documented every behavioral divergence found (module doc comment in `write_path_parity.rs`): proposal.md's B1/B2/B3 (relationship MERGE/SET/RETURN via `write_ops.rs`) reproduced and pinned to exact source lines, PLUS 5 additional divergences found empirically that proposal.md did not anticipate — B4 (engine `SET x = $param` resolves to `Null`), B6 (engine `UNWIND $param AS row` unsupported), B7 (`write_ops.rs` combined CREATE+REMOVE drops the removal), B8 (engine `SET x = null` stores literal null instead of removing the key), B9 (parser rejects a leading `//` comment line). Each RED case carries an `#[ignore = "known-divergence ..."]` reason string; 15 cases are GREEN today, 8 are RED-and-ignored (see harness header for the full list and re-enable instructions)

## 2. Engine-core write gaps (prerequisite for Step 2 — discovered by the harness)
- [x] 2.0a B4: engine `SET x = $param` — `Expression::Parameter` resolution added to the SET evaluation in `engine/match_exec.rs` (params threaded from `write_exec.rs`); engine tests in `engine/tests/write.rs`
- [x] 2.0b B8: engine `SET x = null` now REMOVES the key (Neo4j semantics), incl. the `+= {k: null}` map-merge path
- [x] 2.0c B6: engine `UNWIND $param AS row` — parameter resolved to a JSON array and iterated; non-array param errors cleanly
- [x] 2.0d B9: parser — `CypherParser::skip_whitespace` (`executor/parser/tokens.rs`) now skips `//` line comments and `/* */` block comments; 23 tokenizer tests green
- [x] 2.0e 17b: engine-routed `BEGIN → CREATE → ROLLBACK` VERIFIED WORKING — the nexus-core ignored rollback tests were stale and are re-enabled (124 lines of new transaction tests in `engine/tests/transactions.rs`, 17/17 green)
- [x] 2.0f Harness cases un-ignored: 5b, 5d, 7 now GREEN (18 passed / 0 failed). Case 15 stays ignored with updated reason — parser half (B9) fixed, but string-prefix ROUTING still misroutes comment-prefixed writes (L1); flips green with section 4. Full nexus-core: 2403 passed / 0 failed

## 3. Route HTTP writes to the engine (Step 2)
- [ ] 2.1 Move audit logging (`audit_logger.log_write_operation`) into a wrapper around the engine call in handler.rs (BLOCKING: must land before write_ops deletion)
- [ ] 2.2 Replace the `is_create_query || is_merge_query → execute_create_or_merge` branch in handler.rs with the same `execute_cypher_with_params` call the MATCH/UNWIND branches use
- [ ] 2.3 Parity harness green on the engine-routed path; existing `api/cypher/tests.rs` green

## 4. AST-predicate routing (Step 3)
- [ ] 3.1 Lift RPC's `needs_engine_interception(&ast)` into shared `api/cypher/routing.rs`, used by HTTP + RPC
- [ ] 3.2 Delete the string-prefix heuristics (`query_upper.starts_with(...)`) from handler.rs
- [ ] 3.3 Routing unit-test table: mixed queries (MATCH+CREATE, UNWIND+MERGE, WITH+MERGE), leading comments, lowercase keywords, EXPLAIN/PROFILE-prefixed writes

## 5. Delete the fork (Step 4)
- [ ] 4.1 Remove `mod write_ops` and delete `crates/nexus-server/src/api/cypher/execute/write_ops.rs`
- [ ] 4.2 Convert the parity harness to assert engine-path behavior only
- [ ] 4.3 Verify original bugs closed: MERGE-rel creates edge idempotently, `SET r.k` persists, `CREATE...RETURN r.prop` projects the value (spec scenarios in specs/http-write-path/spec.md)

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update docs: `docs/nexus/04-write-path-unification.md` progress + CHANGELOG entries for behavioral fixes
- [ ] 5.2 Write tests covering the new behavior (harness + routing table + regression tests for B1/B2/B3/B8)
- [ ] 5.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-server`, clippy zero warnings)
