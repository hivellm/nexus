# Changelog — Nexus Rust SDK

All notable changes to the Rust SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [2.0.0] — 2026-04-25

### Fixed

- **`get_node(id)` now round-trips for id `0`.** The server-side
  validator was rejecting `node_id == 0` before consulting the
  engine, so the natural `create_node` → `get_node` flow shown in
  `examples/basic_usage.rs` returned `node: None` for the very
  first node ever created in a fresh database. The validator no
  longer treats `0` as a sentinel; existence is the engine's job.
  Reported as [hivellm/nexus#2][issue-2].
- **`/health` self-reported version drift.** `health.version` now
  asserts byte-equality with `env!("CARGO_PKG_VERSION")` in CI so a
  future release whose docker image is built before the workspace
  bump fails the test gate instead of leaking the wrong number to
  users.

### Changed (BREAKING)

- **`ListLabelsResponse.labels`** is now `Vec<LabelInfo>` instead of
  `Vec<(String, u32)>`. `LabelInfo` is `{ name: String, id: u32 }`,
  so the second field's meaning is explicit. The wire format also
  changes from a JSON tuple `["Person", 0]` to an object
  `{"name":"Person","id":0}` — non-Rust consumers must update.
- **`ListRelTypesResponse.types`** mirrors the same change with a
  new `RelTypeInfo` struct.
- Migration: `for (name, _) in resp.labels` → `for label in
  &resp.labels` and use `label.name` / `label.id`. Same for
  `list_rel_types()`.

[issue-2]: https://github.com/hivellm/nexus/issues/2

## [1.0.0] — 2026-04-19

### Added

- **Native binary RPC transport** — new default. `NexusClient::new("nexus://host:15475")`
  connects over length-prefixed MessagePack on TCP, backed by
  `nexus-protocol::rpc`. Measurably ~3–10× lower latency and 40–60%
  smaller payloads vs HTTP/JSON on the same workload.
- **`Transport` trait** (`sdks/rust/src/transport/mod.rs`) abstracts
  over the wire format. Two implementations ship: `RpcTransport`
  (lazy TCP connect + `HELLO` + optional `AUTH` + monotonic request
  ids with `PUSH_ID` avoidance) and `HttpTransport` (wraps
  `reqwest::Client` with a hard-coded route table for CYPHER / PING
  / HEALTH / STATS / EXPORT / IMPORT).
- **`TransportMode` enum** — `NexusRpc` (default), `Resp3`, `Http`,
  `Https`. Single-token string serialisation (`"nexus"` / `"resp3"`
  / `"http"` / `"https"`) aligned with the CLI's URL scheme. Parses
  `"rpc"` / `"nexusrpc"` as aliases. **There is no `"nexus-rpc"` or
  `"nexus+rpc"` token.**
- **Endpoint URL parser** (`sdks/rust/src/transport/endpoint.rs`)
  supporting `nexus://host[:port]`, `http://host[:port]`,
  `https://host[:port]`, `resp3://host[:port]`, and bare
  `host[:port]` (defaults to RPC). IPv6 literals supported.
- **Command map** (`sdks/rust/src/transport/command_map.rs`)
  translates SDK dotted names (`graph.cypher`, `db.list`,
  `knn.search`, ...) into wire commands per
  `docs/specs/sdk-transport.md` §6.
- **`ClientConfig.transport`, `rpc_port`, `resp3_port`** fields.
  Transport precedence: URL scheme > `NEXUS_SDK_TRANSPORT` env var
  > config field > default (`NexusRpc`).
- **`NexusClient::endpoint_description()`** and
  **`is_rpc()`** diagnostic accessors for application wrappers
  (e.g. CLI `--verbose` output).
- **Integration test suite** at `tests/rpc_transport.rs` — 10 tests,
  3 of which exercise a real running server when
  `NEXUS_SDK_LIVE_TEST=1` is set (round-trip CYPHER, STATS, HEALTH).

### Changed

- **Default `ClientConfig.base_url`** is now `nexus://127.0.0.1:15475`
  (was `http://localhost:15474`). Every existing `NexusClient::new(...)`
  / `with_api_key(...)` / `with_credentials(...)` call that passed
  an explicit URL continues to work unchanged; callers relying on
  the default now get RPC.
- **`NexusClient::execute_cypher()`, `get_stats()`, `health_check()`**
  route through the `Transport` trait. Public signatures unchanged;
  user code compiles without edits.
- **Bumped workspace dependency** `nexus-protocol` to the
  workspace-path version that ships the RPC codec.
- **Bumped version** 0.1.0 → **1.0.0**. The breaking default-change
  warrants the major bump even though the API surface is stable.

### Opt-out paths

If you need the pre-1.0.0 default (HTTP/JSON on port 15474):

```rust
// Option 1: pass the scheme explicitly
let client = NexusClient::new("http://127.0.0.1:15474")?;

// Option 2: set the env var
std::env::set_var("NEXUS_SDK_TRANSPORT", "http");
let client = NexusClient::with_config(Default::default())?;

// Option 3: override in ClientConfig
let client = NexusClient::with_config(nexus_sdk::ClientConfig {
    transport: Some(nexus_sdk::transport::TransportMode::Http),
    ..Default::default()
})?;
```

### Deferred

- RESP3 transport — `TransportMode::Resp3` parses and is accepted in
  config, but `NexusClient::with_config` returns a structured
  `NexusError::Configuration` pointing at
  `phase2_sdk-rpc-transport-default §2.3` when actually instantiated.
  RESP3 lands in a follow-up when the Rust RESP3 parser is ready.
- `ClientConfig::with_transport(...)` builder shortcut — direct
  struct literal construction works fine; a terse builder API is a
  cosmetic follow-up.
- HTTP-path verbs for legacy multi-database manager methods
  (`list_databases`, `create_database`, `get_database`,
  `drop_database`, `get_current_database`, `switch_database`) —
  these still hit REST on port 15474 via the sibling HTTP URL. They
  move to RPC once the server exposes matching verbs.

### Related

- Task: `phase2_sdk-rpc-transport-default` (sections 1 + 2
  delivered; sections 3–11 queued per-language).
- Related task: `phase2_cli-default-rpc-transport` (already
  archived — the CLI shipped the same transport model in an earlier
  cycle and informs the SDK design).
- ADR: `2026-04-19-sdk-transport-default-is-nexusrpc`.
- Spec: [`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md).

## [0.12.0] and earlier

Unversioned; this SDK tracked the server's 0.x line informally via a
single `0.1.0` crate version. The 1.0.0 release resets the SDK's
independent versioning.
