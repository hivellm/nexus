# Proposal: phase15_unblock-rmcp-downstream-audit

Closes: https://github.com/hivellm/nexus/issues/28
**Priority: HIGH, effort: very low — one deleted line plus a republish.**
Should be executed before the Thunder migration (phase10–13).

## Why

Cortex's `cargo audit` gate flags **RUSTSEC-2026-0189** on `rmcp` (patched in
>= 1.4.0). `nexus-protocol` 2.5.0 declares `rmcp = ^0.8.1` non-optionally, and the
caret range excludes 1.x, so any downstream lockfile depending on
`nexus-graph-sdk` 2.5.0 cannot resolve a patched rmcp. The issue asks for a bump to
rmcp >= 1.4.0 and notes it is a breaking 0.8 → 1.x API change.

**That migration is not necessary to close this issue. `nexus-protocol` declares
rmcp but never imports it — it is a dead dependency.** CONFIRMED: `grep -rn rmcp
crates/nexus-protocol/src` returns zero hits. `crates/nexus-protocol/src/mcp.rs` is a
hand-rolled `reqwest` JSON-RPC client, and the file says so itself at `:3-4`:
*"RMCP client types are not available in the current version / We'll implement a
simplified MCP client for now."* The public surface `McpClient` / `McpClientError`
(`crates/nexus-protocol/src/lib.rs:18`) exposes no rmcp types — only
`serde_json::Value`, `reqwest::Error`, and `String`. Deleting the declaration is not
even semver-visible.

The leak path is a single line, and notably **not** the workspace pin the issue
cites. Workspace `Cargo.toml:56` feeds `nexus-server`, which is an unpublished binary
and never reaches crates.io. The published path is a separate hardcoded declaration:

```
nexus-graph-sdk 2.5.0  (sdks/rust/Cargo.toml:32)
  └─ nexus-protocol 2.5.0
       └─ rmcp ^0.8.1   ← crates/nexus-protocol/Cargo.toml:37  (hardcoded, not .workspace)
```

Confirmed against `sdks/rust/Cargo.lock:890-903`, which lists rmcp among
`nexus-protocol`'s dependencies.

You do not feature-gate a dependency you never call — you delete it. Cortex's audit
clears immediately because rmcp leaves their graph entirely.

## What Changes

- Delete `crates/nexus-protocol/Cargo.toml:37` (`rmcp = { version = "0.8.1",
  features = ["client", "macros"] }`). No source change.
- Fix `crates/nexus-protocol/README.md:14`, which still claims MCP is provided "via
  `rmcp`" — it is not; the client is hand-rolled over reqwest.
- Version bump and republish `nexus-protocol` then `nexus-graph-sdk` per
  `sdks/rust/PUBLISH.md` (publish protocol first, wait for the index, then the SDK;
  both carry the same version, bumped in root `Cargo.toml [workspace.package]` and in
  `sdks/rust/Cargo.toml` — both the package version and the `nexus-protocol` pin at
  `:32`).
- Add a `sdks/rust/CHANGELOG.md` entry. That file's latest entry is `[2.1.0] —
  2026-05-02`, stale relative to the published 2.5.0, so the release note should cover
  the gap rather than pretend 2.5.x was documented.

### Relationship to the Thunder migration

`nexus-protocol` is scheduled for **deletion** in phase11 — after Thunder, Nexus keeps
no shared protocol crate, and anything the server and the Rust SDK both need is copied
into each side independently (Synap's model). That deletion permanently removes this
dependency path and supersedes this task's fix.

This task still ships first, and separately: it is a one-line deletion plus a
republish, Cortex is blocked *now*, and phase11 is several phases out. Just do not
invest in `nexus-protocol` beyond what is listed here — no refactors, no new surface.

### Explicitly NOT in scope

`nexus-server` still runs the vulnerable rmcp 0.8.3 at runtime, which is a real
exposure this change does not address — it only unblocks downstream consumers. That
remediation is the genuine 0.8 → 2.x API migration and is tracked separately as
**phase16_rmcp-server-migration**, which has test-coverage prerequisites of its own.
Do not couple the two: this task is a dead-code deletion that can ship today, and
blocking it on the server migration would leave Cortex blocked for no reason.

## Impact

- Affected specs: none
- Affected code: `crates/nexus-protocol/Cargo.toml` (one line deleted),
  `crates/nexus-protocol/README.md`, root `Cargo.toml` + `sdks/rust/Cargo.toml`
  (version bump), `sdks/rust/CHANGELOG.md`
- Breaking change: NO — no rmcp type is in any published public API, so this is not
  semver-visible.
- User benefit: downstream `cargo audit` gates (Cortex `phase28_live-testing-bugfixes`
  §1.5, currently blocked) unblock immediately; Nexus stops shipping a dependency it
  does not use.

## References

- Issue: https://github.com/hivellm/nexus/issues/28
- The one offending line: `crates/nexus-protocol/Cargo.toml:37`
- Proof it is unused: `crates/nexus-protocol/src/mcp.rs:3-4` (the crate's own comment),
  `crates/nexus-protocol/src/lib.rs:18` (public surface, no rmcp types)
- Leak proof: `sdks/rust/Cargo.lock:890-903`
- Release procedure: `sdks/rust/PUBLISH.md:17-31`
- Follow-up for the actual runtime exposure: phase16_rmcp-server-migration
