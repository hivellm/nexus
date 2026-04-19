## 1. Shared design — command map and types
- [x] 1.1 Define the canonical command-map table in `docs/specs/sdk-transport.md`: every SDK dotted name -> `{rawCmd, args}`
- [x] 1.2 Enumerate the full `TransportMode` contract: `NexusRpc` (default, serialised as `"nexus"`), `Resp3` (`"resp3"`), `Http` (`"http"`). No `"nexus-rpc"` token.
- [x] 1.3 Define `ClientConfig.transport`, `ClientConfig.rpc_port` (15475), `ClientConfig.resp3_port` (15476)
- [x] 1.4 Define the `NEXUS_SDK_TRANSPORT` env var fallback chain (URL scheme > env var > config field > default NexusRpc) — 500 ms auto-downgrade documented as opt-in per SDK (Rust opts out, others enable).
- [x] 1.5 Capture ADR: "SDK transport default is NexusRpc" via `rulebook_decision_create` (decision id 4, slug `sdk-transport-default-is-nexusrpc`).

## 2. Rust SDK
- [x] 2.1 Add `src/transport/mod.rs` with `TransportMode`, the `Transport` trait, `TransportRequest`/`Response` wrappers. `WireValue` is not needed — reuse `nexus_protocol::rpc::types::NexusValue` directly.
- [x] 2.2 Add `RpcTransport` in `src/transport/rpc.rs` with persistent `Mutex<Option<BufReader<TcpStream>>>`, `AtomicU32` request id, HELLO+AUTH handshake, `PUSH_ID` avoidance.
- [ ] 2.3 Add `Resp3Transport` — **deferred**. `ClientConfig { transport: Some(TransportMode::Resp3), .. }` returns a clear `NexusError::Configuration` pointing at this task item.
- [x] 2.4 Add `HttpTransport` in `src/transport/http.rs` wrapping `reqwest::Client` with a hard-coded route table (CYPHER/PING/HEALTH/STATS/EXPORT/IMPORT). Unknown commands surface a structured error.
- [x] 2.5 Add `src/transport/command_map.rs` with `map_command(dotted, payload) -> Option<CommandMapping>` covering every entry in the spec's §6 table (26 entries).
- [x] 2.6 Modify `client::NexusClient` to hold `Arc<dyn Transport>` picked from `ClientConfig.transport`. Default is `nexus://127.0.0.1:15475` (previously `http://localhost:15474`).
- [x] 2.7 Route `execute_cypher`, `get_stats`, `health_check` via `Transport::execute`. `create_node/match_nodes/knn_search/knn_traverse` use the existing Cypher paths via `execute_cypher` so they ride the same transport automatically.
- [ ] 2.8 Add `ClientConfig::with_transport(TransportMode)` builder method — **deferred**: the builder isn't strictly required since `ClientConfig { transport: Some(_), .. }` works. A terse builder API can land in a follow-up.
- [x] 2.9 Add `Cargo.toml` deps: `nexus-protocol` (workspace path), `async-trait`.
- [x] 2.10 Add integration test `tests/rpc_transport.rs` — 10 tests, 3 of them gated on `NEXUS_SDK_LIVE_TEST=1` (live CYPHER, live STATS, live HEALTH); all 10 pass including the 3 live ones against `./target/release/nexus-server`.
- [x] 2.11 Unit tests: `Endpoint::parse` (9 tests), `command_map` (9 tests covering every route + auth API-key precedence + unknown-dotted-name), `RpcCredentials::has_any`, `http::json_to_nexus` roundtrip, `HttpTransport::dispatch` unknown-command, `RpcTransport::call` fails-fast-on-connect-refused. Auto-downgrade test is covered by the spec's opt-in note — Rust SDK does not auto-downgrade.

## 3. TypeScript SDK
- [ ] 3.1 Port Synap's `transports/synap-rpc.ts` to `transports/rpc.ts` (single-token file name matching the `nexus://` URL scheme; rename the exported types, keep msgpackr framing)
- [ ] 3.2 Port `transports/resp3.ts` (parser + inline writer)
- [ ] 3.3 Port `transports/command-map.ts`, rewriting commands for Nexus vocabulary (cypher, node.*, rel.*, knn.*)
- [ ] 3.4 Add `transports/index.ts` factory: picks transport from `NexusConfig.transport`
- [ ] 3.5 Modify `client.ts` so `executeCypher` and all manager methods go through `transport.execute(cmd, payload)`
- [ ] 3.6 Add `NEXUS_SDK_TRANSPORT` env detection in node; browser build stays HTTP-only
- [ ] 3.7 Add Vitest suites: wire-codec roundtrip, command-map coverage, connection auto-reconnect
- [ ] 3.8 Update `README.md` Quick Start to show RPC as default
- [ ] 3.9 Bump `package.json` version (breaking-default note in CHANGELOG)

