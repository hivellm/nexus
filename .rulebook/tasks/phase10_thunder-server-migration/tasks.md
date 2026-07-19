# Tasks: phase10_thunder-server-migration

Migrate the Nexus native binary RPC server (port 15475) from the hand-rolled
MessagePack transport to `thunder-rpc` (Thunder wire v1 — already byte-identical).
Reference seam: Synap `crates/synap-server/src/protocol/synap_rpc/`.

## 1. Dependency and protocol config
- [ ] 1.1 Workspace `Cargo.toml`: add `thunder-rpc = { version = "0.2.2", default-features = false }` to `[workspace.dependencies]`; `crates/nexus-server/Cargo.toml` depends with `features = ["server"]` and dev-dependency with `features = ["client", "server"]`
- [ ] 1.2 Create `crates/nexus-server/src/protocol/rpc/config.rs` with `nexus_thunder_config() -> thunder::Config`: scheme `nexus`, port 15475, handshake `AuthCommand`, push `Reserved`, error codes `Resp3Prefixes`, `max_frame_bytes` 64 MiB — mirror Synap `synap_rpc/config.rs`. This is the **server's own copy**: the SDK will carry an independent duplicate (phase11), neither importing the other — no shared protocol crate
- [ ] 1.3 Pin the config with a unit test asserting every literal field value (each SDK re-declares these independently in its own language; with no shared type this test pair is the only thing preventing wire drift) — mirror Synap `config.rs` test

## 2. Dispatch bridge
- [ ] 2.1 In `protocol/rpc/server.rs` (or new `thunder_dispatch.rs`): `struct NexusDispatch { state }` implementing `thunder::server::Dispatch` with `type Identity` = the authenticated principal (api-key or user identity from the existing auth manager)
- [ ] 2.2 `dispatch()` delegates to the existing `dispatch::run(state, cmd, args)` command tree unchanged (CYPHER … EXPORT/IMPORT); handle `HELLO`/`PING` semantics consistently with Thunder's pre-auth allowlist
- [ ] 2.3 `authenticate()` maps `thunder::server::Credentials::{ApiKey, UserPass, Token, None}` onto the existing auth manager, preserving current `AUTH <key>` / `AUTH <user> <pass>` behavior and `NEXUS_RPC_REQUIRE_AUTH` (use `ListenerConfig.open()` when auth is disabled)
- [ ] 2.4 Migrate `dispatch/` modules from `nexus_protocol::rpc::NexusValue` to `thunder::Value` (type alias first, then mechanical rename; arg helpers `arg_str`/`arg_bytes`/`arg_int`/… keep their signatures)

## 3. Listener swap
- [ ] 3.1 Replace the hand-rolled accept loop in `protocol/rpc/server.rs` with `thunder::server::spawn_listener(dispatch, nexus_thunder_config(), ServerInfo, ListenerConfig)`; map `NEXUS_RPC_{ADDR,MAX_FRAME_BYTES,MAX_IN_FLIGHT,SLOW_MS}` onto `ListenerConfig` (addr, frame cap, connection/in-flight bounds, `slow_threshold`, `idle_timeout`); hold the `ListenerHandle` for process lifetime in `main.rs:441` region
- [ ] 3.2 Implement `thunder::server::MetricsObserver` feeding the existing `nexus_rpc_*` Prometheus series in `protocol/rpc/metrics.rs` (`command_completed`, `connection_opened/closed/refused`); register via `ListenerConfig.with_observer`
- [ ] 3.3 Delete the now-dead hand-rolled framing/writer-task/semaphore code from `server.rs`; document any knob that no longer maps 1:1 (e.g. per-conn in-flight semantics) in the module rustdoc

## 4. Shim nexus-protocol::rpc (crate is deleted in phase11)
- [ ] 4.1 Replace `crates/nexus-protocol/src/rpc/types.rs` + `codec.rs` bodies with re-exports: `pub use thunder::{Value as NexusValue, Request, Response}` and codec fns from `thunder::wire`. These are temporary compile shims for `nexus-cli` and the Rust SDK across the phase boundary only — the crate itself is removed in phase11, so do NOT invest in this surface or let new consumers adopt it
- [ ] 4.2 Add `thunder-rpc = { version = "0.2.2", default-features = false }` to `crates/nexus-protocol/Cargo.toml`; drop now-unused `rmp-serde` direct deps if nothing else uses them; `rest.rs`/`mcp.rs`/`umicp.rs`/`resp3/` untouched here (they relocate into `nexus-server` in phase11)
- [ ] 4.3 Confirm nothing in `nexus-server` still imports from `nexus_protocol::rpc` — the server must consume `thunder` directly, so that when the crate disappears in phase11 the server needs no further change
- [ ] 4.4 Audit `Dockerfile` and `.github/workflows/**` for references to removed files/paths (Synap gotcha: image build broke on COPY of dissolved crate)

## 5. Integration tests (mirror Synap synap_rpc_thunder_tests.rs)
- [ ] 5.1 New `crates/nexus-server/tests/thunder_rpc_tests.rs` using `thunder::client::Client` against a spawned listener: PING/HELLO pre-auth, AUTH gate (`NOAUTH` before, success after), CYPHER round-trip, Bytes embedding round-trip (KNN_SEARCH with raw f32 LE Bytes == Array<Float>)
- [ ] 5.2 Legacy-wire compat test: hand-encoded legacy frames (Bytes as int-array, map-shaped Request) still decode and execute — proves old deployed SDKs survive the swap
- [ ] 5.3 Error-model test: dispatch `Err(String)` surfaces as `ClientError::Server` with the connection still usable; `NOAUTH`/`WRONGPASS` map to the Auth class

## 6. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 6.1 Update or create documentation covering the implementation (`docs/specs/rpc-wire-format.md` declares the wire as Thunder v1 with links to Thunder SPEC docs; update `docs/specs/api-protocols.md`; CHANGELOG entry)
- [ ] 6.2 Write tests covering the new behavior (section 5 integration tests + config-pin unit test; keep coverage ≥ 95% on touched code)
- [ ] 6.3 Run tests and confirm they pass (`cargo +nightly fmt --all` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green, `scripts/compatibility/test-transport-parity.sh` green)
