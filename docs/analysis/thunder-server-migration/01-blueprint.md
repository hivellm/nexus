# Thunder server migration (phase10) — implementation blueprint

Durable record of the two research passes that scoped
`phase10_thunder-server-migration`, so §2–§6 continue without re-researching.
All Thunder API facts are from the **registry copy**
`~/.cargo/registry/src/index.crates.io-*/thunder-rpc-0.2.2/` (the pinned
source of truth), not the possibly-ahead `E:\HiveLLM\Thunder` repo. The
crate is `thunder-rpc`; the import path is `thunder`.

Status: **§1 done** (commit `0351a69f` — dep + `nexus_thunder_config()` +
pin test). §2–§6 pending.

---

## A. The Nexus seams to bridge / preserve (current RPC)

- **Dispatch entry to delegate to (KEEP unchanged):**
  `crates/nexus-server/src/protocol/rpc/dispatch/mod.rs`
  - `pub async fn dispatch(state: &RpcSession, req: Request) -> Response` (`:63`) —
    wraps `run` into `Response::ok/err`.
  - `pub async fn run(state: &RpcSession, command: &str, args: Vec<NexusValue>) -> Result<NexusValue, String>` (`:73`) —
    uppercases, applies the `NOAUTH` gate, routes to group `run`s.
    **This is the seam the Thunder `Dispatch::dispatch` should call** (return the bare `Result`, let Thunder own id/encoding).
  - Group modules all: `pub async fn run(state: &RpcSession, command: &str, args: &[NexusValue]) -> Result<NexusValue, String>`.
  - Commands: admin(PING/HELLO/AUTH/QUIT/STATS/HEALTH), cypher(CYPHER), graph(CREATE_NODE/CREATE_REL/UPDATE_NODE/DELETE_NODE/MATCH_NODES), knn(KNN_SEARCH/KNN_TRAVERSE), ingest(INGEST), schema(LABELS/REL_TYPES/PROPERTY_KEYS/INDEXES), database(DB_LIST/DB_CREATE/DB_DROP/DB_USE), export(EXPORT/IMPORT).
- **`RpcSession`** (`dispatch/mod.rs:36`): `{ server: Arc<NexusServer>, authenticated: Arc<AtomicBool>, auth_required: bool, connection_id: u64 }` + `is_authenticated()`/`mark_authenticated()`.
- **Arg helpers** (`dispatch/mod.rs:115-180`, KEEP): `arg_str/arg_bytes/arg_int/arg_float/arg_map/arg_array(args: &[NexusValue], idx) -> Result<_, String>`.
- **Auth** (`dispatch/admin.rs:89-146`): `AUTH <key>` → `auth_manager.verify_api_key`; `AUTH <user> <pass>` → root fast-path (`server.root_user_config`) else `server.rbac.read().await` + `nexus_core::auth::verify_password(pass, hash)`. Pre-auth allowlist `PRE_AUTH_COMMANDS = ["PING","HELLO","AUTH","QUIT"]` (`dispatch/mod.rs:54`); gate in `run` returns `"NOAUTH authentication required to run '{command}'"`.
- **Metrics** (`protocol/rpc/metrics.rs`, feed from a Thunder observer): `rpc_connection_open()`, `rpc_connection_close()`, `record_rpc_command(cmd:&str, ok:bool, secs:f64)`, `record_rpc_frame_sizes(in:usize, out:usize)`, `record_rpc_slow_command()`, `snapshot()->RpcMetricsSnapshot`. Prometheus series rendered in `api/prometheus.rs:201-257`: `nexus_rpc_{connections, commands_total, commands_error_total, command_duration_microseconds_total, frame_bytes_in_total, frame_bytes_out_total, slow_commands_total}`.
- **Spawn site** `main.rs:480-506`: `spawn_rpc_listener(server.clone(), config.rpc.addr, config.rpc.clone(), config.rpc.require_auth).await` — currently returns `Ok(())` after bind and detaches (no handle held). New wiring holds an `Arc<ListenerHandle>` for process lifetime.
- **Config** `config.rs:176-210`: `RpcConfig { enabled, addr, require_auth, max_frame_bytes, max_in_flight_per_conn, slow_threshold_ms }`, default addr `0.0.0.0:15475`, frame 64 MiB, in_flight 1024, slow 2ms. Env `NEXUS_RPC_{ENABLED,ADDR,REQUIRE_AUTH(→auth.enabled),MAX_FRAME_BYTES,MAX_IN_FLIGHT,SLOW_MS}` (`config.rs:1008+`).
- **Wire types → thin re-export shims (§4)** `nexus-protocol/src/rpc/`: `NexusValue` (8 variants: Null/Bool/Int/Float/Bytes/Str/Array/Map), `Request{id:u32, command:String, args:Vec<NexusValue>}`, `Response{id:u32, result:Result<NexusValue,String>}`, `PUSH_ID=u32::MAX`, `DEFAULT_MAX_FRAME_BYTES=64MiB`, codec `encode_frame/decode_frame[_with_limit]/read_request[_with_limit]/read_response/write_request/write_response`, `DecodeError`. Comment claims Synap byte-for-byte — Thunder v1 preserves it.
- **Consumers the shim must keep compiling** (outside nexus-protocol): `nexus-server/protocol/rpc/server.rs`; `nexus-cli/src/{rpc_transport.rs,client.rs}`; `sdks/rust/src/{transport/rpc.rs,transport/mod.rs,transport/http.rs,transport/command_map.rs,client.rs}`; `nexus-bench/src/client/rpc.rs`; `nexus-core/benches/protocol_point_read.rs`. Plus integration tests. **Docker/CI**: `Dockerfile:84` `COPY crates/nexus-protocol`; workflow path filters `crates/nexus-protocol/**` in rust-test/rust-lint + all 5 sdk-*-test workflows; `sdk-release.yml` publish-order comments.

