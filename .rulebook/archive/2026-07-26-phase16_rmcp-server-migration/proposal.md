# Proposal: phase16_rmcp-server-migration

Related: https://github.com/hivellm/nexus/issues/28 (does **not** close it — phase15 does)
**Not filed as an issue; discovered while investigating #28.**

## Why

Closing #28 by deleting the dead dependency (phase15) unblocks downstream consumers,
but it does not remove the vulnerability from Nexus itself. `nexus-server` genuinely
uses rmcp and resolves **0.8.3** (root `Cargo.lock:4884-4885`), which is affected by
**RUSTSEC-2026-0189** (patched in >= 1.4.0). The MCP StreamableHTTP surface mounted at
`/mcp` runs on that vulnerable version at runtime. `nexus-server` is an unpublished
binary, so nobody's `cargo audit` flags it for them — which is exactly why this would
otherwise go untracked.

This is the real 0.8 → 1.x/2.x API migration the issue anticipated. It is modest in
size (~14 struct-literal rewrites) but has a genuine validation problem that must be
fixed first, described below.

## What Changes

**Target version: 2.2.0, not 1.4.0.** Latest on crates.io is 2.2.0
(`cargo info rmcp`), so 1.x is already superseded, and the dominant migration cost —
the `#[non_exhaustive]` rewrite — is identical either way. Going to 1.4.0 would mean
paying it twice.

**The breaking change that matters (CONFIRMED against vendored sources
`~/.cargo/registry/.../rmcp-{0.8.5,1.5.0}`):** rmcp added `#[non_exhaustive]` to every
model struct in 1.0.0-alpha (*"add #[non_exhaustive] and mutation methods to improve
compatibility (#715)"*). Verified absent in 0.8.5 and present in 1.5.0 for `Tool`,
`ToolAnnotations`, `Implementation`, `ServerInfo`, `ListToolsResult`,
`ListResourcesResult`, `CallToolResult`. **Struct-literal construction from another
crate becomes a hard compile error**, including `..Default::default()` functional
update. Affected construction sites:

- `crates/nexus-server/src/api/streaming/tools.rs` — 9 `rmcp::model::Tool { .. }`
  literals (`:9, :38, :74, :102, :145, :169, :210, :243, :276`), each with a
  `ToolAnnotations::new().read_only(..)`
- `crates/nexus-server/src/api/streaming/service.rs` — `ServerInfo` / `Implementation`
  (`:33-46`), `ListToolsResult` (`:56-59`), `ListResourcesResult` (`:75-78`)

Changes that are **not** breaking, confirmed rather than assumed:
- `CallToolRequestParam` → `CallToolRequestParams` and `PaginatedRequestParam` →
  `PaginatedRequestParams` are renames, but back-compat aliases exist
  (`rmcp-1.5.0/src/model.rs:1088`, `:3010`) — existing imports keep compiling.
- `StreamableHttpService<S, M = LocalSessionManager>` lost its default type param, but
  `main.rs:1123` passes `LocalSessionManager` positionally so `M` is inferred; no
  explicit annotation exists anywhere to break.
- `ServerHandler` future bound moved from `+ Send` to `+ MaybeSendFuture`
  (`rmcp-1.5.0/src/handler/server.rs:217,:265,:272`). `async fn` impls should satisfy
  it under the default feature set, and rmcp's 1.3.0 changelog states the cfg-gating
  was done specifically to avoid a semver break — **HYPOTHESIS, not compile-verified.**
- The `macros` feature is enabled in both manifests but **no `#[tool]` /
  `#[tool_router]` / `#[tool_handler]` macro is used anywhere** — the trait is
  implemented by hand, so the feature can be dropped while we are here.

**The prerequisite: the surface this migration touches is the one surface with no
live test coverage.** This is why the test work comes first in the checklist rather
than in the tail.

- `crates/nexus-server/src/api/streaming/tests.rs` (530 lines, 19 tests including
  `test_get_info` and `test_get_nexus_mcp_tools`) is **dead code** — `:1-4` is
  `#[cfg(FALSE)]` with the comment `// DISABLED - Tests need update`. Not compiled.
- `tests/api_integration_test.rs` (994 lines) is the **only** file exercising the real
  `StreamableHttpService` transport wiring end-to-end over `/mcp`, and it is **never
  built**: the root `Cargo.toml` is a virtual manifest (`[workspace]`, no `[package]`),
  so a root-level `tests/` directory is not a cargo test target.
- What *is* live (`api/graph_correlation_mcp_tests.rs`, 1146 lines;
  `tests/streaming_mcp_write_test.rs`) covers the dispatcher/handler layer via
  `CallToolRequestParam` — useful, but it does not touch `ServerHandler`,
  `StreamableHttpService`, `LocalSessionManager`, `get_info`, or `list_tools`.

Net: a migration done today would compile-check but not behaviour-check exactly the
layer rmcp changed. Re-enabling the disabled tests and relocating the orphaned
integration test into `crates/nexus-server/tests/` is therefore step 1, not cleanup.

## Impact

- Affected specs: `docs/specs/api-protocols.md` (MCP section, if the advertised
  protocol version changes)
- Affected code: `crates/nexus-server/src/api/streaming/{tools.rs,service.rs,handlers.rs,dispatcher.rs}`
  (~1180 lines total, ~14 construction sites), `crates/nexus-server/src/main.rs:1111-1145`
  (transport setup), root `Cargo.toml:56` (pin), `crates/nexus-server/src/api/streaming/tests.rs`
  (re-enable), `tests/api_integration_test.rs` (relocate)
- Breaking change: NO for published crates (all of this is inside the unpublished
  `nexus-server` binary). Potentially user-visible if the MCP protocol version
  advertised to clients changes — verify against MCP clients before release.
- User benefit: removes a known-vulnerable dependency from the running server; the
  `/mcp` surface finally gains compiled, executed test coverage at the transport layer.

## References

- Advisory: RUSTSEC-2026-0189 (rmcp, patched >= 1.4.0); currently resolved 0.8.3 at
  root `Cargo.lock:4884-4885`
- Breaking-change evidence: `~/.cargo/registry/src/*/rmcp-1.5.0/src/model/tool.rs:36,:42`,
  `rmcp-1.5.0/src/handler/server.rs:217`, `rmcp-1.5.0/src/model.rs:1088,:3010`,
  `rmcp-1.5.0/src/transport/streamable_http_server/tower.rs:330`
- Dead/orphaned tests: `crates/nexus-server/src/api/streaming/tests.rs:1-4`,
  `tests/api_integration_test.rs`
- Prerequisite task: phase15_unblock-rmcp-downstream-audit (independent; ships first)
