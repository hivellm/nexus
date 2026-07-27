# Proposal: phase10_thunder-server-migration

## Why

Nexus's native binary RPC (`crates/nexus-protocol/src/rpc` + `crates/nexus-server/src/protocol/rpc`,
port 15475) is a hand-maintained copy of Synap's pre-Thunder wire: length-prefixed
MessagePack, `NexusValue` "matches Synap's SynapValue byte-for-byte"
(`crates/nexus-protocol/src/rpc/types.rs:6`). That wire has since been extracted into
**Thunder** (`e:\HiveLLM\Thunder`, crate `thunder-rpc` 0.2.2, wire v1 **frozen**) — the
shared HiveLLM RPC standard, with conformance vectors, cross-language fuzz, and native
client packages in all 6 SDK languages. Synap already completed this exact migration in
its 1.2.0 release (dissolved `synap-protocol`, kept REST/RESP3 untouched).

Migrating removes ~2k LOC of duplicated wire/codec/accept-loop code from the server and
protocol crates, inherits Thunder's hardening (frame-cap validation before allocation,
slow-loris idle timeout, connection ceiling, metrics observer, session identity), and
aligns Nexus with the family standard so SDK transports (phase11) become thin wrappers
over the per-language Thunder packages.

**Wire compatibility guarantee**: Thunder wire v1 *is* the current Nexus RPC wire
(same frame = u32 LE length + rmp-serde externally-tagged body, same `Request`/`Response`/
value model, same `PUSH_ID = u32::MAX`). Existing deployed SDKs/CLI keep working against
the migrated server unchanged — client migration is decoupled (phase11).

## What Changes

- `Cargo.toml` (workspace): add `thunder-rpc = { version = "0.2.2", default-features = false }`
  to `[workspace.dependencies]`; `nexus-server` takes `features = ["server"]` (+ dev-dep
  with `["client","server"]` for integration tests). Registry version only — no path/git
  (crates.io publish constraint, same as Synap).
- `crates/nexus-server/src/protocol/rpc/`: the hand-rolled accept loop / writer task /
  framing in `server.rs` is replaced by `thunder::server::spawn_listener` +
  `ListenerConfig`. A `NexusDispatch` (impl `thunder::server::Dispatch`) delegates to the
  **existing** `dispatch/` command modules (cypher/graph/knn/ingest/schema/database/
  admin/export) — the command catalog and arg conventions do not change.
- New `protocol/rpc/config.rs`: `nexus_thunder_config()` — single source of truth pinned
  by a test (Synap pattern, `synap_rpc/config.rs`): scheme `nexus`, default port 15475,
  handshake `AuthCommand`, push `Reserved` (no push handlers ship yet), error convention
  `Resp3Prefixes` (shared with the RESP3 listener), `max_frame_bytes` = 64 MiB
  (current `DEFAULT_MAX_FRAME_BYTES`; env `NEXUS_RPC_MAX_FRAME_BYTES` still overrides).
- `crates/nexus-protocol` goes on a **deletion path** — the crate ceases to exist once
  phase11 lands (see "No shared protocol crate" below). In this phase its `rpc/`
  module (types.rs + codec.rs) becomes thin re-exports of `thunder::{Value, Request,
  Response}` + `thunder::wire`, purely so `nexus-cli` and the Rust SDK keep compiling
  across the phase boundary. `rest.rs` / `mcp.rs` / `umicp.rs` / `resp3/` are untouched
  here and relocate into `nexus-server` in phase11.
- Auth: `Dispatch::authenticate` maps `thunder::server::Credentials`
  (ApiKey/UserPass/Token/None) onto the existing auth manager; the pre-auth allowlist
  (PING/HELLO/AUTH/QUIT) and `NEXUS_RPC_REQUIRE_AUTH` semantics are preserved
  (`ListenerConfig.open()` when auth off).
- Metrics: `thunder::server::MetricsObserver` impl feeding the existing `nexus_rpc_*`
  Prometheus series (`protocol/rpc/metrics.rs`) — no dashboard changes.

### No shared protocol crate — duplicate, don't share (project decision)

