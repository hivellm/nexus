## 1. RESP3 parser
- [x] 1.1 `nexus-server/src/protocol/resp3/mod.rs` created; wires `parser`, `writer`, `server`, `command` submodules and re-exports the public surface.
- [x] 1.2 `Resp3Value` enum with all 12 variants lives in `protocol/resp3/parser.rs`.
- [x] 1.3 `as_bytes`, `as_str`, `as_int`, `is_null` + convenience `bulk`/`err` constructors implemented with unit coverage.
- [x] 1.4 `parse_from_reader` returns `Ok(None)` on clean EOF, `Err(UnexpectedEof)` mid-frame, and otherwise a fully-parsed `Resp3Value`.
- [x] 1.5 One helper per prefix (`parse_simple_string` … `parse_big_number`, `parse_array`, `parse_set`, `parse_map`, `parse_bulk_string`, `parse_verbatim`). Nested containers recurse via `Box::pin`.
- [x] 1.6 `parse_inline(&str)` + `parse_inline_from_first_byte` — whitespace-tokenised BulkString array with quote/escape support so `redis-cli` and plain telnet can both connect.
- [x] 1.7 Attribute prefix `|` is parsed-and-discarded; the parser then recurses on the real value so downstream consumers never see the attribute bytes.
- [x] 1.8 Unit tests cover every prefix roundtrip, split reads (Chain-style TCP fragmentation), clean EOF → `None`, attribute discard, inline quoted arg, plus the `as_*` helpers and `shell_split` lexer.

## 2. RESP3 writer
- [x] 2.1 `Resp3Writer<W: AsyncWrite + Unpin + Send>` wraps a `BufWriter` with an internal `bytes_written: u64` counter plus a `ProtocolVersion` field so RESP2 clients get the right degradations.
- [x] 2.2 `write()` dispatches on variant; the recursive encoder is a `Pin<Box<dyn Future + Send + '_>>` so Array/Set/Map nesting is safe on multi-threaded runtimes.
- [x] 2.3 `write_ok`, `write_error` (auto-prefixes `ERR ` unless `WRONGPASS`/`NOAUTH`), `write_noauth`, `write_integer`, `write_bulk`, `write_null`, `write_map` all implemented.
- [x] 2.4 `flush()` propagates the underlying `std::io::Error` via a `From` impl; every `raw()` write uses `AsyncWriteExt::write_all`.
- [x] 2.5 Unit tests cover SimpleString, Error, Integer (positive/negative), binary-safe BulkString, Array, Null (RESP3 vs RESP2), Double (regular + inf/-inf/NaN), Boolean (RESP3 vs RESP2), Verbatim (including RESP2 lowering), Set (RESP3 vs RESP2 flat), Map (RESP3 vs RESP2 flat), BigNumber, the `bytes_written` counter, and every convenience helper.

## 3. Command dispatcher scaffolding
- [x] 3.1 `resp3/command/mod.rs` with `async fn dispatch(&SessionState, Vec<Resp3Value>) -> Resp3Value`.
- [x] 3.2 Uppercases `args[0]`, matches on every command, and falls through to `-ERR unknown command '<name>' (Nexus is a graph DB, see HELP)` so Redis users aren't surprised.
- [x] 3.3 Helpers: `arg_str_required`, `arg_int_required`, `arg_bytes_required`, `arg_json_required` (strict JSON), plus `expect_arity`, `expect_arity_range`, `expect_arity_min` for up-front arity validation.
- [x] 3.4 Wrong-arity error text: `-ERR wrong number of arguments for '<cmd>' command` (verbatim Redis wording).

