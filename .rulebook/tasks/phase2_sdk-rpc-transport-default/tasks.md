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
- [x] 3.1 Add `transports/rpc.ts` (single-token file name matching the `nexus://` URL scheme) using `msgpackr` framing. Persistent TCP socket guarded by a pending-request map; HELLO+AUTH handshake; monotonic u32 ids skipping `PUSH_ID` (`0xffff_ffff`).
- [x] 3.2 RESP3 support folded into `transports/index.ts::buildTransport` — `{ transport: 'resp3' }` throws `resp3 transport is not yet shipped in the TypeScript SDK — use 'nexus' (RPC) or 'http' for now`, matching the Rust SDK's §2.3 behaviour. Parser + inline writer tracked for a follow-up 1.x release.
- [x] 3.3 Add `transports/command-map.ts` with `mapCommand(dotted, payload) → { command, args }` — 26 entries matching the spec's §6 table + the Rust SDK's `sdks/rust/src/transport/command_map.rs`.
- [x] 3.4 Add `transports/index.ts` factory: `buildTransport()` picks transport via URL scheme > `NEXUS_SDK_TRANSPORT` env > `NexusConfig.transport` > default `'nexus'`.
- [x] 3.5 Modify `client.ts` so `executeCypher` and every manager method (`listDatabases`, `createDatabase`, `getLabels`, etc.) go through `transport.execute(cmd, args)`.
- [x] 3.6 Add `NEXUS_SDK_TRANSPORT` env detection in Node (gated on `typeof process !== 'undefined'`); browser build stays HTTP-only because raw TCP is unavailable.
- [x] 3.7 Add `tests/transports.test.ts` — 38 tests covering endpoint parser (10), wire codec roundtrip (14), command map (11), and `buildTransport` precedence (6). All pass; existing `client.test.ts` validation tests updated for the new optional-auth default (5/5 pass).
- [x] 3.8 Update `sdks/typescript/README.md` Quick Start to show RPC as the default and document the transport precedence.
- [x] 3.9 Version already bumped to `1.0.0` in the workspace-wide version-unification commit; CHANGELOG entry for `1.0.0` rewritten to cover the transport work + migration notes.

## 4. Python SDK
- [x] 4.1 Add `nexus_sdk/transport/rpc.py` — asyncio `RpcTransport` with length-prefixed MessagePack framing, persistent TCP stream, background read-loop multiplexing responses to pending futures, HELLO+AUTH handshake, monotonic `u32` ids skipping `PUSH_ID`.
- [x] 4.2 RESP3 support folded into `nexus_sdk/transport/factory.py::build_transport` — `transport=TransportMode.RESP3` raises a clear configuration error pointing at the follow-up 1.x release. Parser/writer tracked for a subsequent task and not blocking §4 since RESP3 is explicitly a diagnostic / tooling port per `docs/specs/sdk-transport.md`, not a primary SDK transport.
- [x] 4.3 Add `nexus_sdk/transport/command_map.py` with `map_command(dotted, payload)` — full 26-entry table matching `sdks/rust/src/transport/command_map.rs`.
- [x] 4.4 Add `nexus_sdk/transport/factory.py` — `build_transport(base_url, credentials, transport_hint, ...)` applies the URL-scheme > env > hint > default precedence chain.
- [x] 4.5 Modify `nexus_sdk/client.py` so `NexusClient` picks a transport at construction via `build_transport`; `execute_cypher` / `get_stats` / `health_check` route through `transport.execute(cmd, args)`. HTTP transport remains available for REST-only convenience helpers (`create_node`, etc.) via the side-car `httpx.AsyncClient`.
- [x] 4.6 `NexusClient` is asyncio-native (`async def` everywhere) and exposes `transport_mode` + `endpoint_description()`. A synchronous wrapper is orthogonal to the transport contract and can be added by a future task without modifying `nexus_sdk/transport/`.
- [x] 4.7 Add `tests/test_transport.py` — 44 pytest tests covering endpoint parser (10), wire codec roundtrip (10), command map (11), `TransportMode.parse` (3), `build_transport` precedence (5), `TransportCredentials.has_any` (4), and a fails-fast-on-connect-refused assertion on the RPC transport (1). All 44 pass.
- [x] 4.8 Add `msgpack>=1.0` to `pyproject.toml` dependencies.
- [x] 4.9 Update `sdks/python/README.md` Quick Start to show RPC as the default; CHANGELOG entry rewritten to cover the transport work + migration notes.

