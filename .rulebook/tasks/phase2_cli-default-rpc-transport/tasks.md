## 1. Implementation
- [ ] 1.1 Inventory every call-site in `nexus-cli/src/client.rs` + `commands/*.rs` that hits an HTTP endpoint; map each to the RPC verb already served by `nexus-server/src/protocol/rpc/dispatch/*` (or mark as "HTTP-only for now" with a reason)
- [ ] 1.2 Add `nexus-protocol` as a dependency of `nexus-cli` (workspace dep) and expose the RPC codec + types
- [ ] 1.3 Create `nexus-cli/src/transport/mod.rs` defining the `Transport` trait covering: `cypher`, `ping`, `stats`, `db_list`, `db_create`, `db_drop`, `db_use`, `ingest`, `knn_search`, `knn_traverse`, schema inspection, user / api-key admin ops that have RPC counterparts, export/import
- [ ] 1.4 Implement `RpcTransport` (wraps a persistent TCP connection, handles `hello` + `auth.required` + request correlation) in `nexus-cli/src/transport/rpc.rs`
- [ ] 1.5 Implement `HttpTransport` in `nexus-cli/src/transport/http.rs` by extracting the current `client.rs` behaviour behind the same trait; keep it usable for commands without an RPC verb
- [ ] 1.6 Replace the `http://localhost:3000` default in `nexus-cli/src/config.rs` with an `EndpointConfig { scheme: Rpc, host: "127.0.0.1", port: 15475 }`; update precedence loader (CLI flag > env var > config file > default)
- [ ] 1.7 Add `--transport rpc|http` flag + `NEXUS_TRANSPORT` env var parsing in `nexus-cli/src/main.rs`; implement URL-scheme parsing with `nexus://` as the canonical Nexus binary-RPC scheme (not `nexus-rpc://`), plus `http://`, `https://`, and bare `host:port` (defaults to RPC on 15475)
- [ ] 1.8 Route every `commands::*` entry point through `Transport`; for commands without an RPC verb emit an explicit `eprintln!("falling back to HTTP for <cmd>: no RPC verb yet")` before dispatching to `HttpTransport`
- [ ] 1.9 `cargo +nightly fmt --all` + `cargo +nightly clippy -p nexus-cli --all-targets --all-features -- -D warnings` clean

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation: `nexus-cli/README.md` "Configuration" section + a new `docs/guides/CLI.md` (or extend the existing CLI doc) "Transports" section explaining `--transport`, `NEXUS_TRANSPORT`, the default, and how to diagnose by forcing HTTP
- [ ] 2.2 Write tests covering the new behavior: unit tests for URL-scheme parsing; an integration test that boots a `NexusServer`, exercises `nexus query 'RETURN 1'` via `RpcTransport`, and asserts the response matches the HTTP equivalent
- [ ] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-cli`