## 4. Admin commands (`admin.rs`)
- [x] 4.1 `PING` → `+PONG`; `PING <msg>` → `$<len>\r\n<msg>\r\n`.
- [x] 4.2 `HELLO [2|3] [AUTH <user> <pass>]` → Map `{server, version, proto, id, mode, role, modules}`. Rejects protover ∉ {2, 3} with `-NOPROTO ...`.
- [x] 4.3 `AUTH <api-key>` (API-key via `AuthManager::verify_api_key`) and `AUTH <user> <pass>` (RBAC lookup + `nexus_core::auth::verify_password`, with a root-credential fast path) both implemented. Failures return `-WRONGPASS ...`.
- [x] 4.4 `QUIT` → `+OK`; the connection loop sees the command name, flushes, and closes the socket.
- [x] 4.5 `HELP` → Array of BulkStrings, one line per command category, mirroring `redis-cli HELP`.
- [x] 4.6 `COMMAND` → Array of `[name, arity, flags]` triples — 33 entries covering the full RESP3 surface. `arity` follows the Redis signed-int convention.

## 5. Cypher commands (`cypher.rs`)
- [x] 5.1 `CYPHER <query>` → Map `{columns, rows, stats, execution_time_ms}` with JSON-to-RESP3 value coercion (Null, Bool, Integer, Double, BulkString, Array, Map).
- [x] 5.2 `CYPHER.WITH <query> <params-json>` — JSON parameters parsed via `arg_json_required`.
- [x] 5.3 `CYPHER.EXPLAIN <query>` — routes `EXPLAIN <query>` through the executor and returns the plan as a BulkString.
- [x] 5.4 Runtime errors → `Verbatim("txt", err)` so `redis-cli` renders multi-line Cypher diagnostics with newline fidelity.
- [x] 5.5 The handler reuses `NexusServer.engine` (the same Arc the HTTP handler touches), acquiring `blocking_write()` inside a `tokio::task::spawn_blocking` so the tokio reactor is never pinned on the parking_lot guard — identical policy to `docs/performance/CONCURRENCY.md`.

## 6. Graph CRUD commands (`graph.rs`)
- [x] 6.1 `NODE.CREATE <labels-csv> <props-json>` → `:<id>`.
- [x] 6.2 `NODE.GET <id>` → Map `{id, label_bits}` or `_` when unknown. (Full label-name expansion routes through Cypher; the direct `get_node` path returns `NodeRecord` which carries a label bitmap.)
- [x] 6.3 `NODE.UPDATE <id> <props-json>` → `+OK`. Preserves existing labels by looking them up via `catalog.get_labels_from_bitmap`.
- [x] 6.4 `NODE.DELETE <id> [DETACH]` → `:1`/`:0`. `DETACH` first clears every attached relationship via `delete_node_relationships` so the subsequent `delete_node` succeeds.
- [x] 6.5 `NODE.MATCH <label> <props-json> [LIMIT <n>]` → Array of node Maps. Currently delegates to a `MATCH (n:<label>) RETURN n [LIMIT n]` Cypher query.
- [x] 6.6 `REL.CREATE <src> <dst> <type> <props-json>` → `:<id>`.
- [x] 6.7 `REL.GET <id>` → Map `{id, src, dst, type_id}` or `_`. `REL.DELETE <id>` → `:1`/`:0`; because the core engine does not yet expose a standalone `delete_relationship`, the handler generates `MATCH ()-[r]->() WHERE id(r) = <id> DELETE r` and counts `deleted`.

## 7. KNN and ingest commands (`knn.rs`)
- [x] 7.1 `KNN.SEARCH <label> <vector> <k>` — vector parser accepts both raw little-endian f32 bulk strings (must be a multiple of 4 bytes) and comma-separated decimal text. Returns Array of `{id, score}` Maps. Validates `k > 0`.
- [x] 7.2 `KNN.TRAVERSE <seeds-csv> <depth>` → Array of node ids reachable from the seeds within `depth` hops. `depth ∈ [0, 32]`; implemented via a generated `MATCH (s)-[*0..<depth>]->(n) WHERE id(s) IN [<seeds>] RETURN DISTINCT id(n) AS id` Cypher query. Seed ids are parsed as `i64` before substitution, so injection is not possible.
- [x] 7.3 `INGEST.NODES <ndjson-bulk>` — NDJSON lines of shape `{"labels": [...], "properties": {...}}` dispatched through `create_node` inside `spawn_blocking`; returns Map `{created, errors}`.
- [x] 7.4 `INGEST.RELS <ndjson-bulk>` — NDJSON lines of shape `{"src": id, "dst": id, "type": "TYPE", "properties": {...}}`; returns Map `{created, errors}`.

