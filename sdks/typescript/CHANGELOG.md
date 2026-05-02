# Changelog

All notable changes to the Nexus TypeScript SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.1.1] - 2026-05-01

### Added

- **Live integration test suite** `tests/external-id.live.test.ts` (phase10 §3):
  - 16 vitest cases covering all six `ExternalId` variants (sha256, blake3,
    sha512, uuid, str, bytes) via `createNodeWithExternalId` +
    `getNodeByExternalId` round-trips.
  - All three conflict policies (`error`, `match`, `replace`), including a
    regression guard for the `replace` prop-ptr fix (commit `fd001344`) that
    reads back the updated property value.
  - Cypher `CREATE (n {_id: '...'}) RETURN n._id` round-trip via
    `executeCypher` — value is compared positionally because the server
    normalises the column alias to `result`.
  - `MATCH ... RETURN n._id` null-projection check for plain nodes.
  - Cypher-created node lookup via `getNodeByExternalId`.
  - Length-cap rejection tests for `str` > 256 bytes, `bytes` > 64 bytes,
    and empty `uuid:` payload.
  - Absent-id returns `null` node (not an HTTP error).
  - Suite is gated on `NEXUS_LIVE_HOST` env var so unit-only CI passes
    without a running container.
- `npm run test:live` script: runs the live suite against
  `http://localhost:15474`.

## [1.0.0] - 2026-04-19

### Added

- **Native binary RPC transport** (`nexus://host:15475`) — length-prefixed
  MessagePack over TCP, persistent socket, HELLO+AUTH handshake, frame
  reassembly with reserved-id avoidance. Ported from the Rust SDK.
- `TransportMode` type (`'nexus'` / `'resp3'` / `'http'` / `'https'`) — single
  token, aligned with the `nexus://` URL scheme. No `'nexus-rpc'` token.
- `NexusConfig.transport`, `NexusConfig.rpcPort`, `NexusConfig.resp3Port`
  configuration fields.
- `NEXUS_SDK_TRANSPORT` env var detection in node builds. Browser builds
  stay HTTP-only (browsers cannot open raw TCP).
- Command-map module — every SDK method funnels through
  `mapCommand(dotted, payload)` which translates the dotted name into a
  `{ command, args }` wire envelope. Parity with `sdks/rust/src/transport/command_map.rs`.
- `NexusClient.endpointDescription()`, `.getEndpoint()`, `.getTransportMode()`,
  `.close()` — surface the resolved transport so callers know where they
  are connected.
- Vitest suite `tests/transports.test.ts` — 38 tests covering URL parsing,
  wire codec roundtrip, command map, and transport precedence resolution.
- `msgpackr` dependency for MessagePack framing.

### Changed

- **Default endpoint is now `nexus://127.0.0.1:15475`** (RPC). Previously
  defaulted to HTTP on `http://localhost:15474`. Existing HTTP callers
  are unaffected if they pass `baseUrl: 'http://…'` explicitly; callers
  relying on the default now need to either (a) run the Nexus server
  with the RPC listener open (default in 1.0.0) or (b) opt back in with
  `{ transport: 'http' }` or `http://…`.
- **`NexusClient()` accepts no-args construction** — defaults to the
  local RPC endpoint with no auth (suitable for `127.0.0.1` development).
- `baseUrl` and `auth` are now optional on `NexusConfig`. Auth validation
  only rejects an empty `apiKey` or a mismatched username/password pair.
- All manager methods (`executeCypher`, `listDatabases`, `getLabels`, etc.)
  dispatch via `transport.execute(cmd, args)` rather than a raw axios
  instance — same public signatures, different wire path.
- `executeBatch` is now sequential on a single TCP socket so RPC frames
  cannot interleave.

### Migration

- **Opt out of RPC** if your deployment cannot open port `15475`:
  - Env var (process-wide): `export NEXUS_SDK_TRANSPORT=http`
  - Per-client: `new NexusClient({ baseUrl: 'http://host:15474', auth: {...} })`
  - Per-client explicit: `new NexusClient({ transport: 'http', baseUrl: 'host:15474' })`
- **Port changes**: RPC listens on `15475`, HTTP on `15474`. If you were
  pointing at a custom `baseUrl: 'http://host:7687'`, keep that — the
  SDK honours whatever you pass.
- **Auth**: API keys and username/password continue to work on both
  transports. RPC sends an `AUTH` frame right after the HELLO handshake;
  HTTP sends `X-API-Key` / `Authorization` headers.

See [`docs/MIGRATION_SDK_TRANSPORT.md`](../../docs/MIGRATION_SDK_TRANSPORT.md) for the cross-SDK guide.

## [0.12.0] - 2025-11-28

### Added
- Multi-database support:
  - `listDatabases()` - List all databases
  - `createDatabase(name)` - Create a new database
  - `getDatabase(name)` - Get database information
  - `dropDatabase(name)` - Drop a database
  - `getCurrentDatabase()` - Get current session database
  - `switchDatabase(name)` - Switch to a different database
- Full data isolation between databases
- Database lifecycle management support

### Changed
- Updated to work with Nexus Server v0.12.0

## [0.11.0] - 2025-11-16

### Added
- Initial release of Nexus TypeScript/JavaScript SDK
- Full TypeScript support with complete type definitions
- API Key and username/password authentication
- Cypher query execution with parameters
- Node CRUD operations (create, read, update, delete)
- Relationship CRUD operations
- Schema introspection (labels, relationship types)
- Batch operations support
- Automatic retry logic with exponential backoff
- Comprehensive error handling
- Query statistics and monitoring
- Plan cache management
- High test coverage
- Complete documentation and examples

### Features
- `NexusClient` class for all database operations
- Type-safe query parameters
- Async/await support
- Connection pooling
- Request timeout configuration
- Debug logging option
- Multiple export formats (CommonJS, ES Modules)

## [Unreleased]

### Planned
- Transaction support
- Index management operations
- Graph algorithm wrappers
- Streaming large result sets
- Connection pooling improvements
- Browser compatibility

