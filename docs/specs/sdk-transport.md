# SDK Transport Specification

Status: **Accepted** — enforced by `phase2_sdk-rpc-transport-default`.

This document is the canonical contract every Nexus SDK implements. It
describes the transport selection model, the URL grammar, the
`ClientConfig` fields, the environment-variable opt-out chain, and the
command-map that lets SDK methods translate to wire commands.

## 1. Transport modes

The SDK recognises three transport modes:

| Enum variant | String value | Default port | Wire format                |
|--------------|--------------|--------------|----------------------------|
| `NexusRpc`   | `"nexus"`    | `15475`      | length-prefixed MessagePack (see `docs/specs/rpc-wire-format.md` once landed) |
| `Resp3`      | `"resp3"`    | `15476`      | RESP3 over TCP              |
| `Http`       | `"http"`     | `15474`      | JSON/HTTP (legacy)          |

**There is no `"nexus-rpc"` or `"nexus+rpc"` token.** The canonical
single-token identifier is `nexus`. Anything richer is a typo and MUST
be rejected.

**RPC is the default** for every SDK starting with phase 2 of the
transport migration. The choice is measurable:
- ~3–10× lower query latency vs HTTP/JSON
- ~40–60% smaller payloads
- Persistent TCP connection, no per-request TCP handshake
- Bytes variants are native (no base64 step for KNN embeddings)

RESP3 is offered for compatibility with redis-tooling (redis-cli,
grafana tail, debug dashboards). HTTP remains available for networks
where only port 80/443 is reachable and for SDK operations that do
not yet have an RPC verb.

## 2. URL grammar

SDKs accept three URL forms in `ClientConfig.baseUrl`:

```
nexus://host[:port]        → TransportMode::NexusRpc, default port 15475
http://host[:port]         → TransportMode::Http,     default port 15474
https://host[:port]        → TransportMode::Http (TLS), default port 443
host[:port]                → TransportMode::NexusRpc, default port 15475
```