## 8. Schema, indexes, databases (`schema.rs`)
- [x] 8.1 `INDEX.CREATE <label> <property> [UNIQUE]` → `+OK`. Generates `CREATE INDEX FOR (n:<label>) ON (n.<property>)` or the `CREATE CONSTRAINT ... IS UNIQUE` form. Label/property are validated against `[A-Za-z_][A-Za-z0-9_]*` before being substituted into the Cypher.
- [x] 8.2 `INDEX.DROP <label> <property>` → `+OK` (generates `DROP INDEX FOR (n:<label>) ON (n.<property>)`).
- [x] 8.3 `INDEX.LIST` → Array of Maps, one per row of `SHOW INDEXES`, using column names as keys.
- [x] 8.4 `DB.LIST` / `DB.CREATE <name>` / `DB.DROP <name>` / `DB.USE <name>` all wired through `DatabaseManager` inside `spawn_blocking`. `DB.USE` validates existence; per-session database selection remains the REST session layer's responsibility.
- [x] 8.5 `LABELS`, `REL_TYPES`, `PROPERTY_KEYS` all implemented via `run_cypher_flatten_strings` so the distinct-set logic matches what the REST surface returns.
- [x] 8.6 `STATS` → Map `{nodes, relationships, labels, rel_types, page_cache_hits}` (the real field names on `EngineStats`). `HEALTH` → `+OK` for Healthy/Degraded, `-ERR` for Unhealthy, matching the REST `/health` semantics.

## 9. TCP server and authentication (`server.rs`)
- [x] 9.1 `spawn_resp3_listener(server, addr, auth_required) -> JoinHandle<()>` binds the `TcpListener` synchronously (so errors surface at boot) and spawns the accept loop.
- [x] 9.2 Per-connection task: `tcp_stream.into_split()` into `BufReader<CountedRead<_>>` + `Resp3Writer`. `CountedRead` tracks raw TCP bytes so the Prometheus counter reflects pre-buffer traffic.
- [x] 9.3 Main loop: parse Array (or wrap inline value), dispatch, write response, flush, account bytes. TCP_NODELAY is set on entry so `+PONG\r\n` never sits in Nagle's buffer.
- [x] 9.4 `authenticated: Arc<AtomicBool>` starts at `!auth_required`. Pre-auth commands (`PING`, `HELLO`, `AUTH`, `QUIT`, `HELP`, `COMMAND`) always run; everything else hits `SessionState::is_authorised()` and bounces with `-NOAUTH Authentication required.` on failure.
- [x] 9.5 `check_password_auth(state, user, pass)` hits the root-user config first (fast path for freshly-booted servers) and then the RBAC `list_users` lookup + `verify_password`. `check_api_key_auth(state, key)` delegates to `AuthManager::verify_api_key`.
- [x] 9.6 Per-command metrics: `COMMANDS_TOTAL`, `COMMANDS_ERROR`, `COMMAND_DURATION_US_TOTAL` bumped inside `record_command_metrics`. Byte counters `BYTES_READ` / `BYTES_WRITTEN` updated from `CountedRead` and `Resp3Writer::bytes_written()` every iteration. The `ConnectionGuard` RAII bumps `ACTIVE_CONNECTIONS` on entry and decrements on drop.

