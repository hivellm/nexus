# Proposal: phase1_nexus-resp3-compatibility

## Why

Shipping the native binary RPC (see `phase1_nexus-rpc-binary-protocol`) gets
us speed, but it is Nexus-specific — every SDK, every tool, every debug shell
has to learn our wire format. RESP3 solves a different problem: **any
RESP-compatible client, from `redis-cli` to `iredis` to `RedisInsight`, can
already speak to a RESP3 port**, which gives Nexus three things it does not
have today:

1. **A first-class debug shell.** `redis-cli -p 15476 CYPHER "MATCH (n) RETURN n"`
   beats `curl -X POST` with hand-crafted JSON headers for every ad-hoc query
   an operator ever needs. Synap's experience confirms this is the #1
   reason people kept the RESP3 listener on in production.
2. **Language parity for long-tail ecosystems.** Java, Ruby, PHP, and
   Elixir already have mature RESP clients. Shipping a thin command-map
   wrapper on top of Jedis/redis-rb/Redix gives Nexus SDKs for those
   languages for near-zero maintenance cost.
3. **Interop with existing infra.** Connection poolers (Twemproxy, envoy's
   `redis_proxy`), metric exporters (`redis_exporter`), and load balancers
   that speak RESP can front Nexus without code changes. For users already
   running Redis in the same stack, this dramatically lowers the barrier
   to adding a graph database.

We are **not** pretending to be Redis. RESP3 is used as a **transport
encoding** with a Nexus command vocabulary (`CYPHER`, `KNN`, `INGEST`,
`PING`, `AUTH`, `HELLO`, `DB_USE`…). Any attempt to `SET key value` against
the RESP3 port returns `-ERR unknown command 'SET' (Nexus is a graph DB,
see HELP)` — we do not emulate the KV semantics.

Synap has already validated the same approach end-to-end: hand-written
RESP3 parser in <500 LOC, inline-command support for `redis-cli` compat,
writer with RESP3 native types (Map/Set/Boolean/Double/Verbatim), and
full interop with any RESP2 client via `-1` null encoding. We reuse that
design verbatim.

## What Changes

Add `nexus-server::protocol::resp3` mirroring Synap's layout:

```
nexus-server/src/protocol/resp3/
|- mod.rs         # re-exports
|- parser.rs      # RESP3 parser (all 12 type prefixes + inline)
|- writer.rs      # RESP3 writer with BufWriter and byte counter
|- server.rs      # spawn_resp3_listener + handle_connection
|- command/
   |- mod.rs     # dispatch(state, args: &[Resp3Value]) -> Resp3Value
   |- cypher.rs  # CYPHER query, CYPHER_PARAMS for parameterised
   |- graph.rs   # CREATE_NODE / CREATE_REL / ...
   |- knn.rs     # KNN_SEARCH / KNN_TRAVERSE
   |- admin.rs   # PING / HELLO / AUTH / QUIT / HELP / COMMAND
```

Supported RESP3 type prefixes (parser + writer):

| Prefix | Type           | Notes                                    |
|--------|----------------|------------------------------------------|
| `+`    | SimpleString   | used for `OK`                            |
| `-`    | Error          | `-ERR <message>`, `-WRONGPASS`, `-NOAUTH`|
| `:`    | Integer        | node/rel ids, counts                     |
| `$`    | BulkString     | Cypher strings, binary-safe              |
| `*`    | Array          | result rows                              |
| `_`    | Null           | RESP3 native null                        |
| `,`    | Double         | float properties, KNN scores             |
| `#`    | Boolean        | `#t` / `#f`                              |
| `=`    | Verbatim       | Cypher error messages with `txt:` prefix |
| `~`    | Set            | `DISTINCT` results                       |
| `%`    | Map            | node properties, response envelopes      |
| `|`    | Attribute      | parsed and discarded on input            |
| `(`    | BigNumber      | out-of-range integers                    |

Key Nexus RESP3 commands (full list in `tasks.md`):

