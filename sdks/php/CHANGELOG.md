# Changelog — Nexus PHP SDK

All notable changes to the PHP SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html) via Git tags
(Composer resolves `^1.0` from the `v1.0.0` repo tag).

## [1.15.0] — 2026-04-25

### Changed (BREAKING)

- **`NexusClient::listLabels()`** now returns
  `array<int, array{name: string, id: int}>` instead of
  `string[]`. Mirrors the Rust / Python / C# / Go SDKs and matches
  the new server wire shape (`{"name":..., "id":...}`). Migrate
  any `foreach ($labels as $name)` loop to
  `foreach ($labels as $label) { $label['name']; $label['id']; }`.
- **`NexusClient::listRelationshipTypes()`** mirrors the same
  change.
- **Route fix**: both `NexusClient::listRelationshipTypes()` and
  the `REL_TYPES` HTTP fallback in
  `Transport\HttpTransport::execute()` were previously calling the
  non-existent `/schema/relationship-types`; they now hit the real
  server route `/schema/rel_types`.

Tracks [hivellm/nexus#2](https://github.com/hivellm/nexus/issues/2).

## [1.0.0] — 2026-04-19

### Added

- **Native binary RPC transport** (`nexus://host:15475`) — a new
  `Nexus\SDK\Transport` namespace implements a synchronous
  single-socket `RpcTransport` using `rybakit/msgpack` for the
  MessagePack body and hand-rolled length-prefix framing over
  `stream_socket_client`. HELLO+AUTH handshake on connect; monotonic
  `uint32` ids skipping `PUSH_ID` (`0xFFFFFFFFu`).
- `TransportMode` enum (`NexusRpc` / `Resp3` / `Http` / `Https`)
  aligned with the URL scheme and the `NEXUS_SDK_TRANSPORT` env-var
  tokens. `TransportMode::parse()` accepts `rpc` / `nexusrpc`
  aliases.
- `Config::$transport`, `Config::$rpcPort`, `Config::$resp3Port`
  fields on the client config.
- `Transport` interface (`Nexus\SDK\Transport\Transport`) +
  `HttpTransport` implementation wrapping GuzzleHttp with a route
  table for CYPHER / PING+HEALTH / STATS / DB_* / schema. Non-2xx
  responses surface as `HttpRpcException`.
- `CommandMap::map(dotted, payload)` — 26-entry table matching
  `sdks/rust/src/transport/command_map.rs`.
- `TransportFactory::build(baseUrl, credentials, …)` — precedence
  chain: URL scheme > `NEXUS_SDK_TRANSPORT` env > hint > default
  (`NexusRpc`).
- `NexusClient::getTransportMode()` / `endpointDescription()` /
  `close()` surface the resolved transport.
- `rybakit/msgpack` 0.9 Composer dependency.
- `tests/TransportTest.php` — 30+ PHPUnit tests covering endpoint
  parser, wire codec roundtrip, command map, `TransportMode::parse`,
  `TransportFactory` precedence, and `Credentials::hasAny`.

### Changed

- **Default endpoint is now `nexus://127.0.0.1:15475`** (RPC).
  Previously defaulted to HTTP on `http://localhost:15474`. Existing
  callers passing an explicit `http://` URL are unaffected. Callers
  relying on the default now need either (a) a running Nexus server
  with the RPC listener open (default in 1.0.0) or (b)
  `NEXUS_SDK_TRANSPORT=http` / `Config::$transport = TransportMode::Http`.
- `NexusClient::executeCypher` dispatches via the active transport.
  The response is decoded from the `NexusValue` envelope into the
  existing `QueryResult` type.
- `NexusClient::__destruct` releases the persistent RPC socket.

### Migration

- **Opt out of RPC** if your deployment cannot open port `15475`:
  - Env var: `export NEXUS_SDK_TRANSPORT=http`
  - Per-client: `new Config(baseUrl: 'http://host:15474', apiKey: '...')`
  - Per-client explicit: `new Config(transport: TransportMode::Http, baseUrl: 'host:15474')`
- **CRUD helpers** (`createNode`, `updateNode`, …) continue to hit
  the REST endpoints via the side-car Guzzle client. For full RPC
  coverage, call `executeCypher` with equivalent Cypher statements.

See [`docs/MIGRATION_SDK_TRANSPORT.md`](../../docs/MIGRATION_SDK_TRANSPORT.md) for the cross-SDK guide.

## Earlier versions

Prior to 2026-04-19 the SDK shipped without an explicit version
field (Composer consumers pulled `dev-main` from the mono-repo).