Nexus will **not** keep a protocol crate after Thunder. `nexus-protocol` is deleted,
and anything the server and the Rust SDK both need is **copied into each side
independently** — exactly what Synap did: its SDK re-declares
`synap_protocol_config()` (`sdks/rust/src/transport/mod.rs:58-68`) byte-identical to
the server's `synap_config()`
(`crates/synap-server/src/protocol/synap_rpc/config.rs:28-37`) rather than importing
it, so the SDK depends only on registry crates and the server publishes nothing.

Consequences for this phase:

- `nexus_thunder_config()` (item 1.2) is the **server's own copy**. The SDK gets its
  own independent copy in phase11. Neither imports the other.
- Each copy is pinned by its own test asserting the same literal values, so drift
  fails CI on whichever side moved. With no shared type, that test pair is the only
  thing holding the two ends of the wire together — treat it as load-bearing.
- Do **not** introduce a replacement `nexus-wire` / `nexus-common` crate to hold these
  constants. That would rebuild the crate we are deleting.

This also removes the two-step publish dance (`nexus-protocol` → wait for index →
`nexus-graph-sdk`, `sdks/rust/PUBLISH.md:17-31`): after dissolution only
`nexus-graph-sdk` is published, and it depends solely on registry crates — which is
also the constraint phase13's crates.io lane needs.

### What does NOT change

- HTTP/REST (15474), RESP3 (15476), MCP `/mcp`, GraphQL — untouched. The Neo4j 300-test
  compat suite and `test-transport-parity.sh` depend on HTTP and must stay green.
- The RPC command catalog (`CYPHER`, `CREATE_NODE`, … `EXPORT`/`IMPORT`), arg shapes,
  response encodings, error strings, ports, env vars.
- The wire format (Thunder v1 == current wire; legacy int-array `Bytes` and map-shaped
  requests still decode per Thunder's compat guarantees).

### Risks / notes (from Synap's migration, CHANGELOG 1.2.0)

- Dockerfile/CI may still reference dissolved paths — Synap's image build broke on a
  `COPY` of the deleted crate. Audit `Dockerfile`/workflows in the tail.
- `Bytes` will now be *emitted* as msgpack `bin` (Thunder canonical). Nexus already emits
  bin (wire copied post-Synap), but the legacy-decode test must prove int-array Bytes
  from old clients still decode.
- Thunder capability parity: connection ceiling, metrics hook, session identity, and
  shareable listener handle all exist since thunder-rpc 0.2.0 (added for Synap). Nexus's
  per-connection in-flight semaphore (`NEXUS_RPC_MAX_IN_FLIGHT`) maps to Thunder's
  bounded spawn-per-request; verify the config knob maps or document the delta.
- TLS: current Nexus RPC is plaintext; Thunder's `tls` feature stays off (parity).

## Impact

- Affected specs: `docs/specs/rpc-wire-format.md`, `docs/specs/api-protocols.md`
- Affected code: `crates/nexus-server/src/protocol/rpc/**`, `crates/nexus-protocol/src/rpc/**`,
  `crates/nexus-server/src/main.rs` (listener spawn), `crates/nexus-server/src/config.rs`
  (RpcConfig mapping), workspace `Cargo.toml`, Dockerfile/CI
- Breaking change: NO on the wire (Thunder v1 == current wire; old clients keep working).
  YES at the Rust API level of `nexus-protocol::rpc` (types become re-exports of
  `thunder`), and the crate is removed outright in phase11 — consumers move to
  `thunder-rpc` directly.
- User benefit: shared hardened transport (pre-allocation frame caps, idle timeouts,
  connection ceiling), ~2k LOC deleted, family-standard protocol enabling phase11 SDK
  simplification and future push/streaming from Thunder upstream.

## References

- Thunder: `e:\HiveLLM\Thunder\rust\thunder\src\server\dispatch.rs` (Dispatch trait),
  `rust/thunder/examples/hello.rs`, `docs/specs/README.md` (SPEC-003/004).
- Synap reference seam: `e:\HiveLLM\Synap\crates\synap-server\src\protocol\synap_rpc\{server.rs,config.rs,mod.rs}`,
  `crates\synap-server\tests\synap_rpc_thunder_tests.rs`, `CHANGELOG.md` 1.2.0.
- Nexus current: `crates/nexus-server/src/protocol/rpc/{server.rs,dispatch/}`,
  `crates/nexus-protocol/src/rpc/{types.rs,codec.rs}`, `main.rs:441`, `config.rs:952-991`,
  `docs/specs/rpc-wire-format.md`.
