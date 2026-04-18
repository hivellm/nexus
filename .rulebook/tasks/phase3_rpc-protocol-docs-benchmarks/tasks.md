## 1. RPC wire-format spec
- [ ] 1.1 Create `docs/specs/rpc-wire-format.md` scaffold with sections: framing, types, request/response, errors, auth, push
- [ ] 1.2 Document the `[u32 LE length][rmp-serde body]` framing with an annotated hex example
- [ ] 1.3 Document every `NexusValue` variant with serde-tagged examples
- [ ] 1.4 Document the full `Request`/`Response` schema including `id` semantics and reserved `u32::MAX`
- [ ] 1.5 Table of every command -> args, response shape, example frames
- [ ] 1.6 Error catalogue: `ERR`, `WRONGTYPE`, `NOAUTH`, `TIMEOUT`, `RATE_LIMIT`, `NOTFOUND` with triggers
- [ ] 1.7 Authentication section: HELLO/AUTH handshake, per-connection auth state
- [ ] 1.8 Pipelining rules: max in-flight, backpressure, reconnect behavior
- [ ] 1.9 Cross-link from `docs/specs/api-protocols.md`

## 2. RESP3 command reference
- [ ] 2.1 Create `docs/specs/resp3-nexus-commands.md` with all 12 type prefixes and example encodings
- [ ] 2.2 Document each Nexus RESP3 command: syntax line, argument types, response type, example
- [ ] 2.3 Callout block: "Not Redis — does not support SET/GET/HSET/... semantics"
- [ ] 2.4 Inline command syntax (redis-cli, telnet) worked example
- [ ] 2.5 redis-cli cheat sheet: 10 common operations rendered in one page

## 3. SDK transport spec
- [ ] 3.1 Create `docs/specs/sdk-transport.md` with `TransportMode` enum documentation
- [ ] 3.2 Full command-map table: dotted SDK name -> wire command -> argument order
- [ ] 3.3 Auto-downgrade semantics: connect timeout, failed-reconnect count, fall-through order
- [ ] 3.4 Per-SDK coverage % table (target from phase 2)
- [ ] 3.5 "Add a new language SDK" checklist

## 4. Benchmarks — Rust Criterion
- [ ] 4.1 Create `nexus-core/benches/protocol/` directory; add `[[bench]]` entries to `Cargo.toml`
- [ ] 4.2 Write `rpc_cypher_point_read.rs`: 100k iterations of `MATCH (n:Person {id: $id}) RETURN n`
- [ ] 4.3 Write `rpc_cypher_pattern.rs`: 10k 3-hop patterns with parameterised seed
- [ ] 4.4 Write `rpc_knn_search.rs`: 10k KNN `k=10 dim=768` queries with pre-loaded index
- [ ] 4.5 Write `rpc_knn_bytes_vs_array.rs`: same query with embedding as Bytes vs Array<Float>
- [ ] 4.6 Write `rpc_ingest_bulk.rs`: 1M nodes across 100 frames of 10k each, measure throughput
- [ ] 4.7 Write `http_parity.rs`: run the same workloads against the HTTP endpoint
- [ ] 4.8 Write `resp3_parity.rs`: same workloads via RESP3
- [ ] 4.9 Write `pipelining.rs`: 1000 in-flight requests per connection, measure p50/p95/p99
- [ ] 4.10 Add `scripts/benchmarks/run-protocol-suite.sh` that orchestrates the full run and emits a CSV

## 5. Benchmark acceptance and reporting
- [ ] 5.1 Run the suite on a reference machine; document CPU/mem/OS in the output header
- [ ] 5.2 Verify RPC meets acceptance targets from proposal (all 5 rows); if not, root-cause and fix
- [ ] 5.3 Write `docs/PERFORMANCE.md` § "Protocol benchmarks — v0.13" with side-by-side tables
- [ ] 5.4 Include flamegraphs (optional) for the slowest command in each transport
- [ ] 5.5 Add a "reproduce these numbers" block with exact commands

## 6. Operator runbook
- [ ] 6.1 Create `docs/OPERATING_RPC.md` with ports/firewall section and recommended bind addresses
- [ ] 6.2 TLS posture section: no native V1, LB/stunnel patterns
- [ ] 6.3 Full prometheus catalogue: every RPC and RESP3 metric with suggested alert thresholds
- [ ] 6.4 Failure-mode playbook: 5 common failures, diagnostics steps, fixes
- [ ] 6.5 Rate-limit / DOS posture: tuning `max_in_flight_per_conn`, `max_frame_bytes`, per-IP caps
- [ ] 6.6 Upgrade path from v0.12: which ports to open, env-var compatibility, staged rollout

## 7. Migration guide
- [ ] 7.1 Create `docs/MIGRATION_v0.12_to_v0.13.md`
- [ ] 7.2 One-paragraph summary of the change at the top
- [ ] 7.3 Operator checklist: firewall ports, config file example, server restart
- [ ] 7.4 SDK-user checklist: "nothing required" section with the opt-out env var
- [ ] 7.5 Rollback procedure: set `rpc.enabled = false`, restart, confirm HTTP still works

## 8. Decision records
- [ ] 8.1 `rulebook_decision_create`: "Native RPC over HTTP (msgpack+tcp vs gRPC/QUIC/WebSocket)"
- [ ] 8.2 `rulebook_decision_create`: "RESP3 compatibility layer (not RESP2, not custom)"
- [ ] 8.3 `rulebook_decision_create`: "RPC is the default SDK transport"

## 9. Knowledge and learnings capture
- [ ] 9.1 `rulebook_knowledge_add` pattern: "Use one writer task per connection, per-request dispatch via mpsc"
- [ ] 9.2 `rulebook_knowledge_add` pattern: "Reserve a sentinel request id (u32::MAX) for server-initiated push"
- [ ] 9.3 `rulebook_knowledge_add` anti-pattern: "Do not emulate Redis KV commands on the RESP3 port — return -ERR unknown"
- [ ] 9.4 `rulebook_learn_capture` per SDK: how the transport migration went, surprises, gotchas

## 10. CHANGELOG and README updates
- [ ] 10.1 Add a top-level CHANGELOG entry under v0.13 covering server + SDK changes
- [ ] 10.2 Update root `README.md` Quick Start to mention RPC default
- [ ] 10.3 Update `CLAUDE.md`/`AGENTS.override.md` Quick Reference with new ports (15475/15476)

## 11. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 11.1 Update or create documentation covering the implementation (all docs above)
- [ ] 11.2 Write tests covering the new behavior (the benchmarks themselves; `cargo bench --bench protocol_point_read` runs cleanly, and the acceptance-threshold check in `scripts/benchmarks/check-thresholds.sh` exits 0)
- [ ] 11.3 Run tests and confirm they pass (benchmark suite + threshold check in CI)
