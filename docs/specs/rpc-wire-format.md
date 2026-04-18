# Nexus Binary RPC — Wire Format Specification

> Status: **v1 (stable)**
> Source of truth: [nexus-protocol/src/rpc](../../nexus-protocol/src/rpc/)
> Server: [nexus-server/src/protocol/rpc](../../nexus-server/src/protocol/rpc/)
> Default port: **15475** (additive to HTTP 15474 and RESP3 15476)

The native binary RPC is the preferred transport for first-party Nexus SDKs.
It was designed to replace HTTP + JSON for high-throughput Cypher traffic,
bulk ingest, and KNN queries where the JSON tax (per-request handshake,
string parsing, base64-encoded embeddings) dominates wall-clock time.

Clients that prefer a human-debuggable protocol should keep using HTTP or
RESP3; both continue to run alongside RPC and expose the same functionality.

## 1. Framing

Every frame on the wire has the shape:

```
┌───────────────────┬──────────────────────────┐
│  length: u32 (LE) │  body: MessagePack bytes  │
└───────────────────┴──────────────────────────┘
    4 bytes              length bytes
```

- `length` is the little-endian byte length of `body` (MessagePack payload
  only — excludes the prefix itself).
- `body` is a MessagePack-encoded [`Request`](#request) or
  [`Response`](#response), using `rmp-serde`'s default externally-tagged
  representation.
- Maximum allowed body size is **64 MiB** by default
  (`nexus_protocol::rpc::DEFAULT_MAX_FRAME_BYTES`). Operators can tune it
  via `rpc.max_frame_bytes` in config or the `NEXUS_RPC_MAX_FRAME_BYTES`
  env var. Oversized length prefixes are rejected before the server
  allocates the body buffer.

Both requests and responses share this framing — callers read the prefix,
allocate the body buffer, and then decode once the full frame has arrived.

## 2. `NexusValue`

```rust
pub enum NexusValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Bytes(Vec<u8>),
    Str(String),
    Array(Vec<NexusValue>),
    Map(Vec<(NexusValue, NexusValue)>),
}
```

MessagePack-encoded as `rmp-serde`'s default externally-tagged
representation:

- `NexusValue::Null` serialises as the plain MessagePack string `"Null"`.
- Payload-bearing variants (`Bool(true)`, `Int(42)`, ...) serialise as a
  single-key MessagePack map `{"Variant": payload}`.

This matches the Synap project's `SynapValue` byte-for-byte, so tooling
(Wireshark dissectors, Grafana log pipelines) can be shared across both.

### Accessor helpers (Rust)

- `as_str() -> Option<&str>`
- `as_bytes() -> Option<&[u8]>` (also yields UTF-8 bytes of `Str`)
- `as_int() -> Option<i64>`
- `as_float() -> Option<f64>` (widens `Int` to `f64`)
- `is_null() -> bool`

## 3. `Request`

```rust
pub struct Request {
    pub id: u32,
    pub command: String,
    pub args: Vec<NexusValue>,
}
```

- `id` is chosen by the client. The server echoes it on the matching
  `Response` so multiple in-flight requests can share one TCP connection.
  Reserved: `u32::MAX` is `PUSH_ID` (see §6). Clients that use it get a
  dedicated error back.
- `command` is uppercased by the dispatcher. Case does not matter on the
  wire.
- `args` are positional. See §5 for the per-command argument list.

## 4. `Response`

```rust
pub struct Response {
    pub id: u32,
    pub result: Result<NexusValue, String>,
}
```

- `id` echoes the originating request.
- `result` is `Ok(value)` on success, `Err(message)` on failure. Error
  strings follow the RESP3-style prefix convention (`ERR`, `WRONGPASS`,
  `NOAUTH`, `NOPROTO`) so clients can dispatch on the prefix without
  parsing.

## 5. Command reference

> Argument types shown are _required_ [`NexusValue`] variants. A
> trailing `?` means optional.

### Admin / handshake

| Command | Args | Result |
|---------|------|--------|
| `PING`  | `[]` or `[Str or Bytes]` | `Str("PONG")` or echo of payload |
| `HELLO` | `[]` | `Map { server, version, proto, id, authenticated }` |
| `AUTH`  | `[api_key: Str]` *or* `[username: Str, password: Str]` | `Str("OK")` / `Err("WRONGPASS ...")` |
| `QUIT`  | `[]` | `Str("OK")` (connection is closed after) |
| `STATS` | `[]` | `Map { nodes, relationships, labels, rel_types, page_cache_hits, page_cache_misses, wal_entries, active_transactions }` |
| `HEALTH`| `[]` | `Map { state: "healthy" \| "degraded" \| "unhealthy" }` |

`PING`, `HELLO`, `AUTH`, `QUIT` are always accepted pre-auth. Everything
else returns `Err("NOAUTH ...")` when the listener has `auth.required =
true` and the connection has not yet authenticated.

### Cypher

| Command | Args | Result |
|---------|------|--------|
| `CYPHER` | `[query: Str]` or `[query: Str, params: Map]` | see envelope below |

Result envelope:

```text
Map {
  columns:           Array<Str>,
  rows:              Array<Array<NexusValue>>,
  stats:             Map { rows: Int },
  execution_time_ms: Int,
}
```

Clients that want a query plan embed `EXPLAIN` in the query itself
(`"EXPLAIN MATCH (n) RETURN n"`); the Cypher parser handles it.

### Graph CRUD

| Command       | Args | Result |
|---------------|------|--------|
| `CREATE_NODE` | `[labels: Array<Str>, props: Map]` | `Int` node id |
| `CREATE_REL`  | `[src: Int, dst: Int, type: Str, props: Map]` | `Int` rel id |
| `UPDATE_NODE` | `[id: Int, props: Map]` | `Map { id, labels, properties }` |
| `DELETE_NODE` | `[id: Int, detach: Bool]` | `Bool` (true = deleted) |
| `MATCH_NODES` | `[label: Str, props: Map, limit: Int]` | `Array<NexusValue>` (rows) |

### KNN

| Command | Args | Result |
|---------|------|--------|
| `KNN_SEARCH`   | `[label: Str, embedding: Bytes\|Array<Float>, k: Int, filter: Map?]` | `Array<Map { id: Int, score: Float }>` |
| `KNN_TRAVERSE` | `[seeds: Array<Int>, depth: Int, filter: Map?]` | `Array<Int>` (reachable ids) |

Embeddings:

- **`Bytes`** — raw little-endian `f32` values, length must be a multiple
  of 4. Preferred SDK encoding; no JSON tax.
- **`Array<Float>`** — numeric array for language parity. Int elements are
  widened to `f32`.

The optional `filter` map applies per-property equality predicates to the
returned ids via a narrow follow-up Cypher query, so filtering runs under
the engine's planner rather than in the dispatcher.

### Bulk ingest

| Command  | Args | Result |
|----------|------|--------|
| `INGEST` | `[nodes: Array<Map>, rels: Array<Map>]` | see envelope below |

Node map shape: `{ "labels": Array<Str>, "properties": Map }`
Relationship map shape: `{ "src": Int, "dst": Int, "type": Str, "properties": Map }`

Result envelope:

```text
Map {
  nodes:         Map { created: Int, errors: Int },
  relationships: Map { created: Int, errors: Int },
}
```

Malformed batch items are rejected up-front with a clear error so a bad
map can't half-apply.

### Schema introspection

| Command        | Args | Result |
|----------------|------|--------|
| `LABELS`       | `[]` | `Array<Str>` |
| `REL_TYPES`    | `[]` | `Array<Str>` |
| `PROPERTY_KEYS`| `[]` | `Array<Str>` |
| `INDEXES`      | `[]` | `Array<Map { label: Str, kind: Str }>` |

Reads run against the catalog directly so they work before a single
Cypher query has been executed on a fresh engine.

### Multi-database

| Command     | Args | Result |
|-------------|------|--------|
| `DB_LIST`   | `[]` | `Array<Str>` |
| `DB_CREATE` | `[name: Str]` | `Str("OK")` |
| `DB_DROP`   | `[name: Str]` | `Str("OK")` |
| `DB_USE`    | `[name: Str]` | `Str("OK")` (validates existence) |

Per-session database routing is REST-only in v1; `DB_USE` validates the
database exists but does not rebind the session.

## 6. Reserved ids and server push

`u32::MAX` (`nexus_protocol::rpc::PUSH_ID`) is reserved for server-
initiated push frames (future streaming Cypher, pub/sub notifications).
Clients that use it as their own request id receive:

```
Err("ERR request id u32::MAX is reserved for server push frames")
```

The current v1 server never emits push frames; this is a forward-compat
reservation so SDKs can build demultiplexers that keep `PUSH_ID`
exclusively for server→client traffic.

## 7. Multiplexing and ordering

The server issues one read loop and one writer task per connection, with
`tokio::spawn` per request in between. Request handlers run
concurrently; responses arrive on the wire **in completion order**, not
request order. Every `Response.id` matches the originating request, so
out-of-order completion is safe.

In-flight requests per connection are bounded by
`rpc.max_in_flight_per_conn` (default `1024`). Excess requests block on
a per-connection semaphore rather than being refused.

## 8. Metrics

Prometheus metrics exposed on the existing `/metrics` endpoint:

| Metric | Type | Description |
|--------|------|-------------|
| `nexus_rpc_connections` | gauge | Live RPC TCP connections |
| `nexus_rpc_commands_total` | counter | Total RPC commands dispatched |
| `nexus_rpc_commands_error_total` | counter | Dispatched commands that returned an error |
| `nexus_rpc_command_duration_microseconds_total` | counter | Sum of handler wall-clock μs — average via divide-by `commands_total` |
| `nexus_rpc_frame_bytes_in_total` | counter | Sum of incoming frame sizes |
| `nexus_rpc_frame_bytes_out_total` | counter | Sum of outgoing frame sizes |
| `nexus_rpc_slow_commands_total` | counter | Commands exceeding `rpc.slow_threshold_ms` (default 2 ms) |

Tracing: every connection carries `rpc.conn {peer, id}`; every request
carries `rpc.req {id, cmd}`. Commands slower than
`rpc.slow_threshold_ms` log at WARN.

## 9. Configuration

```toml
[rpc]
enabled = true                  # default on
addr = "0.0.0.0:15475"          # host:port
require_auth = true             # inherits from auth.enabled
max_frame_bytes = 67108864      # 64 MiB
max_in_flight_per_conn = 1024
slow_threshold_ms = 2
```

Environment overrides:

- `NEXUS_RPC_ENABLED`
- `NEXUS_RPC_ADDR`
- `NEXUS_RPC_REQUIRE_AUTH`
- `NEXUS_RPC_MAX_FRAME_BYTES`
- `NEXUS_RPC_MAX_IN_FLIGHT`
- `NEXUS_RPC_SLOW_MS`

## 10. Versioning

- `HELLO.proto = 1` — current version.
- Bump only on **wire-incompatible** changes (framing, `NexusValue` tag
  layout, `Request`/`Response` shape). Adding commands does **not** bump
  the version.

## 11. Reference implementation

- **Rust client**: planned in `phase2_sdk-rpc-transport-default` — will
  live in `nexus-protocol/src/rpc` as a lightweight async client built
  on the same codec.
- **Server**: `nexus-server/src/protocol/rpc/server.rs` (accept loop),
  `nexus-server/src/protocol/rpc/dispatch/*.rs` (command handlers).

See the Synap project's `synap_rpc` module for a second independent
implementation the byte-compatible tooling can cross-verify against.
