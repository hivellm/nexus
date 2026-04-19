# Changelog

All notable changes to the Nexus TypeScript SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-04-19

### Changed

- **Version aligned to 1.0.0** across all Nexus SDKs (was 0.12.0).
  No runtime behaviour changes in this release — the SDK continues to
  talk HTTP/JSON against the Nexus REST endpoint on port 15474.

### Pending (tracked by `phase2_sdk-rpc-transport-default` §3)

The following work lands in a subsequent 1.x release:

- **Native binary RPC transport** (`nexus://host:15475`) — default
  transport in the shared SDK contract, already shipped by the Rust
  SDK. TypeScript implementation will use `msgpackr` for MessagePack
  framing.
- `TransportMode` enum with `'nexus'` / `'resp3'` / `'http'` values
  (single-token, aligned with the URL scheme; no `'nexus-rpc'` token).
- `NEXUS_SDK_TRANSPORT` env var detection in node builds. Browser
  builds stay HTTP-only because the browser cannot open raw TCP.
- RESP3 transport.
- 500 ms connect-timeout auto-downgrade to HTTP.
- Command-map parity with the spec's §6 table.

The shared contract lives at
[`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md)
and the Rust SDK is the reference implementation.

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

