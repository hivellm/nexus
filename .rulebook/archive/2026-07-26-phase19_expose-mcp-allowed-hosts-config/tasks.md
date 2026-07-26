# Tasks: phase19_expose-mcp-allowed-hosts-config

Expose the rmcp StreamableHttp `allowed_hosts` (DNS-rebinding protection) as server
config so network `/mcp` deployments can allow their real host without disabling the
safeguard. Default stays localhost-only. Follow-up from phase16.

## 1. Implementation
- [x] 1.1 Added `mcp_allowed_hosts: Vec<String>` (env `NEXUS_MCP_ALLOWED_HOSTS`, comma-separated) + `mcp_allowed_hosts_disable: bool` (env `NEXUS_MCP_ALLOWED_HOSTS_DISABLE`) to `crates/nexus-server/src/config.rs`. Default empty/false → falls through to rmcp's secure localhost default (NOT `with_allowed_hosts([])`, which would disable all hosts)
- [x] 1.2 Wired into `create_mcp_router` in `main.rs`: non-empty list → `StreamableHttpServerConfig::default().with_allowed_hosts(...)`; disable flag → `.disable_allowed_hosts()`; empty + not-disabled → `Default::default()` unchanged. Startup WARNING when binding a non-loopback address with no allowed-hosts configured (so `/mcp` won't silently 403 remote clients)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — `config.rs` doc-comments on both fields; `docs/specs/api-protocols.md` MCP Host section extended with how to set `NEXUS_MCP_ALLOWED_HOSTS` for network deployments + the `NEXUS_MCP_ALLOWED_HOSTS_DISABLE` escape hatch (with its not-recommended caution); CHANGELOG `[3.0.0]` `### Added — phase19_...` entry
- [x] 2.2 Write tests covering the new behavior — added to `tests/mcp_streamable_http_test.rs` (raw-JSON + SSE harness, new `mcp_post_request_with_host` helper): `allows_configured_extra_host` (with_allowed_hosts(["example.com"]) + Host: example.com → 200 + session), `rejects_host_outside_allow_list` (403), `default_allow_list_still_enforced` (default config + non-localhost Host → 403). All pass
- [x] 2.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-server --test mcp_streamable_http_test` 4/4; full `cargo +nightly test -p nexus-server` green (590 lib + all binaries, 0 failed); `clippy -p nexus-server --all-targets --all-features -- -D warnings` clean; `fmt --package nexus-server` clean