## B. Thunder 0.2.2 API essentials

- **Crate root** (`thunder::`): `Value, Request, Response, Config, wire, decode_frame[_with_limit], encode_frame, DEFAULT_MAX_FRAME_BYTES(64MiB), PUSH_ID(u32::MAX)`. Server (feature `server`): `spawn_listener, Dispatch, ListenerConfig, ListenerHandle, MetricsObserver, MetricsSnapshot, Principal, ServerInfo, Session, AuthError, NOAUTH, NOPERM, WRONGPASS, PushSender`. Client (feature `client`): `Client, ClientConfig, ClientError`. **`server::Credentials` and `client::Credentials` are DISTINCT and NOT root-re-exported** — reach via modules. `TlsPolicy` at `thunder::wire::config::TlsPolicy`.
- **`Dispatch` trait** (RPIT async, `Send+Sync+'static`): `type Identity: Send+Sync+'static;` + `async fn dispatch(&self, &Session<Identity>, command:&str, args:Vec<Value>) -> Result<Value,String>` + `async fn authenticate(&self, Credentials) -> Result<Principal<Identity>, AuthError>` + `fn capabilities(&self, &Principal<Identity>) -> Vec<String>` (default `vec![]`).
- **`server::Credentials`**: `ApiKey(String) | UserPass(String,String) | Token(String) | None`.
- **`Principal<I=()>`** `{ name:String, identity:I }`; `Principal::new(name)` / `Principal::with_identity(name, id)`.
- **`AuthError`**: `InvalidCredentials` (→ `WRONGPASS` under Resp3Prefixes) | `Message(String)` (verbatim).
- **`ListenerConfig`** fields: `addr, idle_timeout, slow_threshold, auth_required, tls, max_connections, observer`. Builders: `new(addr)` (idle=0, slow=1000ms, auth_required=true, unbounded), `.with_observer(Arc<dyn MetricsObserver>)`, `.with_max_connections(usize)` (0=unbounded), `.open()` (auth_required=false). **`idle_timeout`/`slow_threshold` have NO builder — assign the field directly.**
- **`spawn_listener<D:Dispatch>(dispatch:Arc<D>, profile:Config, info:ServerInfo, config:ListenerConfig) -> io::Result<ListenerHandle>`**. `ServerInfo{name,version}`.
- **`ListenerHandle`**: `local_addr()`, `snapshot()`, `metrics()`, **`async fn stop(&self)`** (drains; `&self` → wrap in `Arc` to share). `Drop` = fire-and-forget shutdown.
- **`MetricsObserver`** (1 required + 4 default): `command_completed(&self, command:&str, in_bytes:usize, out_bytes:usize, duration:Duration, is_error:bool)` + `connection_opened/closed/refused(&self){}` + `push_emitted(&self, out_bytes:usize){}`. Fire on writer task after socket write; keep non-blocking.
- **`Session<I=()>`**: `connection_id()`, `is_authenticated()`, `with_principal(|Option<&Principal<I>>| -> R)`, `principal_name()`, `push_sender()->Option<&PushSender>` (Some only under push=Enabled), `principal()` (I:Clone).
- **`Config`** fields: `scheme, default_port, handshake, hello_style, push, max_frame_bytes, max_in_flight, error_codes, tls`. `Config::standard()` const base (HelloMandatory/MapPayload/Reserved/64MiB/256/Both/Off) + const setters `.scheme/.port/.handshake/.hello_style/.push/.max_frame_bytes/.max_in_flight/.error_codes/.tls`. Enums in `thunder::wire::config`: `Handshake{None,AuthCommand,HelloMandatory}`, `HelloStyle{NotUsed,ArgLess,MapPayload}`, `PushPolicy{Reserved,Enabled}`, `ErrorConvention{None,Resp3Prefixes,BracketCode,Both}`, `TlsPolicy{Off,Optional,Reserved}`.
- **Client (tests)**: `Client::connect_with("nexus://127.0.0.1:port", config, ClientConfig)`; `ClientConfig::new().api_key(s)/.user_pass(u,p)/.token(s)/.client_name(s)`; `client.call(cmd, Vec<Value>) -> Result<Value, ClientError>`, `.on_push(F)`, `.close()`. `client::Credentials{Token,ApiKey,UserPass{user,pass}}` (no None).

