# Publishing `nexus-graph-sdk` to crates.io

`nexus-graph-sdk` depends on `nexus-protocol` (the native RPC
codec), so the two must be published in strict order — SDK uploads
abort with `no matching package named 'nexus-protocol' found` when
the protocol crate is missing from the registry at the SDK's
declared version.

The SDK is published as **`nexus-graph-sdk`** on crates.io. The
short name `nexus-sdk` is already owned by the unrelated Nexus
Workflow project (placeholder 0.0.0). The module name remains
`nexus_sdk` so every downstream `use nexus_sdk::...;` still
compiles after upgrading the crate name.

## Order (do NOT re-order)

1. **Publish `nexus-protocol` first.** From the repo root:

   ```bash
   cd crates/nexus-protocol
   cargo +nightly publish
   ```

2. **Wait for index propagation.** Cargo's registry index is
   eventually-consistent; 30 s is usually enough. Confirm:

   ```bash
   cargo search nexus-protocol | head -1
   # nexus-protocol = "2.0.0"    # Integration protocols for Nexus - REST, MCP, UMICP
   ```

3. **Publish `nexus-graph-sdk`.**

   ```bash
   cd sdks/rust
   cargo +nightly publish
   ```

## Version bumps

Both crates must ship the same version (the SDK pins
`nexus-protocol = "X.Y.Z"` explicitly). Bump in this order:

1. `Cargo.toml` root `[workspace.package] version = "X.Y.Z"` —
   picked up by `nexus-protocol` via `version.workspace = true`.
2. `sdks/rust/Cargo.toml` top-level `version = "X.Y.Z"` **AND**
   the `nexus-protocol = { ..., version = "X.Y.Z" }` pin.

`cargo publish --dry-run --allow-dirty` in `crates/nexus-protocol`
validates the workspace side before any actual upload.

## Why both `path` and `version` on the protocol dep?

```toml
nexus-protocol = { path = "../../crates/nexus-protocol", version = "2.0.0" }
```

- `path` makes in-workspace builds (and `cargo test`) pick the
  local source, so the SDK tracks protocol changes without
  round-tripping through crates.io.
- `version` is what ends up in the published `Cargo.toml` —
  consumers pulling `nexus-graph-sdk` from crates.io get
  `nexus-protocol@2.0.0` resolved from the registry.

Either half alone breaks:

- No `path` → `cargo check` in the workspace fetches the last
  published protocol from crates.io and ignores local fixes.
- No `version` → `cargo publish` refuses the SDK with
  "all path dependencies must have a version specified".

## Authenticating

`cargo login <token>` once on the host; the token lives in
`~/.cargo/credentials.toml` afterwards. The token needs "publish
new crates" scope the first time and "publish updates" thereafter.

## First-time publish checklist

- [ ] `cargo +nightly fmt --all` clean.
- [ ] `cargo +nightly clippy -p nexus-protocol -- -D warnings` clean.
- [ ] `(cd sdks/rust && cargo +nightly clippy -- -D warnings)` clean.
- [ ] `(cd crates/nexus-protocol && cargo +nightly publish --dry-run --allow-dirty)` passes.
- [ ] Version bumped in both `Cargo.toml` and `sdks/rust/Cargo.toml`.
- [ ] CHANGELOG entry mentions both crates under the same version.
- [ ] Publish protocol, wait 30 s, publish SDK.