The URL scheme is a **stronger** signal than the `transport` config
field. If a user passes `baseUrl: "nexus://db:15475"` *and*
`transport: TransportMode::Http`, the URL wins. (The `transport` field
acts as an override only for bare URLs that do not carry a scheme, or
to force a downgrade like "use HTTP even against `http://` when the
user's config previously defaulted to something else".)

## 3. `ClientConfig` fields

Every SDK's `ClientConfig` (or language-idiomatic equivalent) carries
at minimum:

```rust
pub struct ClientConfig {
    /// Endpoint URL. Accepts `nexus://`, `http://`, `https://`, or
    /// bare `host:port` (treated as `nexus://`).
    pub base_url: String,

    /// Explicit transport override. `None` means "infer from the URL
    /// scheme" (the recommended default).
    pub transport: Option<TransportMode>,

    /// Default RPC port if the URL does not supply one.
    pub rpc_port: u16,           // 15475

    /// Default RESP3 port if the URL does not supply one.
    pub resp3_port: u16,         // 15476

    /// Request timeout applied to every transport.
    pub timeout_secs: u64,

    /// Optional API key.
    pub api_key: Option<String>,

    /// Optional username + password pair.
    pub username: Option<String>,
    pub password: Option<String>,

    pub max_retries: u32,
}
```

## 4. `NEXUS_SDK_TRANSPORT` env var

When the process env `NEXUS_SDK_TRANSPORT` is set, it **overrides**
the `transport` config field but NOT the URL scheme. Accepted values
map case-insensitively to `TransportMode`:

| Env value                        | Effect                      |
|----------------------------------|-----------------------------|
| `nexus` / `rpc` / `nexusrpc`     | Force `NexusRpc`            |
| `resp3`                          | Force `Resp3`               |
| `http` / `https`                 | Force `Http`                |
| `auto` / *unset*                 | Infer from URL / config     |

The check runs once at client construction. Subsequent env changes do
not affect an already-built `NexusClient`.

## 5. Auto-downgrade

If the SDK fails to connect to the RPC port within the first 500 ms
of client construction, it MAY auto-downgrade to HTTP against the
sibling port (`15474`). The downgrade:

1. Is **opt-in** per SDK — the Rust SDK does not downgrade (RPC is
   the only shipped transport); Python/TypeScript/Go/C# do.
2. Emits a single warning on stderr (`warning: Nexus RPC unreachable
   at <host>:<port>; falling back to HTTP at <host>:15474`).
3. Sets a flag on the client so subsequent calls do not retry RPC.

Users who want strict behaviour set `ClientConfig.transport` or
`NEXUS_SDK_TRANSPORT` explicitly; that disables the auto-downgrade.

## 6. Command map

SDKs expose idiomatic dotted names (e.g. `graph.cypher`,
`knn.search`, `db.list`). The transport layer resolves these into
wire-level commands before dispatch. The table below is the
canonical mapping every SDK MUST implement (methods without a row
here have no RPC verb — the SDK falls back to HTTP).

| SDK method (dotted)      | Wire command    | Argument encoding                         |
|--------------------------|-----------------|-------------------------------------------|
| `graph.cypher`           | `CYPHER`        | `[Str(query)]` or `[Str(query), Map(params)]` |
| `graph.ping`             | `PING`          | `[]` or `[Str(payload)]`                  |
| `graph.hello`            | `HELLO`         | `[Int(1)]` (protocol version)             |
| `graph.stats`            | `STATS`         | `[]`                                      |
| `graph.health`           | `HEALTH`        | `[]`                                      |
| `graph.quit`             | `QUIT`          | `[]`                                      |
| `auth.login`             | `AUTH`          | `[Str(api_key)]` or `[Str(user), Str(pass)]` |
| `node.create`            | `CREATE_NODE`   | `[Array<Str>(labels), Map(properties)]`   |
| `node.match`             | `MATCH_NODES`   | `[Array<Str>(labels), Map(filter)]`       |
| `node.update`            | `UPDATE_NODE`   | `[Int(id), Map(properties)]`              |
| `node.delete`            | `DELETE_NODE`   | `[Int(id)]`                               |
| `rel.create`             | `CREATE_REL`    | `[Int(src), Int(dst), Str(type), Map(properties)]` |
| `knn.search`             | `KNN_SEARCH`    | `[Str(label), Bytes(embedding), Int(k)]`  |
| `knn.traverse`           | `KNN_TRAVERSE`  | `[Array<Int>(seeds), Int(depth), Int(limit)]` |
| `ingest`                 | `INGEST`        | `[Array<Map>(nodes), Array<Map>(rels)]`   |
| `schema.labels`          | `LABELS`        | `[]`                                      |
| `schema.rel_types`       | `REL_TYPES`     | `[]`                                      |
| `schema.property_keys`   | `PROPERTY_KEYS` | `[]`                                      |
| `schema.indexes`         | `INDEXES`       | `[]`                                      |
| `db.list`                | `DB_LIST`       | `[]`                                      |
| `db.create`              | `DB_CREATE`     | `[Str(name)]`                             |
| `db.drop`                | `DB_DROP`       | `[Str(name)]`                             |
| `db.use`                 | `DB_USE`        | `[Str(name)]`                             |
| `data.export`            | `EXPORT`        | `[Str(format)]` or `[Str(format), Str(query)]` |
| `data.import`            | `IMPORT`        | `[Str(format), Str(payload)]`             |

Admin-level Cypher (`SHOW USERS`, `CREATE USER`, `CREATE DATABASE`,
`DROP DATABASE`, `SHOW API KEYS`, `CREATE API KEY`, `REVOKE API KEY`,
`SHOW QUERIES`, `TERMINATE QUERY`, `TERMINATE QUERIES`) is routed
through `CYPHER` — the server's RPC CYPHER dispatcher detects the
admin clauses and delegates to the same REST handlers, so SDKs do
not need a separate verb.

## 7. Wire-level value conversion

Every SDK maps its native JSON-like value type to `NexusValue`:

| NexusValue variant | Rust                  | Python            | TypeScript        | Go                | C#              |
|--------------------|-----------------------|-------------------|-------------------|-------------------|-----------------|
| `Null`             | `serde_json::Null`    | `None`            | `null`            | `nil`             | `null`          |
| `Bool(b)`          | `bool`                | `bool`            | `boolean`         | `bool`            | `bool`          |
| `Int(i)`           | `i64`                 | `int`             | `bigint`/`number` | `int64`           | `long`          |
| `Float(f)`         | `f64`                 | `float`           | `number`          | `float64`         | `double`        |
| `Str(s)`           | `String`              | `str`             | `string`          | `string`          | `string`        |
| `Bytes(b)`         | `Vec<u8>`             | `bytes`           | `Uint8Array`      | `[]byte`          | `byte[]`        |
| `Array(v)`         | `Vec<NexusValue>`     | `list`            | `Array`           | `[]interface{}`   | `List<object?>` |
| `Map(v)`           | `Vec<(NV, NV)>`       | `dict`            | `Map` / `Record`  | `map[...]`        | `Dictionary<,>` |

**Bytes are first-class.** KNN embeddings MUST be sent as
`NexusValue::Bytes` (little-endian f32) rather than
`Array<Float>` — the latter is accepted for backwards compatibility
but costs 4× the payload size.

## 8. Error semantics

Every transport surfaces three error classes:

1. **Transport errors** — connection refused, timeout, frame decode
   failed. Mapped to the SDK's `NetworkError` / equivalent.
2. **Protocol errors** — server returned a `Response::err(id, msg)`.
   Mapped to the SDK's `ApiError` with the full server message.
3. **Auth errors** — `WRONGPASS …` or `NOAUTH …` prefixes on the
   protocol-error message. SDKs SHOULD detect these by prefix and
   map to a dedicated `AuthError` type.

No transport MUST retry authentication errors automatically.

## 9. Coverage targets

The task that landed this contract set these per-SDK coverage floors
(as a percentage of public manager methods that go through a native
wire command, measured on the first release that ships RPC):

| SDK         | Target | Fallback transport                   |
|-------------|--------|--------------------------------------|
| Rust        | 100%   | No HTTP inside SDK (RPC-only)        |
| Python      |  95%   | HTTP REST client (existing)          |
| TypeScript  |  95%   | HTTP REST client (existing)          |
| Go          |  90%   | HTTP REST client (existing)          |
| C#          |  90%   | HTTP REST client (existing)          |
| n8n         |  85%   | HTTP (n8n native)                    |
| PHP         |  80%   | HTTP REST client (existing)          |

Anything below the floor is a release blocker.

## 10. Per-SDK notes

- **Rust SDK**: RPC-only. No HTTP transport shipped — the old
  `reqwest`-based client module remains for the `Http` variant but is
  routed through the same `Transport` trait so callers get a uniform
  API.
- **TypeScript SDK**: node builds ship RPC + RESP3 + HTTP; browser
  builds ship HTTP only because the browser cannot open raw TCP.
- **n8n node**: reuses the TypeScript SDK as a peer dependency. No
  independent wire-format implementation.
- **PHP SDK**: RPC is hand-written on top of the standard socket
  API; RESP3 uses `predis` which ships with RESP3 primitives.

## 11. Migration path

`phase2_sdk-rpc-transport-default` (this task) is the first release
that ships RPC as the default. Existing users see:

1. A single changelog entry per SDK calling out the default change.
2. A one-line opt-out: `NEXUS_SDK_TRANSPORT=http` or
   `ClientConfig { transport: TransportMode::Http, .. }`.
3. Automatic fallback to HTTP if the RPC port is unreachable within
   500 ms (for SDKs that enable auto-downgrade — see §5).

The migration is **non-breaking for users whose code does not depend
on the transport layer**. Any manager method signature, return type,
or exception/error contract is preserved byte-for-byte across the
transport change.
