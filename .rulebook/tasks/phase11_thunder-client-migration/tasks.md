# Tasks: phase11_thunder-client-migration

Swap every Nexus client's hand-rolled RPC transport + MessagePack codec for the
language's native Thunder client package, then **delete the `nexus-protocol`
crate** — Nexus keeps no shared protocol crate after Thunder; server and Rust SDK
each carry their own independent copy of the protocol config (Synap's model).

Sections 2–7 are independent of each other (one language each) and may run in
parallel. Section 1 (CLI) first, section 8 (crate deletion) last — it requires
every in-workspace consumer to be off `nexus-protocol` already.
Acceptance bar per SDK: its full comprehensive test suite green against a
phase10 server.

## 1. CLI (Rust)
- [ ] 1.1 `crates/nexus-cli/Cargo.toml`: depend on `thunder-rpc = { version = "0.2.2", default-features = false, features = ["client"] }`; rewrite `src/rpc_transport.rs` to wrap `thunder::Client` (connect with `ClientConfig` credentials from CLI auth flags/env; `call(cmd, args)`; map `thunder::ClientError` to CLI errors)
- [ ] 1.2 Verify `src/endpoint.rs` grammar still resolves `nexus://` → Thunder endpoint parse; `src/client.rs` HTTP-fallback paths untouched; run CLI command tests
- [ ] 1.3 Confirm `nexus-cli` no longer references `nexus_protocol::rpc` at all (grep) — the phase10 shims must have zero in-workspace consumers left before section 8 can delete the crate

## 2. Rust SDK
- [ ] 2.1 `sdks/rust/Cargo.toml`: add `thunder-rpc = { version = "0.2.2", default-features = false, features = ["client"] }` (same pin as server — wire ends must not drift) and **remove the `nexus-protocol` dependency** (`:32`); rewrite `src/transport/rpc.rs` around `thunder::Client`, declaring the SDK's **own copy** of the protocol constants locally (Synap pattern: copied, not imported — the SDK must depend only on registry crates)
- [ ] 2.2 Add the SDK-side config pin test asserting the same literal values as the server's `nexus_thunder_config()` test from phase10 §1.3 — with no shared type, this test pair is the only guard against the two ends of the wire drifting apart
- [ ] 2.3 Map `thunder::ClientError` classes (Auth/Server/Connection/Timeout/FrameTooLarge/Decode) to the SDK error type; ensure credentials ride `ClientConfig` and re-send on reconnect
- [ ] 2.4 Run `sdks/rust` test suite (incl. `tests/rpc_transport.rs`) against a phase10 server, and confirm `cargo tree -p nexus-graph-sdk` shows no `nexus-protocol` and no path dependencies — registry crates only

## 3. TypeScript SDK
- [ ] 3.1 `sdks/typescript/package.json`: add `@hivehub/thunder ^0.2.2`; rewrite `src/transports/rpc.ts` around the Thunder client; delete `src/transports/codec.ts` (msgpack codec now Thunder's)
- [ ] 3.2 Preserve `command-map.ts`/`endpoint.ts`/factory selection and HTTP fallback; map Thunder error classes to SDK errors; credentials on connect + reconnect
- [ ] 3.3 Run vitest suite (`tests/{client,transports,multi-database}.test.ts` + live tests) against a phase10 server

## 4. Python SDK
- [ ] 4.1 `sdks/python/pyproject.toml`: add `hivellm-thunder>=0.2.2`; rewrite `nexus_sdk/transport/rpc.py` around `thunder_rpc` client; delete `transport/codec.py`
- [ ] 4.2 Preserve `command_map.py`/`endpoint.py`/`factory.py` and HTTP fallback; map Thunder errors; credentials on connect + reconnect
- [ ] 4.3 Run `nexus_sdk/tests/` suite against a phase10 server

## 5. Go SDK
- [ ] 5.1 `sdks/go/go.mod`: add `github.com/hivellm/thunder-go v0.2.2`; rewrite `transport/rpc.go` around the Thunder client; delete `transport/codec.go`
- [ ] 5.2 Preserve `command_map.go`/`endpoint.go`/`factory.go` and HTTP fallback; map Thunder errors; non-UTF-8 payloads travel as msgpack `bin` via Thunder's value model (Synap Go corruption gotcha); credentials on connect + reconnect
- [ ] 5.3 Run `client_test.go` + `transport/transport_test.go` against a phase10 server

## 6. C# SDK
- [ ] 6.1 `sdks/csharp`: add NuGet `HiveLLM.Thunder`; rewrite `Transports/RpcTransport.cs` around `ThunderClient`; delete `Transports/Codec.cs`
- [ ] 6.2 Preserve `CommandMap.cs`/`Endpoint.cs`/`TransportFactory.cs` and HTTP fallback; map Thunder errors; never send `id = 0xFFFFFFFF` (reserved PUSH_ID — Synap C# gotcha); credentials on connect + reconnect
- [ ] 6.3 Run `Tests/` suite (`dotnet test`) against a phase10 server

## 7. PHP SDK
- [ ] 7.1 `sdks/php/composer.json`: add `hivellm/thunder` — via Packagist if published by then, else a composer VCS repository entry pointing at the thunder-php repo (tagged version, no vendoring); rewrite `Transport/RpcTransport.php` around the Thunder client; delete `Transport/Codec.php`
- [ ] 7.2 Preserve `CommandMap.php`/`Endpoint.php`/`TransportFactory.php` and HTTP fallback; map Thunder errors; PUSH_ID reservation respected; credentials on connect + reconnect
- [ ] 7.3 Run PHPUnit suite against a phase10 server

## 8. Delete the nexus-protocol crate (last — needs sections 1 and 2 done)
- [ ] 8.1 Relocate the crate's non-RPC modules into `nexus-server` (unpublished, mirroring Synap moving RESP3 + HTTP envelope into `synap-server`): `resp3/` (consumed by the server's RESP3 listener), `rest.rs` (`RestClient` — consumers: `crates/nexus-server/tests/vectorizer_integration_test.rs`, `examples/{real_codebase_test_runner,dataset_loader,cypher_test_runner}.rs`), `mcp.rs`, `umicp.rs`. Update every import; do not fold them into one module if they are unrelated
- [ ] 8.2 Delete `crates/nexus-protocol/` and remove it from the workspace `members`; confirm a clean `cargo +nightly build --workspace` and that no `nexus_protocol` reference survives anywhere (grep including Dockerfile, CI workflows, examples, docs — Synap's image build broke on a stale COPY of exactly this)
- [ ] 8.3 Update `sdks/rust/PUBLISH.md`: the two-step publish (`nexus-protocol` → wait for index → `nexus-graph-sdk`) collapses to publishing `nexus-graph-sdk` alone; note this is also what phase13's crates.io lane requires
- [ ] 8.4 Decide and document the fate of the published `nexus-protocol` crate on crates.io (discontinue vs deprecation notice pointing at `thunder-rpc`); note in the release notes that phase15's rmcp fix is now permanently superseded, since the dependency path no longer exists

## 9. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 9.1 Update or create documentation covering the implementation (`docs/specs/sdk-transport.md`: transport internals now Thunder, selection/grammar unchanged; per-SDK READMEs mentioning the codec; document that server and Rust SDK each carry an independent copy of the protocol config and that no shared protocol crate exists; CHANGELOG entry covering the `nexus-protocol` removal)
- [ ] 9.2 Write tests covering the new behavior (per-SDK transport tests updated for the Thunder wrap; error-mapping and reconnect-with-credentials cases per language; the server/SDK config pin tests from phase10 §1.3 and §2.2)
- [ ] 9.3 Run tests and confirm they pass (`sdks/run-all-comprehensive-tests.ps1` all green against a phase10 server; CI workflows `.github/workflows/sdk-*-test.yml` green; workspace gate: `cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` with `nexus-protocol` gone)
