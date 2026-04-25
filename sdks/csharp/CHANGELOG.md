# Changelog — Nexus C# SDK

All notable changes to the C# SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.15.0] — 2026-04-25

### Changed (BREAKING)

- **`ListLabelsAsync()`** now returns `List<LabelInfo>` instead of
  `List<string>`. `LabelInfo` is `{ Name, Id }`, mirroring the
  Rust and Python SDKs. Wire format change tracked in
  [hivellm/nexus#2](https://github.com/hivellm/nexus/issues/2).
- **`ListRelationshipTypesAsync()`** now returns
  `List<RelTypeInfo>`.
- **Route fix**: `ListRelationshipTypesAsync` previously called the
  non-existent `/schema/relationship-types`; it now hits the real
  server route `/schema/rel_types`. Same change in `RetryableNexusClient`.

## [1.0.0] — 2026-04-19

### Added

- **Native binary RPC transport** (`nexus://host:15475`) — a new
  `Nexus.SDK.Transports` namespace implements a single-socket
  `RpcTransport` using `MessagePack-CSharp` for the `Typeless`
  codec, length-prefixed framing over `TcpClient`, a background
  reader task that multiplexes responses back to pending
  `TaskCompletionSource`s keyed by request id, HELLO+AUTH handshake
  on connect, and monotonic `uint32` ids skipping `PUSH_ID`
  (`0xFFFFFFFFu`).
- `TransportMode` enum (`NexusRpc` / `Resp3` / `Http` / `Https`)
  aligned with the URL scheme and the `NEXUS_SDK_TRANSPORT` env-var
  tokens. `TransportModeParser.Parse` honours `rpc` / `nexusrpc`
  aliases.
- `NexusClientConfig.Transport`, `.RpcPort`, `.Resp3Port` fields.
- `NEXUS_SDK_TRANSPORT` env-var detection via
  `Nexus.SDK.Transports.TransportFactory.Build`.
- `NexusClient.TransportMode` property and `EndpointDescription()`
  method surface the resolved transport; the new `DisposeAsync`
  implementation releases the persistent RPC socket.
- `HttpTransport` — axum-style route table mapping wire verbs onto
  `/cypher`, `/health`, `/stats`, `/databases`, `/session/database`,
  `/schema/*`. Non-2xx responses surface as `HttpRpcException`.
- `CommandMap.Map(dotted, payload)` — 26-entry table matching
  `sdks/rust/src/transport/command_map.rs`.
- `sdks/csharp/Tests/` xUnit project — 49 tests covering endpoint
  parser (9), wire codec roundtrip (8), command map (10),
  `TransportModeParser` (11), `TransportFactory` precedence (5),
  `Credentials.HasAny` (4), and a fails-fast-on-connect-refused
  assertion (1). All 49 pass.
- `MessagePack` 2.5.187 NuGet dependency.

### Changed

- **Default endpoint is now `nexus://127.0.0.1:15475`** (RPC).
  Previously defaulted to HTTP on `http://localhost:15474`. Existing
  callers passing an explicit `http://` URL are unaffected. Callers
  relying on the default now need either (a) a running Nexus server
  with the RPC listener open (default in 1.0.0) or (b)
  `NEXUS_SDK_TRANSPORT=http` / `Transport = TransportMode.Http`.
- `NexusClient.ExecuteCypherAsync` dispatches via the active
  transport. The response is decoded from the `NexusValue` envelope
  into the existing `QueryResult` type.
- `NexusClient` now implements `IAsyncDisposable` in addition to
  `IDisposable`; prefer `await using` to get the RPC socket released
  cleanly.
- The `TestConsoleSimple` ad-hoc runner was removed earlier in the
  1.0.0 cut. The canonical C# tests now live under
  `sdks/csharp/Tests/`.

### Migration

- **Opt out of RPC** if your deployment cannot open port `15475`:
  - Env var: `set NEXUS_SDK_TRANSPORT=http` (Windows) or
    `export NEXUS_SDK_TRANSPORT=http` (Unix)
  - Per-client: `new NexusClient(new NexusClientConfig { BaseUrl = "http://host:15474", ApiKey = "..." })`
  - Per-client explicit: `new NexusClientConfig { Transport = TransportMode.Http, BaseUrl = "host:15474" }`
- **CRUD helpers** (`CreateNodeAsync`, …) continue to hit the sibling
  HTTP port via the side-car `HttpClient`. For full RPC coverage of
  those flows, call `ExecuteCypherAsync` with equivalent Cypher.

See [`docs/MIGRATION_SDK_TRANSPORT.md`](../../docs/MIGRATION_SDK_TRANSPORT.md) for the cross-SDK guide.

## Earlier versions

Prior to 2026-04-19 the SDK used `0.1.0` in `Nexus.SDK.csproj`. See
git history for the HTTP-only implementation notes.
