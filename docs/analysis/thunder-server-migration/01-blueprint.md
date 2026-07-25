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
