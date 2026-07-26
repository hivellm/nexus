# Proposal: phase19_expose-mcp-allowed-hosts-config

**Follow-up from phase16_rmcp-server-migration. Priority: medium.**
Not filed as an issue; created when the rmcp 0.8→2.2 migration surfaced a behavior change.

## Why

The rmcp 0.8→2.2 server migration (phase16) pulled in rmcp's new DNS-rebinding
protection. `StreamableHttpServerConfig::default()` now ships
`allowed_hosts: ["localhost", "127.0.0.1", "::1"]` and rejects any request whose
`Host` header is not in that list with `403 Forbidden` (and any request with no
`Host`/`:authority` with `400`). `crates/nexus-server/src/main.rs` `create_mcp_router`
constructs the service with `Default::default()`, so the running `/mcp` endpoint is
now localhost-only.

This is the correct secure default and was deliberately kept in phase16 (it is a
security-remediation task). But Nexus supports binding to `0.0.0.0` for production
(auth becomes required there — see CLAUDE.md), and an MCP client reaching the server
over the network via its hostname or public IP will now get a 403. Those deployments
have no way to allow their real host without a config knob.

## What Changes

- Expose the MCP allowed-hosts (and optionally allowed-origins) as server
  configuration (env var + config file field), wired into `create_mcp_router` via
  `StreamableHttpServerConfig::default().with_allowed_hosts([...])`.
- Default stays localhost-only (secure by default); operators opt in to their own
  host(s). Document that `disable_allowed_hosts` exists but is not recommended for
  public deployments.
- Decide the sensible behavior when the server binds a non-localhost address: either
  auto-include that host or require the operator to set allowed-hosts explicitly, and
  warn loudly if `/mcp` would otherwise be unreachable.

## Impact

- Affected specs: none (config + docs)
- Affected code: `crates/nexus-server/src/main.rs` (`create_mcp_router`),
  `crates/nexus-server/src/config.rs` (new config field + env var), docs
- Breaking change: NO — default behavior unchanged (still localhost-only)
- User benefit: network MCP deployments can work again under rmcp ≥ 1.x's
  DNS-rebinding protection without disabling the safeguard wholesale
