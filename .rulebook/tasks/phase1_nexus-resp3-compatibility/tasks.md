## 1. RESP3 parser
- [ ] 1.1 Create `nexus-server/src/protocol/resp3/mod.rs` (module declarations)
- [ ] 1.2 Define `Resp3Value` enum with all 12 variants (SimpleString, Error, Integer, Double, Boolean, BulkString, Null, Array, Set, Map, Verbatim, BigNumber)
- [ ] 1.3 Implement `Resp3Value::{as_bytes, as_str, as_int, is_null}` helpers
- [ ] 1.4 Implement `parse_from_reader(reader) -> Result<Option<Resp3Value>>` — returns `None` on clean EOF
- [ ] 1.5 Implement `parse_type(reader, prefix)` for each of the 12 prefixes, with `Box::pin` recursion for nested containers
- [ ] 1.6 Implement `parse_inline(&str)` for `redis-cli` and telnet compatibility (whitespace-tokenised BulkString array)
- [ ] 1.7 Attribute-prefix `|` is consumed and discarded, then parser continues to the real value
- [ ] 1.8 Unit tests: every prefix roundtrips; split reads (TCP fragmentation) are handled; EOF returns None

## 2. RESP3 writer
- [ ] 2.1 Create `Resp3Writer<W: AsyncWrite>` wrapping a `BufWriter` with an internal `bytes_written: u64` counter
- [ ] 2.2 Implement `write(&self, value: &Resp3Value)` dispatching on variant and emitting the correct prefix + CRLF framing
- [ ] 2.3 Convenience methods: `write_ok`, `write_error`, `write_noauth`, `write_integer`, `write_bulk`, `write_null`, `write_map`
- [ ] 2.4 `flush()` propagates the underlying error; all writes go through `write_all`
- [ ] 2.5 Unit tests: every `Resp3Value` variant encodes to bytes identical to the Redis reference encoding

## 3. Command dispatcher scaffolding
- [ ] 3.1 Create `resp3/command/mod.rs` with `dispatch(state, args: &[Resp3Value]) -> Resp3Value`
- [ ] 3.2 Route on uppercased `args[0]`; return `-ERR unknown command '<name>'` for unknowns
- [ ] 3.3 Argument helpers: `arg_str_required(args, idx, cmd)`, `arg_int_required`, `arg_bytes_required`, `arg_map_required`
- [ ] 3.4 Wrong-arity error text mirrors Redis: `-ERR wrong number of arguments for '<cmd>' command`

## 4. Admin commands
- [ ] 4.1 `PING` -> `+PONG`; `PING <msg>` -> `+<msg>`
- [ ] 4.2 `HELLO [2|3] [AUTH <user> <pass>]` -> Map{server:"nexus", version, proto, id, mode, role}
- [ ] 4.3 `AUTH <password>` OR `AUTH <username> <password>` -> `+OK` / `-WRONGPASS`
- [ ] 4.4 `QUIT` -> `+OK` then close the TCP connection cleanly
- [ ] 4.5 `HELP` -> Array of `BulkString` lines, one per command category
- [ ] 4.6 `COMMAND` -> Array of command specs (name, arity, flags)

## 5. Cypher commands
- [ ] 5.1 `CYPHER <query>` -> Map{columns: Array<Str>, rows: Array<Array<Resp3Value>>, stats: Map, execution_time_ms: Int}
- [ ] 5.2 `CYPHER.WITH <query> <params-json>` -> same, with parameters parsed from JSON arg
- [ ] 5.3 `CYPHER.EXPLAIN <query>` -> BulkString of the planner output
- [ ] 5.4 Map Cypher runtime errors to `Verbatim("txt", err)` so redis-cli renders them with newlines
- [ ] 5.5 Reuse the same `EXECUTOR` static already used by the HTTP handler — no duplication

## 6. Graph CRUD commands
- [ ] 6.1 `NODE.CREATE <labels-csv> <props-json>` -> Integer id
- [ ] 6.2 `NODE.GET <id>` -> Map{id, labels, props}
- [ ] 6.3 `NODE.UPDATE <id> <props-json>` -> Map (full node)
- [ ] 6.4 `NODE.DELETE <id> [DETACH]` -> Integer (1 on success)
- [ ] 6.5 `NODE.MATCH <label> <props-json> [LIMIT <n>]` -> Array of Maps
- [ ] 6.6 `REL.CREATE <src> <dst> <type> <props-json>` -> Integer id
- [ ] 6.7 `REL.GET <id>` -> Map; `REL.DELETE <id>` -> Integer