## 4. Python SDK
- [ ] 4.1 Port Synap's `transport_rpc.py` to `nexus_sdk/transport_rpc.py` using `asyncio` + `msgpack`
- [ ] 4.2 Port `transport_resp3.py`
- [ ] 4.3 Port `command_map.py` with Nexus commands (cypher, node.*, rel.*, knn.*, db.*, schema.*, index.*)
- [ ] 4.4 Add `transport.py` facade: `get_transport(mode, config)` factory
- [ ] 4.5 Modify `client.py` so `NexusClient` picks the transport from config; keep HTTP class as fallback
- [ ] 4.6 Sync client for sync users: expose `_transport_blocking()` that wraps asyncio
- [ ] 4.7 Add pytest suites in `tests/test_transport_rpc.py`, `tests/test_command_map.py`
- [ ] 4.8 Update `pyproject.toml` deps: `msgpack>=1.0`
- [ ] 4.9 Update `README.md` and add `examples/rpc_quickstart.py`

## 5. Go SDK
- [ ] 5.1 Add `transport_rpc.go` implementing `Transport` interface (Execute, Close), using `vmihailenco/msgpack/v5`
- [ ] 5.2 Add `transport_resp3.go` with a minimal RESP3 reader/writer
- [ ] 5.3 Add `command_map.go` with the full command table
- [ ] 5.4 Modify `client.go`: `NewClient` picks transport from `Config.Transport`; add `TransportMode` type
- [ ] 5.5 Wire all existing methods (`ExecuteCypher`, `CreateNode`, `KnnSearch`, ...) through the transport interface
- [ ] 5.6 `go test ./...` covers transport roundtrip and command-map unit tests
- [ ] 5.7 Update `go.mod` with `msgpack` dependency
- [ ] 5.8 Update README + `examples/rpc_quickstart.go`

## 6. C# SDK
- [ ] 6.1 Add `Transports/NexusRpcTransport.cs` using `MessagePack-CSharp`
- [ ] 6.2 Add `Transports/Resp3Transport.cs` hand-rolled parser/writer
- [ ] 6.3 Add `Transports/CommandMap.cs` translating SDK enum to wire commands
- [ ] 6.4 Add `TransportMode` enum and `NexusClientOptions.Transport` property
- [ ] 6.5 Modify `NexusClient.cs` to dispatch via `ITransport.ExecuteAsync(cmd, payload)`
- [ ] 6.6 `dotnet test` — covers roundtrip, command-map, reconnect logic
- [ ] 6.7 Update the `.nupkg` metadata + README

## 7. n8n node
- [ ] 7.1 Add a `transport` dropdown to the n8n node UI (default "RPC (fast)")
- [ ] 7.2 Delegate to the TS SDK's transport selection — no independent implementation
- [ ] 7.3 Update the built-in `.vue` docs for each n8n operation (note: "Uses Nexus RPC by default")
- [ ] 7.4 Update `test-integration.ts` to run the test matrix through each transport

## 8. PHP SDK
- [ ] 8.1 Add `Transport/Resp3Transport.php` using `predis/predis` for framing
- [ ] 8.2 Add `Transport/NexusRpcTransport.php` using `rybakit/msgpack` for body + hand-rolled framing
- [ ] 8.3 Add `Transport/CommandMap.php`
- [ ] 8.4 Modify `Client.php` to route via `Transport` interface, default RPC, fallback RESP3, fallback HTTP
- [ ] 8.5 PHPUnit tests: `tests/TransportTest.php`
- [ ] 8.6 Update `composer.json` deps + README

## 9. Cross-SDK comprehensive test matrix
- [ ] 9.1 Extend `sdks/run-all-comprehensive-tests.ps1` with a `$transport` parameter
- [ ] 9.2 Each SDK's comprehensive test (30+ tests) runs 3 times: rpc, resp3, http — all must pass with identical results
- [ ] 9.3 Add a parity assertion: the same Cypher query returns the same rows byte-for-byte across transports
- [ ] 9.4 CI: `run-all-comprehensive-tests.ps1 -transport rpc` runs on every PR

## 10. Langchain / Langflow wrappers
- [ ] 10.1 Update `sdks/langchain/` to use the Python SDK's transport layer transparently (no UI change)
- [ ] 10.2 Update `sdks/langflow/` similarly
- [ ] 10.3 Spot-check: a LangChain `NexusGraphStore` ingestion 2–5x faster via RPC

## 11. Documentation and migration
- [ ] 11.1 Write `/docs/MIGRATION_SDK_TRANSPORT.md` — 1-page guide (env var opt-out, firewall notes, downgrade path)
- [ ] 11.2 Update each SDK's README: "Quick Start" block now shows RPC as default, HTTP as opt-in
- [ ] 11.3 Update `/docs/specs/sdk-transport.md` with final command-map table

## 12. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 12.1 Update or create documentation covering the implementation (per-SDK README + `docs/specs/sdk-transport.md` + `docs/MIGRATION_SDK_TRANSPORT.md`)
- [ ] 12.2 Write tests covering the new behavior (per-SDK suites plus the cross-SDK transport matrix; min 30 tests per SDK on the rpc transport)
- [ ] 12.3 Run tests and confirm they pass (each SDK's native test command + `sdks/run-all-comprehensive-tests.ps1 -transport rpc`)