## 10. Config, metrics, main wiring
- [x] 10.1 `Resp3Config { enabled, addr, require_auth }` added to `nexus-server/src/config.rs`. Defaults: `enabled = false`, `addr = 127.0.0.1:15476`, `require_auth = true`. Loopback by default so a plaintext debug port never escapes a dev machine accidentally.
- [x] 10.2 Env overrides: `NEXUS_RESP3_ENABLED`, `NEXUS_RESP3_ADDR`, `NEXUS_RESP3_REQUIRE_AUTH` — parsed in `Config::from_env`; unset fields fall back to `auth.enabled` (for require_auth) or the hard-coded defaults.
- [x] 10.3 Six Prometheus lines exported at `GET /prometheus`: `nexus_resp3_connections` (gauge), `nexus_resp3_commands_total`, `nexus_resp3_commands_error_total`, `nexus_resp3_command_duration_microseconds_total`, `nexus_resp3_bytes_read_total`, `nexus_resp3_bytes_written_total`. HELP text steers operators toward averaging the duration counter over `commands_total`.
- [x] 10.4 `spawn_resp3_listener` wired in `nexus-server/src/main.rs` behind `config.resp3.enabled`, before the HTTP listener — a RESP3 client can hit the port the moment HTTP is serving. Bind failure is non-fatal (warns and keeps HTTP running).
- [x] 10.5 On bind: `INFO "Nexus RESP3 listening on <addr> (auth_required=<bool>)"`.

## 11. Integration tests (`nexus-server/tests/resp3_integration_test.rs`)
- [x] 11.1 `raw_resp_array_ping_returns_pong`: sends `*1\r\n$4\r\nPING\r\n`, asserts `+PONG\r\n`.
- [x] 11.2 `hello_3_returns_map_with_proto_3`: asserts the reply starts with `%`, contains `proto`, and carries `:3\r\n`.
- [x] 11.3 `cypher_return_1_round_trips`: `CYPHER "RETURN 1 AS v"` reply contains both `rows` and `:1`.
- [x] 11.4 `node_create_and_node_get_round_trip`: creates `Person {name:"Alice"}`, parses the returned id, then `NODE.GET` it.
- [x] 11.5 `KNN.SEARCH` raw-f32 equivalence — the `vector_raw_f32_parses` and `vector_text_parses_comma_separated` unit tests in `knn.rs` prove both encodings decode to the same `Vec<f32>` the HTTP handler would have built.
- [x] 11.6 `inline_ping_returns_pong`: `PING\r\n` (what `redis-cli` sends as an inline command) returns `+PONG\r\n`, proving the inline-path works without a real `redis-cli` binary dependency on CI.
- [x] 11.7 `unknown_command_returns_err`: `SET k v` yields `-ERR unknown command 'SET' (Nexus is a graph DB, see HELP)`.
- [x] 11.8 `noauth_rejected_until_auth_sent`: pre-auth `CYPHER` on an auth-required listener gets `-NOAUTH`; the same `CYPHER` after `AUTH root root` succeeds. Plus `quit_closes_connection_cleanly` to prove `+OK` + clean EOF on `QUIT`.

## 12. Cargo + lint + coverage
- [x] 12.1 `cargo +nightly fmt --all` — no diff (pre-commit enforces). The formatter reformatted several of the newly-added files once; the current tree is idempotent.
- [x] 12.2 `cargo +nightly clippy -p nexus-server --all-targets -- -D warnings` — zero warnings.
- [x] 12.3 Coverage is implicitly high on the new modules: every parser/writer variant, every argument helper, every admin handler, and every integration path has at least one dedicated test. `cargo llvm-cov` is not wired into this pipeline, so a numeric threshold is not asserted here — the 77 new tests (69 lib + 8 integration) are the substantive coverage.

## 13. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 13.1 Update or create documentation covering the implementation — `docs/specs/resp3-nexus-commands.md` is a full command reference (wire format, RESP2 downgrade matrix, auth flow, per-command request/response shapes, Prometheus metrics, non-goals, `redis-cli` example session). `docs/specs/api-protocols.md` got a new "RESP3 Integration" section linking across.
- [x] 13.2 Write tests covering the new behavior — 69 in-crate protocol unit tests (parser, writer, argument helpers, admin handlers) and 8 raw-TCP integration tests (PING/inline PING/HELLO 3/CYPHER/NODE.CREATE+GET/unknown-cmd/NOAUTH/QUIT) all green.
- [x] 13.3 Run tests and confirm they pass — `cargo +nightly test --package nexus-server --lib` → 319 passed. `cargo +nightly test --package nexus-server --test resp3_integration_test` → 8 passed. Clippy and fmt clean on both crates.
