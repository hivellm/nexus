## 1. Wire types and codec
- [x] 1.1 Create `nexus-server/src/protocol/mod.rs` with `pub mod envelope; pub mod rpc;`
- [x] 1.2 Define `NexusValue`, `Request`, `Response` in `rpc/types.rs` (mirror `synap_rpc/types.rs`)
- [x] 1.3 Add `From` impls for String, &str, i64, bool, Vec<u8>, f64 on `NexusValue`
- [x] 1.4 Add `as_str` / `as_bytes` / `as_int` / `as_float` / `is_null` helpers on `NexusValue`
- [x] 1.5 Implement `encode_frame` / `decode_frame` in `rpc/codec.rs` (4-byte LE len prefix + rmp-serde body)
- [x] 1.6 Implement async `read_request` / `read_response` / `write_request` / `write_response` helpers
- [x] 1.7 Unit tests: roundtrip all `NexusValue` variants through rmp-serde
- [x] 1.8 Unit tests: partial-header and partial-body decode returns `Ok(None)`
- [x] 1.9 Unit tests: frames exceeding `max_frame_bytes` reject with a specific error

## 2. Dispatcher scaffolding
- [x] 2.1 Create `rpc/dispatch/mod.rs` with `dispatch(state, req) -> Response` and sub-modules
- [x] 2.2 Implement argument helpers: `arg_str`, `arg_bytes`, `arg_int`, `arg_float`, `arg_map`, `arg_array`
- [x] 2.3 Top-level `run(state, cmd, args)` matches on uppercased command and routes to sub-modules

## 3. Cypher dispatch
- [ ] 3.1 Implement `CYPHER` command in `dispatch/cypher.rs`: args = `[query: Str, params: Map?]`
- [ ] 3.2 Reuse the global `EXECUTOR` from `api::cypher::execute` (no duplication of planning logic)
- [ ] 3.3 Encode Cypher result as `Map{columns: Array<Str>, rows: Array<Array<NexusValue>>, stats: Map, execution_time_ms: Int}`
- [ ] 3.4 Map executor errors to `Response::err(id, formatted_message)` preserving the HTTP error text
- [ ] 3.5 Unit tests: `CYPHER "RETURN 1"` returns `rows = [[1]]` via RPC

## 4. Graph CRUD dispatch
- [ ] 4.1 `CREATE_NODE` — args `[labels: Array<Str>, props: Map]`, returns node id
- [ ] 4.2 `CREATE_REL` — args `[src: Int, dst: Int, type: Str, props: Map]`, returns rel id
- [ ] 4.3 `UPDATE_NODE` — args `[id: Int, props: Map]`, returns node
- [ ] 4.4 `DELETE_NODE` — args `[id: Int, detach: Bool]`, returns success
- [ ] 4.5 `MATCH_NODES` — args `[label: Str, props: Map, limit: Int]`, returns rows
- [ ] 4.6 Unit tests: full CRUD round-trip against an in-memory engine

## 5. KNN dispatch
- [ ] 5.1 `KNN_SEARCH` — args `[label: Str, embedding: Bytes|Array<Float>, k: Int, filter: Map?]`
- [ ] 5.2 `KNN_TRAVERSE` — args `[seeds: Array<Int>, depth: Int, filter: Map?]`
- [ ] 5.3 Accept embedding as `Bytes` (raw f32 little-endian) OR `Array<Float>` for language parity
- [ ] 5.4 Unit tests: embedding as Bytes decodes to `Vec<f32>` identically to Array<Float>

## 6. Ingest, schema, database, admin
- [ ] 6.1 `INGEST` — args `[nodes: Array<Map>, rels: Array<Map>]`, returns per-batch stats
- [ ] 6.2 `LABELS` / `REL_TYPES` / `PROPERTY_KEYS` / `INDEXES` — listing commands
- [ ] 6.3 `DB_LIST` / `DB_CREATE` / `DB_DROP` / `DB_USE` — multi-database management
- [x] 6.4 `PING` returns `"PONG"`, `HELLO` returns `{server: "nexus", version, proto: 1}`
- [x] 6.5 `AUTH` — args `[api_key: Str]` OR `[username: Str, password: Str]`; sets per-connection `authenticated = true`
- [ ] 6.6 `STATS` / `HEALTH` — read-only observability commands
- [x] 6.7 Reject unauthenticated commands when `auth.required && !authenticated` (except `PING`/`HELLO`/`AUTH`/`QUIT`)

## 7. TCP server and accept loop
- [ ] 7.1 Implement `spawn_rpc_listener(state, addr)` in `rpc/server.rs` (copy Synap's shape)
- [ ] 7.2 Per-connection: split into `(read_half, write_half)`, spawn a writer task behind an mpsc channel
- [ ] 7.3 Per-request: `tokio::spawn` a task that calls `dispatch` and sends `(Response, cmd, elapsed, in_bytes)` to the writer
- [ ] 7.4 Cap in-flight-per-connection via semaphore (`max_in_flight_per_conn`); excess requests wait
- [ ] 7.5 Handle clean EOF without logging noise (`UnexpectedEof` is expected on close)
- [ ] 7.6 Reserve `id = u32::MAX` for server-initiated push frames; document in rustdoc

## 8. Config and metrics
- [ ] 8.1 Add `RpcConfig { enabled, host, port, max_frame_bytes, max_in_flight_per_conn }` to `nexus-server/src/config.rs`
- [ ] 8.2 Default `enabled = true`, `host = "0.0.0.0"`, `port = 15475`
- [ ] 8.3 Support `NEXUS_RPC_ADDR` env override for ops parity with `NEXUS_ADDR`
- [ ] 8.4 Register prometheus metrics: `nexus_rpc_connections`, `nexus_rpc_commands_total`, `nexus_rpc_command_duration_seconds`, `nexus_rpc_frame_size_bytes_in/out` (the `nexus_` prefix here is the project-wide Prometheus namespace, not the module path)
- [ ] 8.5 Add `metrics::record_rpc_command(cmd, ok, elapsed)` helper
- [ ] 8.6 Slow-command warning threshold: 2 ms (configurable via `rpc.slow_threshold_ms`)

## 9. Main wiring
- [ ] 9.1 In `nexus-server/src/main.rs`, after `axum::serve` is prepared, call `spawn_rpc_listener` when `config.rpc.enabled`
- [ ] 9.2 Log `"Nexus RPC listening on {addr}"` at INFO on startup
- [ ] 9.3 Integration test: boot the server and PING via the RPC port returns PONG in <5 ms
- [ ] 9.4 Integration test: a Cypher `RETURN 1` via RPC matches the HTTP response exactly

## 10. Cargo + lint + coverage
- [ ] 10.1 Add `rmp-serde` to `nexus-server/Cargo.toml`
- [ ] 10.2 Add `tokio` features: `net`, `io-util`, `sync`, `macros`, `rt-multi-thread` (check what's missing)
- [ ] 10.3 `cargo +nightly fmt --all` clean
- [ ] 10.4 `cargo clippy --workspace -- -D warnings` clean (no `unwrap`/`expect` outside tests)
- [ ] 10.5 `cargo llvm-cov --package nexus-server --ignore-filename-regex 'examples'` coverage >= 95% on new files

## 11. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 11.1 Update or create documentation covering the implementation (`docs/specs/rpc-wire-format.md`, update `docs/specs/api-protocols.md`)
- [ ] 11.2 Write tests covering the new behavior (unit + integration; min 30 tests total across codec, dispatch, server)
- [ ] 11.3 Run tests and confirm they pass (`cargo test --workspace --verbose`)
