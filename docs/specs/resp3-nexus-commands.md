# RESP3 — Nexus Command Reference

Nexus ships an additive RESP3 listener so any RESP3 client (`redis-cli`,
`iredis`, RedisInsight, Jedis, redis-rb, Redix, ...) can talk to the graph
database with a Nexus command vocabulary. **This is a transport encoding,
not Redis emulation**: `SET key value` returns
`-ERR unknown command 'SET' (Nexus is a graph DB, see HELP)`.

Source: [`nexus-server/src/protocol/resp3/`](../../nexus-server/src/protocol/resp3).

## Configuration

```toml
# config.yml (partial)
[resp3]
enabled      = true           # default: false
addr         = "127.0.0.1:15476"  # loopback by default for safety
require_auth = true           # default: inherits from [auth].enabled
```

Environment-variable overrides:

| Var | Meaning |
|---|---|
| `NEXUS_RESP3_ENABLED` | `true` / `false` |
| `NEXUS_RESP3_ADDR` | `host:port` |
| `NEXUS_RESP3_REQUIRE_AUTH` | `true` / `false` |

## Wire format

RESP3 spec: <https://github.com/antirez/RESP3/blob/master/spec.md>. All 12
type prefixes are supported on both parse and write. The writer also
downgrades RESP3-only types to RESP2 equivalents when the client
negotiates `HELLO 2`:

| Prefix | RESP3 type | RESP2 degradation |
|---|---|---|
| `+` | SimpleString | — |
| `-` | Error | — |
| `:` | Integer | — |
| `$` | BulkString | — |
| `*` | Array | — |
| `_` | Null | `$-1\r\n` |
| `,` | Double | BulkString |
| `#` | Boolean | `:0` / `:1` |
| `=` | Verbatim | BulkString (drops format tag) |
| `~` | Set | Flat Array |
| `%` | Map | Flat Array (k1 v1 k2 v2 ...) |
| `|` | Attribute | Parsed and discarded |
| `(` | BigNumber | BulkString |

