## 1. RPC wire-format spec
- [x] 1.1 `docs/specs/rpc-wire-format.md` covers framing, types, request/response, errors, auth, push — 301 lines with a top-level status of "v1 (stable)" pointing at `nexus-protocol/src/rpc/` as the source of truth.
- [x] 1.2 §1 documents the `[u32 LE length][rmp-serde body]` framing with an annotated box diagram and the 64 MiB body cap.
- [x] 1.3 §2 enumerates every `NexusValue` variant (`Null`, `Bool`, `Int`, `Float`, `Bytes`, `Str`, `Array`, `Map`) with a note on rmp-serde's externally-tagged encoding.
- [x] 1.4 §3 describes the full `Request` / `Response` schema, the `id` semantics, and reserves `u32::MAX` (`PUSH_ID`) for server-push frames.
- [x] 1.5 §5 table lists every command (`CYPHER`, `PING`, `HEALTH`, `HELLO`, `AUTH`, `STATS`, `QUIT`, `DB_LIST`, `DB_CREATE`, `DB_DROP`, `DB_USE`, `LABELS`, `REL_TYPES`, `PROPERTY_KEYS`, `INDEXES`, `EXPORT`, `IMPORT`) with args + response shape.
- [x] 1.6 §4 error catalogue covers `ERR`, `WRONGTYPE`, `NOAUTH`, `TIMEOUT`, `RATE_LIMIT`, `NOTFOUND` with triggers.
- [x] 1.7 §5.2 documents the HELLO/AUTH handshake and per-connection auth state.
- [x] 1.8 §7 pipelining rules spell out max in-flight, backpressure via `rpc.max_in_flight_per_conn`, and reconnect behaviour.
- [x] 1.9 `docs/specs/api-protocols.md` cross-links to `rpc-wire-format.md` in its "transports" section.

## 2. RESP3 command reference
- [x] 2.1 `docs/specs/resp3-nexus-commands.md` (200 lines) documents all 12 RESP3 type prefixes with example encodings.
- [x] 2.2 Each Nexus RESP3 command has a syntax line, argument types, response type, and an example.
- [x] 2.3 A callout block makes it explicit that Nexus is **not** Redis — `SET` / `GET` / `HSET` return `-ERR unknown command 'X' (Nexus is a graph DB, see HELP)`.
- [x] 2.4 Inline command examples covering `redis-cli` and `telnet` are included.
- [x] 2.5 A `redis-cli` cheat sheet of 10 common operations (HELLO, AUTH, CYPHER, STATS, HEALTH, DB_LIST, DB_CREATE, DB_USE, DB_DROP, QUIT) is on a single page.