## C. Load-bearing gotchas (from Synap CHANGELOG 1.2.0 + listener code)

1. **HELLO/PING/AUTH/QUIT owned by the listener**, not the command tree. Under `AuthCommand` the listener answers pre-auth `PING` inline (`builtin_ping`) and drives `AUTH` via `authenticate()`. **Do NOT keep/add HELLO or PING handlers** that the listener would shadow — verify Nexus's current admin PING/HELLO handlers against the listener behavior (`listener.rs` handle_auth/builtin_ping) during §2; likely the admin PING/HELLO arms become dead for the Thunder path.
2. **Enforcement is `ListenerConfig::auth_required` (`.open()`), NOT the profile.** Wire Nexus's `require_auth` → `.open()` when false. Conflating handshake shape with enforcement is the documented BN-023 trap that left Synap unable to auth.
3. **`NexusValue = thunder::Value`** (type alias, not newtype) keeps the whole dispatch tree + arg helpers compiling unchanged (§2.4). `Value::bytes()/Str/as_str/map_get` all exist on `thunder::Value`.
4. **`Bytes` emitted as msgpack `bin`** (Thunder canonical; both bin and legacy int-array decode forever). Keep a legacy-decode compat test (§5.2).
5. **Return conventions from `dispatch`**: `Err(String)` = command error, verbatim on wire, connection stays open; `"NOPERM …"` verbatim for ACL; `authenticate` returns `AuthError::InvalidCredentials` for the profile-correct `WRONGPASS`. `NOAUTH` is emitted by the listener gate (don't hand-roll it — but the existing `run` gate returning `"NOAUTH …"` is harmless if the command reaches dispatch; reconcile so it's not doubled).
6. **`max_in_flight`**: Thunder has `Config::max_in_flight` (256 default) — Nexus's per-conn `NEXUS_RPC_MAX_IN_FLIGHT` (1024) maps here or document the delta (§3.3).
7. **Push disabled** (`PushPolicy::Reserved`) → `session.push_sender()` returns `None`; no `push_emitted`/SUBSCRIBE bridge needed (Nexus diverges from Synap which uses Enabled).
8. Never send a client request with `id = u32::MAX` (PUSH_ID); the Thunder client allocates ids skipping it.

## D. Skeleton for §2/§3 (adapt to Nexus's `RpcSession`/auth types)

Mirror Synap `crates/synap-server/src/protocol/synap_rpc/server.rs`. Key
divergences for Nexus: `type Identity` — decide whether to carry the
authenticated principal (api-key/user) or `()` and keep using the existing
`RpcSession.authenticated` AtomicBool. Simplest first cut: `type Identity =
()`, keep the `RpcSession` created per connection from `session.connection_id()`
and the auth flag mirrored via `session.is_authenticated()`; `authenticate()`
maps `Credentials` onto `auth_manager.verify_api_key` / rbac+`verify_password`
and returns `Principal::new(...)`. Then `dispatch()` builds an `RpcSession`
(server Arc + authenticated flag from `session.is_authenticated()` +
`auth_required` + `session.connection_id()`) and calls
`dispatch::run(&rpc_session, command, args)`.