Inline commands (any line whose first byte isn't a RESP3 prefix) are
whitespace-split and treated as an Array of BulkStrings — what
`redis-cli` and `telnet` send.

## Authentication

- `HELLO 3 AUTH <user> <pass>` — negotiate protocol 3 and authenticate
  in a single round-trip.
- `AUTH <user> <pass>` — RBAC username/password (uses `verify_password`
  internally). A successful root-user match (against the configured
  `root_user` credentials) also works, matching the REST login path.
- `AUTH <api-key>` — single-argument form verifies a `nx_...` API key.

Pre-auth commands that never require authentication:
`PING`, `HELLO`, `AUTH`, `QUIT`, `HELP`, `COMMAND`.

Every other command checks `SessionState::is_authorised()`; when the
listener was configured with `require_auth = true` and the client hasn't
authenticated yet, the reply is `-NOAUTH Authentication required.`.

## Commands

### Admin

| Command | Reply |
|---|---|
| `PING [msg]` | `+PONG` (or `$<len>\r\n<msg>\r\n` when a message is supplied) |
| `HELLO [2\|3] [AUTH user pass]` | `%7` Map with `server`, `version`, `proto`, `id`, `mode`, `role`, `modules`. |
| `AUTH <api-key>` / `AUTH <user> <pass>` | `+OK` or `-WRONGPASS ...`. |
| `QUIT` | `+OK` then the server closes the socket. |
| `HELP` | `*N` Array of one-BulkString-per-line human-readable help. |
| `COMMAND` | `*N` Array of `[name, arity, flags]` triples. |

`arity` follows the Redis convention — positive = exact count (command
name included), negative = minimum count.

### Cypher

| Command | Reply |
|---|---|
| `CYPHER <query>` | `%4` Map with `columns`, `rows`, `stats`, `execution_time_ms`. |
| `CYPHER.WITH <query> <params-json>` | Same shape. `params-json` is a JSON object. |
| `CYPHER.EXPLAIN <query>` | `$<plan>` — planner output as a BulkString (or `=txt:...` for error text). |

The handler runs `Engine::execute_cypher` inside `tokio::task::spawn_blocking`
so the tokio reactor is never pinned on the engine's `parking_lot::RwLock`
(see `docs/performance/CONCURRENCY.md`).

### Graph CRUD

| Command | Reply |
|---|---|
| `NODE.CREATE <labels-csv> <props-json>` | `:<id>` |
| `NODE.GET <id>` | `%...` Map `{id, label_bits}` or `_` for unknown id. |
| `NODE.UPDATE <id> <props-json>` | `+OK` (labels preserved). |
| `NODE.DELETE <id> [DETACH]` | `:1` on success, `:0` if not found. `DETACH` first clears every relationship of the node. |
| `NODE.MATCH <label> <props-json> [LIMIT <n>]` | `*N` Array of node Maps (currently sourced via a `MATCH (n:<label>) RETURN n [LIMIT n]` Cypher query). |
| `REL.CREATE <src> <dst> <type> <props-json>` | `:<id>` |
| `REL.GET <id>` | `%4` Map `{id, src, dst, type_id}` or `_`. |
| `REL.DELETE <id>` | `:1` on success, `:0` otherwise. Implemented via a generated `MATCH ()-[r]->() WHERE id(r) = <id> DELETE r` Cypher statement because the core engine does not yet expose a standalone `delete_relationship` API. |

### KNN / ingest

| Command | Reply |
|---|---|
| `KNN.SEARCH <label> <vector> <k>` | `*N` Array of `{id, score}` Maps. `<vector>` is either a BulkString of raw f32 little-endian bytes OR a comma-separated decimal text. `k > 0`. |
| `KNN.TRAVERSE <seeds-csv> <depth>` | `*N` Array of node ids reachable from the seeds within `depth` hops (inclusive). `depth` in `[0, 32]`. Implemented via a Cypher variable-length expansion, so filter pushdown rides on the planner's own rules. |
| `INGEST.NODES <ndjson-bulk>` | `%2` Map `{created, errors}`. Each line is `{"labels": [...], "properties": {...}}`. |
| `INGEST.RELS <ndjson-bulk>` | `%2` Map `{created, errors}`. Each line is `{"src": id, "dst": id, "type": "TYPE", "properties": {...}}`. |

### Schema, indexes, databases

| Command | Reply |
|---|---|
| `INDEX.CREATE <label> <property> [UNIQUE]` | `+OK`. With `UNIQUE`, emits `CREATE CONSTRAINT ... IS UNIQUE`; otherwise `CREATE INDEX`. |
| `INDEX.DROP <label> <property>` | `+OK` |
| `INDEX.LIST` | `*N` Array of Maps (one per Cypher `SHOW INDEXES` row). |
| `DB.LIST` | `*N` Array of database-name BulkStrings. |
| `DB.CREATE <name>` | `+OK` |
| `DB.DROP <name>` | `+OK` |
| `DB.USE <name>` | `+OK` iff the database exists (session selection via the REST session layer is tracked separately). |
| `LABELS` | `*N` distinct node labels. |
| `REL_TYPES` | `*N` distinct relationship types. |
| `PROPERTY_KEYS` | `*N` distinct property keys. |
| `STATS` | `%5` Map of engine counters (`nodes`, `relationships`, `labels`, `rel_types`, `page_cache_hits`). |
| `HEALTH` | `+OK` when the engine reports healthy or degraded, `-ERR` otherwise. |

## Metrics

RESP3 traffic is surfaced at `GET /prometheus` alongside HTTP traffic:

- `nexus_resp3_connections` — gauge of currently-live connections.
- `nexus_resp3_commands_total` — counter of dispatched commands.
- `nexus_resp3_commands_error_total` — subset that returned `-ERR` / `-NOAUTH`.
- `nexus_resp3_command_duration_microseconds_total` — counter, divide by
  `nexus_resp3_commands_total` for an average wall-clock.
- `nexus_resp3_bytes_read_total` / `nexus_resp3_bytes_written_total` — raw
  TCP byte counters.

## Non-goals

- **Not a Redis drop-in.** No `SET`/`GET`/`HSET`/... The KV semantics are
  absent on purpose.
- **Not a replacement for the binary RPC** (see
  `phase1_nexus-rpc-binary-protocol`). RESP3 is optimised for tooling and
  long-tail language support; the binary RPC stays the default for SDKs.
- **Not V1's push channel.** RESP3 push frames (`>` prefix) are reserved
  for V2 alongside streaming Cypher; V1 responses are all request/reply.

## Example: `redis-cli`

```shell
$ redis-cli -p 15476
127.0.0.1:15476> HELLO 3 AUTH root root
...
127.0.0.1:15476> CYPHER "RETURN 1 AS v"
1) "columns"
   1) "v"
2) "rows"
   1) 1) (integer) 1
3) "stats"
   1) "rows"
   2) (integer) 1
4) "execution_time_ms"
   (integer) 1
127.0.0.1:15476> NODE.CREATE "Person" "{\"name\":\"Alice\"}"
(integer) 42
127.0.0.1:15476> STATS
 1) "nodes"
 2) (integer) 1
 ...
```

## Testing

- **Protocol unit tests** (`nexus-server/src/protocol/resp3/**/tests.rs`):
  parser roundtrips every prefix, writer encodes every variant in both
  RESP3 and RESP2, dispatcher argument helpers, admin commands.
- **Integration tests** (`nexus-server/tests/resp3_integration_test.rs`):
  raw TCP round-trips for `PING`, `HELLO 3`, `CYPHER RETURN 1 AS v`,
  `NODE.CREATE` + `NODE.GET`, `-ERR` on unknown commands, `-NOAUTH` flow,
  clean `QUIT` teardown.

Run either with:

```shell
cargo +nightly test --package nexus-server -- protocol::
cargo +nightly test --package nexus-server --test resp3_integration_test
```
