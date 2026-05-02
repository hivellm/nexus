# Changelog — Nexus Go SDK

All notable changes to the Go SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html) via Git tags
(Go modules read `v1.0.0` and up from the repo tag).

## [2.1.0] — 2026-05-02

### Added — `phase9_external-node-ids`

- **`Client.CreateNodeWithExternalID(ctx, labels, props, externalID, conflictPolicy)`**
  helper around `POST /data/nodes` with the new `external_id` +
  `conflict_policy` request fields. Accepts the prefixed external-id
  string form (`sha256:<hex>`, `blake3:<hex>`, `sha512:<hex>`,
  `uuid:<canonical>`, `str:<utf8 ≤256 B>`, `bytes:<hex ≤128 chars>`);
  `conflictPolicy` is `""` / `"error"` (default), `"match"`, or
  `"replace"`.
- **`Client.GetNodeByExternalID(ctx, externalID)`** — resolves a node by
  its prefixed external-id string via `GET /data/nodes/by-external-id`;
  returns `node == nil` when absent (no HTTP error).
- `CreateNodeRequest.ExternalID` + `CreateNodeRequest.ConflictPolicy`
  fields with `omitempty` JSON tags so legacy callers stay
  wire-compatible.
- Test coverage in `sdks/go/test/test_sdk.go` and unit tests for URL
  encoding.

### Added — `phase10_external-id-live-suite`

- `sdks/go/test/external_id_live_test.go` — 15 live integration tests
  (build tag `live`) covering all six `ExternalId` variants
  (sha256/blake3/sha512/uuid/str/bytes), all three conflict policies
  (error/match/replace), Cypher `_id` round-trip, length-cap rejection
  (str >256 bytes, bytes >64 bytes, empty uuid payload), and absent
  external-id GET. Gate on `NEXUS_LIVE_HOST` env var — skipped when unset.
  Run with: `NEXUS_LIVE_HOST=http://localhost:15474 go test -tags=live -v ./test/...`
- README `External IDs` quickstart section with copy-pasteable snippet for
  `CreateNodeWithExternalID`, `GetNodeByExternalID`, and Cypher `_id` usage.

## [2.0.0] — 2026-04-25

### Changed (BREAKING)

- **`Client.ListLabels(ctx)`** now returns `[]LabelInfo` instead of
  `[]string`. `LabelInfo` is `{Name, ID}` — `ID` is the catalog id
  allocated by the engine, not a count. Mirrors the Rust and
  Python SDKs and the new server wire shape (`{"name":..., "id":...}`).
- **`Client.ListRelationshipTypes(ctx)`** now returns
  `[]RelTypeInfo`.
- **Route fix**: `ListRelationshipTypes` was previously calling the
  non-existent `/schema/relationship-types`; it now hits the real
  server route `/schema/rel_types`. Same change applied in
  `RetryableClient`.

Migration: replace `for _, name := range labels { … }` with
`for _, label := range labels { … label.Name … label.ID … }`.

Tracks [hivellm/nexus#2](https://github.com/hivellm/nexus/issues/2).

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
