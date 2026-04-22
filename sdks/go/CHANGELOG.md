# Changelog — Nexus Go SDK

All notable changes to the Go SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html) via Git tags
(Go modules read `v1.0.0` and up from the repo tag).

## [1.0.0] — 2026-04-19

### Added

- **Native binary RPC transport** (`nexus://host:15475`) — a new
  `transport` subpackage (`github.com/hivellm/nexus-go/transport`)
  implements a single-socket `RpcTransport` with length-prefixed
  MessagePack framing, a background reader goroutine that multiplexes
  responses back to pending callers keyed by request id, HELLO+AUTH
  handshake, monotonic `uint32` ids skipping `PUSH_ID` (`0xffffffff`).
- `transport.Mode` type (`"nexus"` / `"resp3"` / `"http"` / `"https"`)
  aligned with the URL scheme and the `NEXUS_SDK_TRANSPORT` env-var
  tokens.
- `transport.Build(opts, creds)` factory with precedence chain: URL
  scheme > env var > `Config.Transport` > default (`"nexus"`).
- `transport.HttpError{StatusCode, Body}` structured HTTP error type;
  `Client.ExecuteCypher` translates it into the SDK-level `*Error`
  so callers keep the existing type-assertion pattern.
- `transport.MapCommand(dotted, payload)` — 26-entry table matching
  `sdks/rust/src/transport/command_map.rs`.
- `Client.TransportMode()` / `Client.EndpointDescription()` /
  `Client.Close()` surface the resolved transport.
- `Client.ExecuteCypherHTTP()` — preserved legacy HTTP-only path for
  callers that need the raw `/cypher` response.
- `sdks/go/transport/transport_test.go` — 34 tests covering endpoint
  parser (9), wire codec roundtrip (8), command map (7), `ParseMode`
  (3), `Build` precedence (4), `Credentials.HasAny` (4 assertions in
  1 test), and a fails-fast-on-connect-refused assertion (1). All pass.
- `github.com/vmihailenco/msgpack/v5` dependency.

### Changed

- **Default endpoint is now `nexus://127.0.0.1:15475`** (RPC). Existing
  callers that pass an explicit `http://` URL are unaffected. Callers
  relying on the default now need either (a) a running Nexus server
  with the RPC listener open (default in 1.0.0) or (b)
  `NEXUS_SDK_TRANSPORT=http` / `Config.Transport: transport.ModeHttp`.
- `Config` grew new fields: `Transport`, `RpcPort`, `Resp3Port`.
- `NewClient(config)` still returns `*Client` (panics on invalid
  configuration); `NewClientE(config)` is the error-returning variant.
- `Client.ExecuteCypher` now dispatches via the active transport. The
  response is decoded from the `NexusValue` envelope into the existing
  `QueryResult` type.

### Migration

- **Opt out of RPC** if your deployment cannot open port `15475`:
  - Env var: `export NEXUS_SDK_TRANSPORT=http`
  - Per-client: `nexus.NewClient(nexus.Config{BaseURL: "http://host:15474", …})`
  - Per-client explicit: `Config{Transport: transport.ModeHttp, BaseURL: "host:15474"}`
- **CRUD helpers** (`CreateNode`, `UpdateNode`, …) continue to hit the
  REST endpoints on the sibling HTTP port. For RPC coverage of those
  flows, call `ExecuteCypher` with equivalent `CREATE` / `MATCH` /
  `SET` / `DELETE` statements.

See [`docs/MIGRATION_SDK_TRANSPORT.md`](../../docs/MIGRATION_SDK_TRANSPORT.md) for the cross-SDK guide.

The shared contract lives at
[`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md)
and the Rust SDK is the reference implementation.

## Earlier versions

Prior to 2026-04-19 the SDK tracked the server's 0.x line informally
via `go.mod` without explicit version tags.