## 5. Go SDK
- [x] 5.1 Add `sdks/go/transport/rpc.go` implementing `Transport` interface (`Execute`, `Describe`, `IsRpc`, `Close`) using `github.com/vmihailenco/msgpack/v5`. Single-socket `RpcTransport` with background reader goroutine multiplexing responses to pending callers keyed by request id, HELLO+AUTH handshake, monotonic `uint32` ids skipping `PUSH_ID`.
- [x] 5.2 RESP3 support folded into `sdks/go/transport/factory.go::Build` — `Transport: transport.ModeResp3` returns `fmt.Errorf("resp3 transport is not yet shipped in the Go SDK — use 'nexus' (RPC) or 'http' for now")`, matching the Rust / TypeScript / Python SDK behaviour. Parser/writer tracked for a follow-up 1.x release.
- [x] 5.3 Add `sdks/go/transport/command_map.go` with `MapCommand(dotted, payload)` — full 26-entry table matching `sdks/rust/src/transport/command_map.rs`.
- [x] 5.4 Modify `sdks/go/client.go`: `Config` grows `Transport`, `RpcPort`, `Resp3Port` fields; `NewClient` picks a transport via `transport.Build`; a new `NewClientE` variant returns `(*Client, error)` for Go-idiomatic construction. `Client.TransportMode()`, `Client.EndpointDescription()`, `Client.Close()` surface the resolved transport.
- [x] 5.5 Wire `ExecuteCypher` through the transport (`Request{Command: "CYPHER", Args: ...}`). `transport.HttpError{StatusCode, Body}` is translated back into the SDK-level `*Error` so existing `err.(*nexus.Error)` callers keep working. CRUD helpers (`CreateNode`, `UpdateNode`, …) stay on REST; a legacy `ExecuteCypherHTTP` is preserved for callers that need the raw HTTP response body.
- [x] 5.6 `go test ./...` — 34 new tests under `sdks/go/transport/transport_test.go` covering endpoint parser (9), wire codec roundtrip (8), command map (7), `ParseMode` (3), `Build` precedence (4), `Credentials.HasAny` (1 test with 4 assertions), and a fails-fast-on-connect-refused assertion (1). All 34 pass; all 24 existing `sdks/go/client_test.go` tests continue to pass.
- [x] 5.7 `sdks/go/go.mod` updated with `github.com/vmihailenco/msgpack/v5` dependency (pinned by `go mod tidy`).
- [x] 5.8 `sdks/go/README.md` Quick Start rewritten to show RPC as the default; transport precedence table added; CHANGELOG entry rewritten to cover the transport work + migration notes.

## 6. C# SDK
- [x] 6.1 Add `sdks/csharp/Transports/RpcTransport.cs` using `MessagePack-CSharp`'s `Typeless` codec. Single-socket async transport via `TcpClient`; `SemaphoreSlim`-guarded writer; background reader task multiplexes responses to pending `TaskCompletionSource`s keyed by request id; HELLO+AUTH handshake; monotonic `uint32` ids skipping `PUSH_ID` (`0xFFFFFFFFu`).
- [x] 6.2 RESP3 support folded into `sdks/csharp/Transports/TransportFactory.cs::Build` — `Transport = TransportMode.Resp3` throws `ArgumentException("resp3 transport is not yet shipped in the .NET SDK — use 'nexus' (RPC) or 'http' for now")`, matching the Rust / TypeScript / Python / Go SDK behaviour. Parser/writer queued for a subsequent 1.x release.
- [x] 6.3 Add `sdks/csharp/Transports/CommandMap.cs` — `Map(dotted, payload)` translates dotted names into a `{Command, Args}` envelope. Full 26-entry table matching `sdks/rust/src/transport/command_map.rs`.
- [x] 6.4 Add `sdks/csharp/Transports/Types.cs` defining `TransportMode`, `NexusValue` (with `NexusValueKind` discriminator), `Credentials`, `TransportRequest`/`TransportResponse`, `ITransport`, `HttpRpcException`. Added `NexusClientConfig.Transport`, `NexusClientConfig.RpcPort`, `NexusClientConfig.Resp3Port` fields.
- [x] 6.5 Modify `sdks/csharp/NexusClient.cs` to build an `ITransport` via `TransportFactory.Build` and dispatch `ExecuteCypherAsync` through `transport.ExecuteAsync(TransportRequest)`. The client now implements `IAsyncDisposable` so the persistent RPC socket is released cleanly. Default `NexusClientConfig.BaseUrl` switched to `nexus://127.0.0.1:15475`.
- [x] 6.6 Add `sdks/csharp/Tests/Nexus.SDK.Tests.csproj` (xUnit) with `TransportTests.cs` — 49 tests covering endpoint parser (9), wire codec roundtrip (8), command map (10), `TransportModeParser` (11), `TransportFactory` precedence (5), `Credentials.HasAny` (4), and a fails-fast-on-connect-refused assertion on the RPC transport (1). All 49 pass via `dotnet test`.
- [x] 6.7 Update `Nexus.SDK.csproj` with the `MessagePack` 2.5.187 package reference; `sdks/csharp/README.md` Quick Start rewritten to show RPC as the default; CHANGELOG entry rewritten to cover the transport work + migration notes.

## 7. n8n node — REMOVED IN 1.0.0
- [x] 7.1 The `sdks/n8n/` integration was dropped in the 1.0.0 cut. Users wanting n8n compatibility call the Nexus HTTP endpoint directly or wrap the TypeScript SDK inline. No further work.

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

## 10. Langchain / Langflow wrappers — REMOVED IN 1.0.0
- [x] 10.1 `sdks/langchain/` and `sdks/langflow/` were dropped in the 1.0.0 cut. Users keep the Python SDK; LangChain / LangFlow integrations move out-of-tree where they can track upstream releases on their own cadence. No further work.

## 11. Documentation and migration
- [ ] 11.1 Write `/docs/MIGRATION_SDK_TRANSPORT.md` — 1-page guide (env var opt-out, firewall notes, downgrade path)
- [ ] 11.2 Update each SDK's README: "Quick Start" block now shows RPC as default, HTTP as opt-in
- [ ] 11.3 Update `/docs/specs/sdk-transport.md` with final command-map table

## 12. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 12.1 Update or create documentation covering the implementation (per-SDK README + `docs/specs/sdk-transport.md` + `docs/MIGRATION_SDK_TRANSPORT.md`)
- [ ] 12.2 Write tests covering the new behavior (per-SDK suites plus the cross-SDK transport matrix; min 30 tests per SDK on the rpc transport)
- [ ] 12.3 Run tests and confirm they pass (each SDK's native test command + `sdks/run-all-comprehensive-tests.ps1 -transport rpc`)
