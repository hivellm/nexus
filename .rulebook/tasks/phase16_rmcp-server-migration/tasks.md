# Tasks: phase16_rmcp-server-migration

Migrate `nexus-server` off the vulnerable rmcp 0.8.3 (RUSTSEC-2026-0189) to 2.2.0.
Section 1 comes first deliberately: the `/mcp` transport layer this migration breaks
is currently the one layer with no compiled test coverage, so migrating first would
ship unvalidated. Does not close issue #28 — phase15 does that independently.

## 1. Restore validation BEFORE migrating (prerequisite)
- [ ] 1.1 Re-enable `crates/nexus-server/src/api/streaming/tests.rs` — currently `#[cfg(FALSE)]` at `:1-4` (`// DISABLED - Tests need update`), so all 19 tests including `test_get_info` and `test_get_nexus_mcp_tools` are dead code. Update them against the current API and get them compiling and passing on rmcp 0.8 first, so they form the before/after baseline
- [ ] 1.2 Relocate `tests/api_integration_test.rs` (994 lines) into `crates/nexus-server/tests/` so cargo actually builds it — the root `Cargo.toml` is a virtual manifest with no `[package]`, so a root-level `tests/` dir is never a test target. This is the only file exercising the real `StreamableHttpService` wiring over `/mcp`; fix whatever has bit-rotted while it was unbuilt
- [ ] 1.3 Confirm the baseline is meaningful: with 1.1 and 1.2 green on rmcp 0.8, verify the suite actually exercises `ServerHandler`, `get_info`, `list_tools`, and a real request through `StreamableHttpService` + `LocalSessionManager` — add coverage for any of those four still untouched, since they are precisely what rmcp 1.x changed

## 2. Migrate to rmcp 2.2.0
- [ ] 2.1 Bump root `Cargo.toml:56` to rmcp 2.2.0 and drop the unused `macros` feature (no `#[tool]` / `#[tool_router]` / `#[tool_handler]` is used anywhere — the trait is implemented by hand); keep `server` + `transport-streamable-http-server`
- [ ] 2.2 Rewrite the 9 `rmcp::model::Tool { .. }` struct literals in `api/streaming/tools.rs` (`:9, :38, :74, :102, :145, :169, :210, :243, :276`) to constructor/setter form — `#[non_exhaustive]` makes cross-crate literal construction a hard error, including `..Default::default()`. Same for the `ToolAnnotations::new().read_only(..)` sites
- [ ] 2.3 Rewrite the literals in `api/streaming/service.rs`: `ServerInfo` / `Implementation` (`:33-46`), `ListToolsResult` (`:56-59`), `ListResourcesResult` (`:75-78`)
- [ ] 2.4 Resolve the `ServerHandler` future-bound change (`+ Send` → `+ MaybeSendFuture`, `rmcp-1.5.0/src/handler/server.rs:217,:265,:272`) — expected to be satisfied by `async fn` under the default feature set, but this was HYPOTHESIS at investigation time; confirm by compiling and fix explicitly if it does not hold
- [ ] 2.5 Verify the non-breaking items actually held: param-type aliases (`CallToolRequestParam`/`PaginatedRequestParam`) still resolve, and `StreamableHttpService` type-param inference at `main.rs:1123` still compiles without annotation. Fix `handlers.rs` / `dispatcher.rs` call sites (~30 `ErrorData::*` and `CallToolResult::success` uses) only where the compiler actually objects — do not churn working code

## 3. Validate the migration
- [ ] 3.1 Re-run the section 1 suites against rmcp 2.2.0 — the transport-layer tests from 1.2/1.3 are the ones that matter; compile success alone is not acceptance
- [ ] 3.2 Verify against a real MCP client end-to-end over `/mcp` (initialize → list_tools → call_tool), and check whether the advertised `ProtocolVersion` changed between rmcp 0.8 and 2.2 — if it did, that is client-visible and must be called out in the release notes
- [ ] 3.3 Confirm the advisory clears: `cargo audit` reports no RUSTSEC-2026-0189, and no rmcp 0.8.x remains in `Cargo.lock`

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation (`docs/specs/api-protocols.md` MCP section if the advertised protocol version or tool schema changed; CHANGELOG entry noting the security remediation and referencing RUSTSEC-2026-0189)
- [ ] 4.2 Write tests covering the new behavior (sections 1.1–1.3 are the deliverable here — the previously dead `streaming/tests.rs` and the previously unbuilt `api_integration_test.rs` must both be compiled, running, and part of the default suite so the `/mcp` surface cannot regress unobserved again)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace`, plus `cargo audit` clean per 3.3)