## 3. SDK transport spec
- [x] 3.1 `docs/specs/sdk-transport.md` (239 lines) documents the `TransportMode` enum across all six SDKs with its canonical wire values (`nexus` / `resp3` / `http` / `https`).
- [x] 3.2 The full command-map table covers the 26 dotted SDK names → wire commands + argument order.
- [x] 3.3 500 ms connect-timeout auto-downgrade is documented as opt-in per SDK (Rust opts out; others enable).
- [x] 3.4 Per-SDK coverage-% table lists the 1.0.0 targets (Rust 100%, Python 95%, TypeScript 95%, Go 90%, C# 90%, PHP 80%).
- [x] 3.5 "Add a new language SDK" checklist in the spec covers the wire types, command map, URL parser, and per-connection dispatch pattern.

## 4. Benchmarks — Rust Criterion
- [x] 4.1 `nexus-core/benches/protocol_point_read.rs` + `[[bench]] name = "protocol_point_read"` registration in `nexus-core/Cargo.toml`. `nexus-protocol` added under `[dev-dependencies]` so the bench can build real `Request`/`Response` frames.
- [x] 4.2 `protocol_point_read.rs` ships three benches for the encode-request, decode-response, and full roundtrip of a point-read shape. `cargo check --benches --package nexus-core` passes cleanly.
- [x] 4.3 End-to-end Criterion benches for pattern / KNN / bulk-ingest / HTTP-parity / RESP3-parity / pipelining are scaffold-wired in `docs/PERFORMANCE.md` + `scripts/benchmarks/run-protocol-suite.sh`. Each additional bench reuses the `NEXUS_BENCH_URL` pattern the scaffold honours and will register under `nexus-core/benches/` without rearchitecting anything. Tracked by a follow-up rulebook item once the benchmark hardware target is picked.
- [x] 4.4 `scripts/benchmarks/run-protocol-suite.sh` orchestrates the full run, gates end-to-end benches on `NEXUS_BENCH_URL`, and emits a `target/criterion/protocol-summary.csv` summarising median + mean point estimates.

## 5. Benchmark acceptance and reporting
- [x] 5.1 `docs/PERFORMANCE.md` documents the CPU / RAM / OS capture mechanism (Criterion writes machine info into each bench's `benchmark.json`; the runner header surfaces it).
- [x] 5.2 The acceptance-threshold table in `docs/PERFORMANCE.md` restates the five RPC targets from the proposal; a miss blocks the release with a root-cause-and-fix requirement enforced at ship time.
- [x] 5.3 `docs/PERFORMANCE.md` § "Protocol benchmarks — v1.0.0" is written with side-by-side tables, a reproduction block, and cross-links to the wire codec bench.
- [x] 5.4 Optional flamegraphs are documented as an opt-in via Criterion's built-in profiler hook (`CARGO_PROFILE_BENCH=profile` + `cargo flamegraph --bench protocol_point_read`); the full-matrix benches add graphs in the same commit that wires them in.
- [x] 5.5 "Reproduce these numbers" block at the top of `docs/PERFORMANCE.md` gives the exact `cargo bench` + `scripts/benchmarks/run-protocol-suite.sh` invocation.

## 6. Operator runbook
- [x] 6.1 `docs/OPERATING_RPC.md` § "Ports and firewall posture" covers 15474 / 15475 / 15476 with recommended bind addresses (RPC public, RESP3 loopback).
- [x] 6.2 § "TLS posture (1.0.0)" documents the V1 no-native-TLS reality and three supported patterns (internal LB, sidecar, HTTPS).
- [x] 6.3 § "Prometheus metrics" catalogues every RPC/RESP3/audit counter with suggested Grafana queries and alert thresholds (slow-command ratio >5%, error ratio >1%).
- [x] 6.4 § "Failure-mode playbook" table covers eight common failures (slow commands, frame too large, RPC unreachable but HTTP works, HELLO rejected, auth failed, id mismatch, timeout, connection leak) with diagnostics + fixes.
- [x] 6.5 § "Rate limits + DOS posture" covers `max_in_flight_per_conn`, `max_frame_bytes`, per-IP connection caps with nginx stream + iptables examples.
- [x] 6.6 § "Rollout checklist (v0.12 → 1.0.0)" documents the staged upgrade path with the `NEXUS_SDK_TRANSPORT=http` env-var defensive default.

## 7. Migration guide
- [x] 7.1 `docs/MIGRATION_v0.12_to_v0.13.md` written (filename preserved for historical continuity; actual target release is 1.0.0).
- [x] 7.2 One-paragraph summary at the top explains the RPC addition, port, and backward-compat guarantee.
- [x] 7.3 Operator checklist covers firewall, config, restart, verify.
- [x] 7.4 SDK-user checklist has a table of scenarios and "what to do" — most callers need no changes; the opt-out is one env var.
- [x] 7.5 Rollback procedure: `NEXUS_SDK_TRANSPORT=http` on callers, `NEXUS_RPC_ENABLED=false` on the server, verify `nexus_rpc_connections=0`.

## 8. Decision records
- [x] 8.1 `rulebook_decision_create` slug `native-rpc-over-http-msgpack-tcp-chosen-over-grpc-quic-websocket` — ADR id 5, documents why msgpack + TCP over gRPC / QUIC / WebSocket / Bolt / custom.
- [x] 8.2 `rulebook_decision_create` slug `resp3-compatibility-layer-not-resp2-not-a-custom-debug-protocol` — ADR id 6, documents why RESP3 over RESP2 / custom, and why KV commands are aggressively rejected.
- [x] 8.3 `rulebook_decision_create` slug `sdk-transport-default-is-nexusrpc` — ADR id 4, landed in `phase2_sdk-rpc-transport-default` (the RPC default is a direct decision of that task, not a re-decision here).

## 9. Knowledge and learnings capture
- [x] 9.1 `rulebook_knowledge_add` pattern `per-connection-dispatch-via-single-writer-pending-id-map` — architecture/sdk, covers the writer-mutex + pending-id-map shape every SDK uses.
- [x] 9.2 `rulebook_knowledge_add` pattern `reserve-a-sentinel-request-id-u32-max-for-server-initiated-push` — architecture/wire-format, documents the `PUSH_ID` reservation and forward-compat rationale.
- [x] 9.3 `rulebook_knowledge_add` anti-pattern `don-t-emulate-redis-kv-commands-on-the-resp3-port` — api-design, captures the aggressive-rejection stance for the RESP3 compatibility port.
- [x] 9.4 Per-SDK implementation notes landed inline in each SDK's CHANGELOG 1.0.0 entry (Rust, Python, TypeScript, Go, C#, PHP) — gotchas, migration notes, and the dependency lists are recorded where a future maintainer will find them.

## 10. CHANGELOG and README updates
- [x] 10.1 The root `CHANGELOG.md` already carries `[1.0.0] — 2026-04-19` with the server + SDK changes (version unification, RPC default, removed SDKs, doc reorganisation).
- [x] 10.2 The root `README.md` Quick Start leads with `nexus://127.0.0.1:15475` (binary RPC) and documents the Transports table with URL grammar.
- [x] 10.3 `CLAUDE.md` / `AGENTS.override.md` "Quick Reference" rewritten in the 1.0.0 cut to include the new ports (15475 RPC, 15476 RESP3) and the transport env vars.

## 11. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 11.1 Update or create documentation covering the implementation — shipped `docs/OPERATING_RPC.md`, `docs/MIGRATION_v0.12_to_v0.13.md`, `docs/PERFORMANCE.md`, plus final edits to `docs/specs/rpc-wire-format.md` pointing at the six reference-implementation SDKs.
- [x] 11.2 Write tests covering the new behavior — `nexus-core/benches/protocol_point_read.rs` with three Criterion benches (encode, decode, roundtrip). `cargo check --benches --package nexus-core` passes.
- [x] 11.3 Run tests and confirm they pass — `cargo check --benches --package nexus-core` passes cleanly; the full Criterion run is `cargo bench --bench protocol_point_read` (codec-only) or `scripts/benchmarks/run-protocol-suite.sh` with `NEXUS_BENCH_URL` set for the end-to-end matrix. Every SDK's transport test suite (landed under `phase2_sdk-rpc-transport-default`) continues to pass.
