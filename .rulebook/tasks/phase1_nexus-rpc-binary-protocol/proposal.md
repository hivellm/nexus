# Proposal: phase1_nexus-rpc-binary-protocol

## Why

Nexus currently exposes all client traffic through HTTP + JSON on `POST /cypher`
and sibling REST endpoints. Every request pays five costs that Synap's binary
RPC already eliminates in the sister project:

1. **HTTP/1.1 framing overhead** — headers, chunked encoding, and per-request
   TCP handshakes (keep-alive helps but does not remove them) add ~200–400 us
   per round-trip on localhost, which is larger than the Cypher execution time
   for most point reads (target: <1 ms p95).
2. **JSON (de)serialisation** — the row format `[[value1, value2]]` is
   re-parsed on both ends for every query. MessagePack is 3–10x faster and
   ~40% smaller on Cypher result payloads containing numbers, nested arrays,
   and embedding vectors.
3. **Multiplexing impossible** — each HTTP request needs its own
   request/response cycle; SDKs cannot pipeline concurrent Cypher statements
   over a single socket. Synap's RPC uses caller-chosen `id` fields so dozens
   of in-flight queries share one TCP connection.
4. **No native push channel** — live query updates, KNN watch streams, and
   future pub/sub notifications all require SSE or WebSocket today. SynapRPC
   reserves `id = u32::MAX` for server-initiated push frames on the same
   connection, a pattern Nexus should copy for streaming Cypher and index
   change notifications.
5. **Binary vector payloads are double-encoded** — KNN embeddings today travel
   as JSON number arrays (ASCII-encoded f64). A native `Bytes` variant in the
   RPC wire type lets the SDK send `&[f32]`/`&[u8]` directly without base64.

Synap has already validated the design in production: 4-byte LE length prefix
+ MessagePack body, per-connection writer task, per-request Tokio task, full
metrics/tracing hooks. We want to **mirror that exact protocol** (not
reinvent it) so that tooling, observability, and eventually a shared HiveLLM
transport library can be reused.

This task only covers the **server-side binary RPC listener** and wire types.
RESP3 compatibility lives in `phase1_nexus-resp3-compatibility`, and SDK
updates live in `phase2_sdk-rpc-transport-default`.

## What Changes

Add a new `nexus-server::protocol::nexus_rpc` module that mirrors Synap's
`synap_rpc` layout:

```
nexus-server/src/protocol/
|- mod.rs
|- envelope.rs          # Request/Response JSON envelope (already used by MCP)
|- nexus_rpc/
   |- mod.rs           # re-exports
   |- types.rs         # NexusValue, Request, Response (rmp-serde)
   |- codec.rs         # encode_frame / decode_frame / async read/write
   |- server.rs        # spawn_nexus_rpc_listener + handle_connection
   |- dispatch/
      |- mod.rs       # dispatch(state, Request) -> Response
      |- cypher.rs    # CYPHER command (single-query + streaming)
      |- graph.rs     # CREATE_NODE / CREATE_REL / UPDATE / DELETE / MATCH
      |- knn.rs       # KNN_SEARCH, KNN_TRAVERSE
      |- ingest.rs    # INGEST (bulk nodes/rels with binary payloads)
      |- schema.rs    # LABELS / REL_TYPES / PROPERTY_KEYS / INDEXES
      |- database.rs  # DB_LIST / DB_CREATE / DB_DROP / DB_USE
      |- admin.rs     # PING / HELLO / STATS / HEALTH / AUTH
```

Wire types match Synap exactly — renamed to `NexusValue` for project clarity:

```rust
enum NexusValue {
    Null, Bool(bool), Int(i64), Float(f64),
    Bytes(Vec<u8>), Str(String),
    Array(Vec<NexusValue>),
    Map(Vec<(NexusValue, NexusValue)>),
}

struct Request  { id: u32, command: String, args: Vec<NexusValue> }
struct Response { id: u32, result: Result<NexusValue, String> }
```

Framing: `[u32 LE length][rmp-serde body]`. Same 64 MiB max frame size and
reconnect-on-error semantics as Synap.

Config additions to `nexus-server/src/config.rs`:

```toml
[rpc]
enabled = true          # default ON - RPC is the new recommended transport
host    = "0.0.0.0"
port    = 15475         # REST is 15474, RPC is +1

[rpc.limits]
max_frame_bytes         = 67108864   # 64 MiB
max_in_flight_per_conn  = 1024
```

Metrics (prom-compatible, names mirror Synap):

- `nexus_rpc_connections` (gauge)
- `nexus_rpc_commands_total{command, status}` (counter)
- `nexus_rpc_command_duration_seconds{command}` (histogram)
- `nexus_rpc_frame_size_bytes_in` / `..._out` (histograms)

Tracing: `tracing::info_span!("rpc.conn", peer)` per connection;
`tracing::debug_span!("rpc.req", id, cmd)` per request; slow-command warning
threshold of 2 ms (Cypher is heavier than Synap's KV so the threshold is 2x).

The HTTP server **stays untouched** — this task adds the RPC listener in
parallel. Wiring defaults to `rpc.enabled = true` so fresh installs get RPC
by default; existing REST clients are unaffected.

## Impact

- **Affected specs**: `/docs/specs/api-protocols.md` (new section: "Native RPC"),
  new file `/docs/specs/rpc-wire-format.md`.
- **Affected code**:
  - NEW: `nexus-server/src/protocol/nexus_rpc/` (8 files, ~1500 LOC total)
  - MODIFIED: `nexus-server/src/main.rs` (spawn RPC listener alongside HTTP)
  - MODIFIED: `nexus-server/src/config.rs` (+ `RpcConfig` struct)
  - MODIFIED: `nexus-server/Cargo.toml` (+ `rmp-serde`, `tokio` features)
  - MODIFIED: `nexus-core/src/metrics.rs` (+ RPC counters/histograms)
- **Breaking change**: NO — HTTP REST surface unchanged. Fresh installs open an
  additional TCP port (15475); operators who firewall-off new ports must
  either allow it or set `rpc.enabled = false`.
- **User benefit**:
  - 3–10x lower Cypher round-trip latency on localhost and LAN.
  - 40–60% smaller payloads for vector-heavy workloads (KNN).
  - Request pipelining: dozens of concurrent queries over one TCP connection.
  - Native push channel ready for V2 streaming Cypher.

## Non-goals

- RESP3 compatibility — handled in `phase1_nexus-resp3-compatibility`.
- SDK wiring — handled in `phase2_sdk-rpc-transport-default`.
- Removing or deprecating HTTP REST — HTTP stays supported indefinitely for
  curl, tooling, and the web GUI.
- Authentication redesign — RPC reuses the existing `AuthMiddleware` via a
  `HELLO` handshake that accepts the same API key / username+password as the
  `Authorization` header does today.

## Reference

Synap implementation (already validated in production):

- `synap-server/src/protocol/synap_rpc/types.rs` — wire types
- `synap-server/src/protocol/synap_rpc/codec.rs` — framing
- `synap-server/src/protocol/synap_rpc/server.rs` — accept loop
- `synap-server/src/protocol/synap_rpc/dispatch/mod.rs` — command routing
