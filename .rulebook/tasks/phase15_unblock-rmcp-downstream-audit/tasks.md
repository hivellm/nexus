# Tasks: phase15_unblock-rmcp-downstream-audit

Close issue #28 by removing the dead `rmcp` dependency from the published
`nexus-protocol` crate, unblocking downstream `cargo audit` on RUSTSEC-2026-0189.
No API migration — `nexus-protocol` never imports rmcp. The server-side migration
(the actual runtime exposure) is phase16, deliberately decoupled.

## 1. Remove the dead dependency
- [ ] 1.1 Re-confirm rmcp is unused in `nexus-protocol` before deleting (`grep -rn rmcp crates/nexus-protocol/`) — expect hits only in `Cargo.toml:37` and `README.md:14`, none in `src/`
- [ ] 1.2 Delete `rmcp = { version = "0.8.1", features = ["client", "macros"] }` from `crates/nexus-protocol/Cargo.toml:37`; leave the workspace pin at root `Cargo.toml:56` untouched (it serves `nexus-server`, which is unpublished and handled in phase16)
- [ ] 1.3 Verify removal: `cargo build -p nexus-protocol` succeeds unchanged, and `cargo tree -p nexus-graph-sdk -i rmcp` (run in `sdks/rust`) reports rmcp is no longer in the graph; regenerate `sdks/rust/Cargo.lock`
- [ ] 1.4 Fix `crates/nexus-protocol/README.md:14` — it claims MCP is provided "via `rmcp`"; the client is hand-rolled over reqwest (`src/mcp.rs`)

## 2. Release
- [ ] 2.1 Bump the version in root `Cargo.toml [workspace.package]` and in `sdks/rust/Cargo.toml` (both the package version and the `nexus-protocol` pin at `:32`) — patch bump is correct, this is not semver-visible
- [ ] 2.2 Add a `sdks/rust/CHANGELOG.md` entry; the file's latest entry is `[2.1.0] — 2026-05-02` and is stale relative to the published 2.5.0, so cover the gap rather than implying 2.5.x was documented
- [ ] 2.3 Publish `nexus-protocol`, wait for the crates.io index, then publish `nexus-graph-sdk`, per `sdks/rust/PUBLISH.md:17-31`
- [ ] 2.4 Verify downstream is actually unblocked: from a clean checkout, resolve `nexus-graph-sdk` at the new version alongside `rmcp >= 1.4.0` and confirm `cargo audit` no longer reports RUSTSEC-2026-0189 through the Nexus path; report the result on issue #28 and close it

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 3.1 Update or create documentation covering the implementation (`crates/nexus-protocol/README.md` from 1.4; `sdks/rust/CHANGELOG.md` from 2.2; root CHANGELOG entry referencing issue #28)
- [ ] 3.2 Write tests covering the new behavior (no behavioural change to test — the guard that matters is a dependency assertion: add a CI check that fails if `rmcp` reappears in the published `nexus-protocol`/`nexus-graph-sdk` dependency graph, so the leak cannot silently return)
- [ ] 3.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` — the whole workspace must still build with `nexus-server` keeping its own rmcp dependency)