```
HELLO [2|3] [AUTH <user> <pass>]
AUTH <api-key>   |   AUTH <user> <pass>
PING
QUIT
HELP

CYPHER <query>                                    -> Map{columns, rows, stats, time_ms}
CYPHER.WITH <query> <param-json>                  -> Map
CYPHER.EXPLAIN <query>                            -> BulkString (plan)

NODE.CREATE <labels-csv> <props-json>             -> Integer (id)
NODE.GET <id>                                     -> Map
NODE.UPDATE <id> <props-json>                     -> Map
NODE.DELETE <id> [DETACH]                         -> Integer (deleted count)
NODE.MATCH <label> <props-json> [LIMIT <n>]       -> Array of Maps

REL.CREATE <src> <dst> <type> <props-json>        -> Integer (id)
REL.GET <id>                                      -> Map
REL.DELETE <id>                                   -> Integer

KNN.SEARCH <label> <vector> <k> [FILTER <json>]   -> Array of {id, score, props}
KNN.TRAVERSE <seeds-csv> <depth>                  -> Array of ids

INGEST.NODES <ndjson-bulk>                        -> Map{created, errors}
INGEST.RELS <ndjson-bulk>                         -> Map

INDEX.CREATE <label> <property> [UNIQUE]          -> +OK
INDEX.DROP <label> <property>                     -> +OK
INDEX.LIST                                        -> Array

DB.LIST                                           -> Array
DB.CREATE <name>                                  -> +OK
DB.DROP <name>                                    -> +OK
DB.USE <name>                                     -> +OK

LABELS                                            -> Array
REL_TYPES                                         -> Array
PROPERTY_KEYS                                     -> Array
STATS                                             -> Map
HEALTH                                            -> +OK | -ERR
COMMAND                                           -> Array of command specs
```

Config additions to `nexus-server/src/config.rs`:

```toml
[resp3]
enabled = true
host    = "127.0.0.1"        # loopback by default for safety
port    = 15476              # RPC is 15475, RESP3 is +1
```

Authentication: the existing `AuthMiddleware` already owns the API key
manager. RESP3 delegates to it via `check_auth(state, password)` — same
function stub as in Synap, wired to the real manager. `HELLO 3 AUTH <user>
<pass>` sets `authenticated = true` on the connection; every non-AUTH
command before that returns `-NOAUTH Authentication required.`.

Metrics (same shape as RPC):

- `nexus_resp3_connections` (gauge)
- `nexus_resp3_commands_total{command, status}` (counter)
- `nexus_resp3_command_duration_seconds{command}` (histogram)
- `nexus_resp3_bytes_read_total` / `..._written_total` (counters)

## Impact

- **Affected specs**: new `/docs/specs/resp3-nexus-commands.md` listing every
  command, its argument shape, and its response encoding. Update
  `/docs/specs/api-protocols.md` with a RESP3 section.
- **Affected code**:
  - NEW: `nexus-server/src/protocol/resp3/` (~1200 LOC, 7 files)
  - MODIFIED: `nexus-server/src/main.rs` (spawn resp3 listener)
  - MODIFIED: `nexus-server/src/config.rs` (+ `Resp3Config`)
  - MODIFIED: `nexus-core/src/metrics.rs` (RESP3 counters)
- **Breaking change**: NO — all surfaces (HTTP, RPC, MCP, UMICP) remain
  untouched. RESP3 is an additive opt-in port.
- **User benefit**:
  - `redis-cli -p 15476 HELLO 3` works out of the box for quick debugging.
  - Mature RESP clients in Java/Ruby/PHP become trivially usable (a thin
    command-map wrapper, no bespoke transport).
  - Ecosystem tooling (connection poolers, exporters) lights up for free.

## Non-goals

- **Not a Redis drop-in.** No `SET`/`GET`/`HSET`/... commands on the RESP3
  port. Attempting them returns `-ERR unknown command`.
- **Not a replacement for RPC.** For SDKs the binary RPC stays the default
  (40% smaller, 2x faster than RESP3 in Synap's benchmarks). RESP3 is for
  tooling and long-tail languages.
- **Not V1's push channel.** RESP3 push frames (`>` prefix) are stubbed for
  V2 alongside streaming Cypher; V1 responses are all request/reply.

## Reference

Synap implementation (already validated):

- `synap-server/src/protocol/resp3/parser.rs` — parser (full RESP3 grammar)
- `synap-server/src/protocol/resp3/writer.rs` — writer with byte counter
- `synap-server/src/protocol/resp3/server.rs` — accept loop + HELLO/AUTH/QUIT
- `synap-server/src/protocol/resp3/command/` — dispatch layout