`spawn_nexus_rpc_listener` mirrors §1.3 of the Synap blueprint: build
`ListenerConfig::new(addr).with_max_connections(n).with_observer(Arc::new(NexusMetrics))`,
set `idle_timeout`/`slow_threshold` by field, `.open()` iff `!require_auth`,
then `spawn_listener(Arc::new(NexusDispatch{..}), nexus_thunder_config(),
ServerInfo{name:"nexus",version:env!("CARGO_PKG_VERSION")}, cfg).await` →
`Arc<ListenerHandle>` held in `main.rs`.

Integration tests (§5): mirror `synap_rpc_thunder_tests.rs` — `start()` binds
`127.0.0.1:0`, `thunder::client::Client::connect_with("nexus://{addr}",
nexus_thunder_config(), ClientConfig)`; cover PING pre-auth, AUTH gate
(NOAUTH before / OK after), CYPHER round-trip, KNN Bytes round-trip, legacy
int-array Bytes decode, unknown-command-keeps-connection, over-cap length
prefix refused, connection ceiling, graceful `stop()`.

---

## E. §2+§3 are ONE atomic increment (concrete rewrite plan)

The `NexusValue = thunder::Value` alias (§2.4) breaks the old `server.rs`
immediately (its accept loop uses `nexus_protocol`'s codec + `Request`/
`Response`), and an unwired `NexusDispatch` fails `clippy -D warnings`
(dead_code). So there is **no intermediate sub-commit that compiles** —
§2 (bridge + alias) and §3 (listener swap + delete old accept loop) land
together and compile once at the end. Treat as a fresh focused increment.

### Changes, in order
1. **`protocol/rpc/mod.rs`**: replace the `nexus_protocol::rpc` re-export
   with `pub type NexusValue = thunder::Value;` + `pub use thunder::{Request,
   Response};`. Drop the codec re-exports (the old accept loop that used them
   is being deleted). Keep `pub use config::nexus_thunder_config;`.
2. **`server.rs` — full rewrite** (delete accept loop / handle_connection /
   writer task / semaphore / encoded_request_size / the old `#[cfg(test)]`
   TCP tests). New contents:
   - `struct NexusDispatch { server: Arc<NexusServer>, auth_required: bool }`.
   - `impl thunder::server::Dispatch`: `type Identity = ()` (simplest — keep
     using `RpcSession.authenticated` mirrored from `session.is_authenticated()`).
     - `async fn dispatch(&self, session: &Session<()>, command, args) ->
       Result<Value,String>`: build an `RpcSession { server: self.server.clone(),
       authenticated: Arc::new(AtomicBool::new(session.is_authenticated())),
       auth_required: self.auth_required, connection_id: session.connection_id() }`
       then `dispatch::run(&rpc, command, args).await`. (Reconcile the double
       NOAUTH gate — see UNKNOWN below.)
     - `async fn authenticate(&self, creds: Credentials) -> Result<Principal<()>,
       AuthError>`: `ApiKey(k)|Token(k)` → `matches!(self.server.auth_manager
       .verify_api_key(&k), Ok(Some(_)))`; `UserPass(u,p)` → root fast-path
       (`self.server.root_user_config`) else `self.server.rbac.read().await`
       list_users + `is_active` + `nexus_core::auth::verify_password(p, hash)`;
       `None` → `Err(AuthError::InvalidCredentials)`. On success
       `Ok(Principal::new(name))`, else `Err(AuthError::InvalidCredentials)`
       (→ profile-correct WRONGPASS). NOTE: factor the `verify_user_password`
       body out of `dispatch/admin.rs` (it currently takes `&RpcSession` but only
       needs `&NexusServer`) OR duplicate it — don't leave two copies drifting.
   - `struct NexusMetrics; impl MetricsObserver`: `command_completed(cmd,in,out,
     dur,is_err)` → `metrics::record_rpc_command(cmd, !is_err, dur.as_secs_f64())`
     + `metrics::record_rpc_frame_sizes(in,out)` + WARN + `record_rpc_slow_command()`
     when `dur > SLOW`; `connection_opened/closed` → `metrics::rpc_connection_open/close`;
     `connection_refused` → (add a counter or reuse). `const SLOW = from_millis(config.slow_threshold_ms)` — but the observer has no config; pass slow via a field on NexusMetrics or use a fixed 2ms.
   - `pub async fn spawn_rpc_listener(server, addr, config: RpcConfig, auth_required)
     -> std::io::Result<Arc<ListenerHandle>>` (KEEP THE NAME so main.rs barely
     changes, but change the return type to hold the handle): build
     `ListenerConfig::new(addr).with_max_connections(0 or a mapped cap)
     .with_observer(Arc::new(NexusMetrics{..}))`; set `.idle_timeout` /
     `.slow_threshold` by field; `.open()` iff `!auth_required`; then
     `spawn_listener(Arc::new(NexusDispatch{server, auth_required}),
     nexus_thunder_config(), ServerInfo{name:"nexus",version:env!("CARGO_PKG_VERSION")},
     cfg).await.map(Arc::new)`.
