# Tasks: phase19_expose-mcp-allowed-hosts-config

Expose the rmcp StreamableHttp `allowed_hosts` (DNS-rebinding protection) as server
config so network `/mcp` deployments can allow their real host without disabling the
safeguard. Default stays localhost-only. Follow-up from phase16.

## 1. Implementation
- [ ] 1.1 Add an `mcp_allowed_hosts` config field (config file + env var, e.g. `NEXUS_MCP_ALLOWED_HOSTS`) in `crates/nexus-server/src/config.rs`, defaulting to empty/None (= keep rmcp's secure localhost default)
- [ ] 1.2 Wire it into `create_mcp_router` in `crates/nexus-server/src/main.rs`: build `StreamableHttpServerConfig::default().with_allowed_hosts([...])` when configured; otherwise keep `Default::default()` (localhost-only). Warn at startup if the server binds a non-localhost address but no allowed-hosts are configured (so `/mcp` won't silently 403 remote clients)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation (config reference + `docs/specs/api-protocols.md` MCP section: how to allow network hosts, and the DNS-rebinding rationale)
- [ ] 2.2 Write tests covering the new behavior (a request with an allowed non-localhost `Host` succeeds; a disallowed `Host` still 403s; empty config preserves localhost-only)
- [ ] 2.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace`)
