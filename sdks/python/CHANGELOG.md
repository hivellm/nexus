# Changelog — Nexus Python SDK

All notable changes to the Python SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.15.0] — 2026-04-25

### Changed (BREAKING)

- **`LabelResponse.labels`** is now `List[LabelInfo]` instead of
  `List[str]`. `LabelInfo` is `{name: str, id: int}`, mirroring
  the Rust SDK and matching the new server wire format
  (`{"name": "Person", "id": 0}`). Migrate any
  `for name in resp.labels` loop to `for label in resp.labels:
  label.name`.
- **`RelTypeResponse.types`** mirrors the same change with the new
  `RelTypeInfo` model.
- Re-exported `LabelInfo` / `RelTypeInfo` from the package root.

Tracks [hivellm/nexus#2](https://github.com/hivellm/nexus/issues/2).

## [1.0.0] — 2026-04-19

### Added

- **Native binary RPC transport** (`nexus://host:15475`) — asyncio
  implementation with length-prefixed MessagePack framing, persistent
  TCP stream guarded by a writer lock, single background read-loop task
  that multiplexes responses back to pending futures, HELLO+AUTH
  handshake on connect, monotonic `u32` ids skipping `PUSH_ID`.
- `nexus_sdk.transport` subpackage with `types.py`, `endpoint.py`,
  `codec.py`, `command_map.py`, `rpc.py`, `http_transport.py`, `factory.py`.
- `TransportMode` enum (`NEXUS` / `RESP3` / `HTTP` / `HTTPS`) aligned
  with the URL scheme and the `NEXUS_SDK_TRANSPORT` env-var tokens.
- `NEXUS_SDK_TRANSPORT` env var detection via
  `nexus_sdk.transport.factory.build_transport`.
- `NexusClient.transport_mode` property and `endpoint_description()`
  method surface the resolved transport.
- `msgpack>=1.0` dependency for MessagePack framing.
- pytest suite `tests/test_transport.py` — 44 tests covering endpoint
  parser, wire codec roundtrip, command map, `TransportMode.parse`,
  `build_transport` precedence, and a fails-fast-on-connect-refused
  assertion for the RPC transport.

### Changed

- **Default endpoint is now `nexus://127.0.0.1:15475`** (RPC). Previously
  defaulted to HTTP on `http://localhost:15474`. Existing callers that
  pass an explicit `http://` URL are unaffected. Callers relying on the
  default now need either (a) a running Nexus server with the RPC
  listener open (default in 1.0.0) or (b) `NexusClient(transport='http')`
  / `NEXUS_SDK_TRANSPORT=http`.
- **`NexusClient()` accepts no-args construction** — defaults to the
  local RPC endpoint with no auth (suitable for `127.0.0.1` development).
- `base_url` is now optional on `NexusClient.__init__`.
- `execute_cypher`, `get_stats`, `health_check` dispatch via
  `transport.execute(cmd, args)` rather than calling `httpx` directly.
  `get_stats` folds the RPC flat-counter shape onto the existing
  `DatabaseStats` model so both transports return the same type.
- `User-Agent` header updated to `nexus-sdk/1.0.0`.

### Migration

- **Opt out of RPC** if your deployment cannot open port `15475`:
  - Env var: `export NEXUS_SDK_TRANSPORT=http`
  - Per-client: `NexusClient(base_url="http://host:15474", api_key="...")`
  - Per-client explicit: `NexusClient(transport="http", base_url="host:15474")`
- **CRUD helpers** (`create_node`, `update_node`, `create_relationship`, …)
  continue to target the REST endpoints on the sibling HTTP port
  (`15474`). When using the RPC default, these helpers still work
  because the transport layer keeps a `httpx.AsyncClient` side-car for
  REST-only routes. For full RPC coverage, call `execute_cypher` with
  the equivalent `CREATE` / `MATCH` / `SET` / `DELETE` statements.

See [`docs/MIGRATION_SDK_TRANSPORT.md`](../../docs/MIGRATION_SDK_TRANSPORT.md) for the cross-SDK guide.

## Earlier versions

The SDK predated formal changelog tracking. See git history prior to
2026-04-19 for the 0.1.0 implementation notes.