3. **`dispatch/mod.rs`**: `arg_bytes` → `Ok(b.to_vec())` (was `b.clone()`, now
   Arc). Fix the `#[cfg(test)]` `NexusValue::Bytes(vec![..])` sites →
   `NexusValue::bytes(vec![..])` (thunder's `Value::bytes(impl Into<Arc<[u8]>>)`).
4. **`dispatch/convert.rs:27`**: `String::from_utf8(b)` → `String::from_utf8(b.to_vec())`
   (b is now `Arc<[u8]>`). Fix its `#[cfg(test)]` `Bytes(..)` construction sites.
5. **`dispatch/admin.rs`**: `:42` `Bytes(b.clone())` compiles (Arc clone). Fix the
   two `#[cfg(test)]` `Bytes(vec![..])` sites → `bytes(..)`.
6. **`dispatch/knn.rs`**: `:165` read arm reads `b` for f32 chunking — `&Arc<[u8]>`
   derefs to `&[u8]`, verify it compiles (likely fine); fix the 3 test `Bytes(raw)`
   sites → `bytes(raw)`.
7. **`main.rs:480-506`**: `let handle = spawn_rpc_listener(...).await?` now returns
   `Arc<ListenerHandle>` — hold it for process lifetime (store in a `let _rpc =`
   that lives as long as the server, or push into a keep-alive Vec like the other
   listeners). Log `handle.local_addr()`.
8. **Metrics**: `metrics.rs` gains a `record_rpc_connection_refused` if we want the
   `connection_refused` observer callback to feed a series (optional — the current
   Prometheus set has no refused counter; add one or drop the callback).

### THE ONE REMAINING UNKNOWN (read before writing §2.2)
Read `~/.cargo/registry/src/index.crates.io-*/thunder-rpc-0.2.2/src/server/listener.rs`
for exactly which allowlist commands the listener ANSWERS ITSELF vs ROUTES to
`Dispatch::dispatch`, under `Handshake::AuthCommand` + `HelloStyle::NotUsed`:
- Pre-auth `PING` is answered builtin (`builtin_ping`). Does an AUTHENTICATED
  `PING` also get intercepted, or routed to dispatch (→ admin PING handler)?
- Is `HELLO` intercepted under `NotUsed`, or routed to dispatch (→ admin HELLO)?
- `AUTH` is driven via `authenticate()` (never reaches dispatch). `QUIT` closes.
This decides whether Nexus's admin `PING/HELLO/QUIT` arms stay (routed) or become
dead (intercepted). If intercepted, remove those arms from `dispatch/mod.rs`'s
`run` match + `admin.rs` and drop them from `PRE_AUTH_COMMANDS`; keep the
`run` NOAUTH gate only if the listener does NOT already enforce it for routed
commands (it does under AuthCommand — so the `run` gate likely becomes redundant
and should be removed to avoid a double-gate that returns a different NOAUTH string
than Thunder's `NOAUTH Authentication required.`).

### §5 tests (new `tests/thunder_rpc_tests.rs`) replace the deleted server.rs TCP tests
Use `thunder::client::Client::connect_with("nexus://{addr}", nexus_thunder_config(),
ClientConfig)`. Port the old server.rs tests (ping, multiplex, unknown-command-keeps-conn,
auth-gate root/root) plus §5.2 legacy int-array Bytes decode + §5.3 error model.
Build the `NexusServer` via the same helper the old `spawn_test_server` used
(lines 259-309 of the pre-rewrite server.rs — preserve that construction).

---

## F. §4 shim — scoped (bigger than "pure re-export"; own focused pass)

The system is FULLY FUNCTIONAL after §2+§3 without §4: the server speaks
Thunder wire, the out-of-server consumers still speak `nexus-protocol`'s own
wire, and the two are byte-identical (proven by `rpc_integration_test` 8/8).
§4 only CONSOLIDATES the consumer-side types onto Thunder ahead of phase11's
deletion of `nexus-protocol`. It is a THROWAWAY shim ("do NOT invest").

**Good news — `thunder::wire` provides everything for a near-pure re-export:**
- `thunder::Request { id:u32, command:String, args:Vec<Value> }` and
  `thunder::Response { id:u32, result:Result<Value,String> }` (`ok`/`err`) are
  IDENTICAL to `nexus_protocol::rpc::{Request,Response}`.
- `thunder::wire` re-exports `encode_frame, decode_frame, decode_frame_with_limit,
  DecodeError` (sync) AND — under the `tokio` feature — `read_request,
  read_request_with_limit, read_response, read_response_with_limit,
  write_request, write_response, read_frame, write_frame` (async). So the
  consumers' `nexus_protocol::rpc::codec::{read_response, write_request}` imports
  can be satisfied by re-export.
- `thunder::Value` is a superset of the old `NexusValue` accessors
  (as_str/as_bytes/as_int/as_float/as_bool/as_array/as_map/map_get/is_null/bytes)
  + From impls (incl. From<Vec<u8>>).

**The catch — blast radius is `nexus-protocol`'s OWN test suite (148 tests):**
`type NexusValue = thunder::Value` means Bytes is `Arc<[u8]>` not `Vec<u8>`.
Gutting `src/rpc/types.rs` + `codec.rs` to re-exports deletes/invalidates their
`#[cfg(test)]` modules (types 5, codec 11) AND breaks the integration suites
`tests/protocol_extended_tests.rs` (**120 tests**) + `tests/auth_test.rs` (12),
many of which construct `NexusValue::Bytes(vec![...])` (→ `Value::bytes(...)`)
or assert wire specifics. Plus 2 consumer read sites: `nexus-bench/src/client/
rpc.rs:298` `String::from_utf8(b)` → `.to_vec()`, and `nexus-cli/src/client.rs:834`
`b.into_iter().map(Value::from)` (owned `Arc<[u8]>` is not `IntoIterator<Item=u8>`
→ `b.iter().copied()`). `sdks/rust/.../http.rs:227` `b.iter().copied()` already
works via deref.

**Plan for the focused pass:**
1. `nexus-protocol/Cargo.toml`: `thunder-rpc = { version = "0.2.2",
   default-features = false, features = ["tokio"] }` (enables the async wire
   helpers; verify "tokio" is directly nameable, else use "client"). Drop
   `rmp-serde` direct dep IF nothing else in the crate (rest.rs/mcp.rs/umicp.rs/
   resp3/) uses it — grep first.
2. `src/rpc/types.rs` → `pub use thunder::{Value as NexusValue, Request, Response};`
   (delete its `#[cfg(test)]`). `src/rpc/codec.rs` → `pub use thunder::wire::{...}`
   (delete its `#[cfg(test)]`). `src/rpc/mod.rs` → `pub use thunder::{PUSH_ID};`
   `pub use thunder::wire::DEFAULT_MAX_FRAME_BYTES;` keep the top-level re-exports.
3. Rewrite/prune `tests/protocol_extended_tests.rs` + `auth_test.rs`: fix every
   `Bytes(vec!)` → `bytes(vec!)`; drop any byte-exact assertions that were
   testing the old hand-rolled codec's identity (Thunder's is the same wire, but
   don't re-assert Thunder's internals from here). These 132 tests are the real
   work — decide per-test keep-and-fix vs delete (they largely duplicate Thunder's
   own conformance suite now).
4. Consumer Bytes fixes: nexus-bench:298 `.to_vec()`, nexus-cli:834 `b.iter().copied()`.
5. §4.3: the server LIB no longer imports `nexus_protocol::rpc` (done in §2). The
   `nexus-server/tests/rpc_integration_test.rs` still does (deliberately — it's the
   old-codec wire-compat proof); it moves/becomes a Thunder-client test when
   `nexus-protocol` is deleted in phase11, or stays until then.
6. §4.4: `Dockerfile:84` COPY + workflow `crates/nexus-protocol/**` path filters
   stay valid until phase11 deletes the crate — audit but likely no change now.

Given the 132-test rewrite, §4 is its own focused increment, not a tail task.