## 7. KNN and ingest commands
- [ ] 7.1 `KNN.SEARCH <label> <vector> <k> [FILTER <props-json>]` — vector is BulkString (raw f32 LE) OR comma-separated doubles
- [ ] 7.2 `KNN.TRAVERSE <seeds-csv> <depth> [FILTER <json>]` -> Array of node ids
- [ ] 7.3 `INGEST.NODES <ndjson-bulk>` — one JSON object per line inside the bulk string; returns Map{created, errors}
- [ ] 7.4 `INGEST.RELS <ndjson-bulk>` -> Map{created, errors}

## 8. Schema, indexes, databases
- [ ] 8.1 `INDEX.CREATE <label> <property> [UNIQUE]` -> `+OK`
- [ ] 8.2 `INDEX.DROP <label> <property>` -> `+OK`
- [ ] 8.3 `INDEX.LIST` -> Array of Maps
- [ ] 8.4 `DB.LIST` / `DB.CREATE <name>` / `DB.DROP <name>` / `DB.USE <name>`
- [ ] 8.5 `LABELS` / `REL_TYPES` / `PROPERTY_KEYS` -> Arrays of BulkString
- [ ] 8.6 `STATS` -> Map of counters (node_count, rel_count, ...); `HEALTH` -> `+OK` or `-ERR`

## 9. TCP server and authentication
- [ ] 9.1 Implement `spawn_resp3_listener(state, addr)` mirroring `synap_rpc::server::spawn`
- [ ] 9.2 Per-connection task wraps read half in `BufReader`, write half in `Resp3Writer`
- [ ] 9.3 Main loop: parse value, unwrap Array or inline-Array, dispatch, write response, flush
- [ ] 9.4 Enforce `authenticated` flag — reject non-AUTH commands with `-NOAUTH` when auth required
- [ ] 9.5 `check_auth(state, password)` delegates to `state.auth_middleware.user_manager.authenticate(...)`
- [ ] 9.6 Record `record_resp3_command(cmd, ok, elapsed)` and `resp3_bytes(read, written)` after every request

## 10. Config, metrics, main wiring
- [ ] 10.1 Add `Resp3Config { enabled, host, port }` to `nexus-server/src/config.rs`; default enabled, port 15476, loopback host
- [ ] 10.2 `NEXUS_RESP3_ADDR` env override
- [ ] 10.3 Register prometheus metrics: `nexus_resp3_connections`, `nexus_resp3_commands_total`, `nexus_resp3_command_duration_seconds`, `nexus_resp3_bytes_read_total`, `nexus_resp3_bytes_written_total`
- [ ] 10.4 Wire listener spawn in `nexus-server/src/main.rs` behind `config.resp3.enabled`
- [ ] 10.5 Log `"Nexus RESP3 listening on {addr}"` at INFO

## 11. Integration tests
- [ ] 11.1 Boot server, connect via raw TCP, send `*1\r\n$4\r\nPING\r\n`, expect `+PONG\r\n`
- [ ] 11.2 `HELLO 3` returns a Map with `proto: 3`
- [ ] 11.3 `CYPHER "RETURN 1 AS v"` returns Map with `rows: [[1]]`
- [ ] 11.4 `NODE.CREATE "Person" '{"name":"Alice"}'` returns an integer id, then `NODE.GET <id>` returns the node
- [ ] 11.5 `KNN.SEARCH` with a raw-f32 bulk string returns the same result as the HTTP endpoint
- [ ] 11.6 Using `redis-cli -p 15476 PING` (if available on CI) returns `PONG`
- [ ] 11.7 Unknown command returns `-ERR unknown command 'SET'`
- [ ] 11.8 Unauthenticated access (with auth enabled) returns `-NOAUTH`

## 12. Cargo + lint + coverage
- [ ] 12.1 `cargo +nightly fmt --all` clean
- [ ] 12.2 `cargo clippy --workspace -- -D warnings` clean
- [ ] 12.3 `cargo llvm-cov --package nexus-server --ignore-filename-regex 'examples'` >= 95% on new files

## 13. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 13.1 Update or create documentation covering the implementation (`docs/specs/resp3-nexus-commands.md`, update `docs/specs/api-protocols.md`)
- [ ] 13.2 Write tests covering the new behavior (min 40 tests covering parser, writer, every command, auth, error paths)
- [ ] 13.3 Run tests and confirm they pass
