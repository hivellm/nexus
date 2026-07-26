# Publishing `nexus-graph-sdk` to crates.io

The SDK is published as **`nexus-graph-sdk`** on crates.io. The
short name `nexus-sdk` is already owned by the unrelated Nexus
Workflow project (placeholder 0.0.0). The module name remains
`nexus_sdk` so every downstream `use nexus_sdk::...;` still
compiles after upgrading the crate name.

**Transport note**: As of phase11, `nexus-graph-sdk` no longer
depends on `nexus-protocol` (which was removed). The RPC transport
now wraps the published `thunder-rpc` crate directly. The SDK
depends only on crates.io registry crates, so a single-step publish
suffices.

## Publishing

1. **Bump the version** in `sdks/rust/Cargo.toml` (top-level) and
   verify workspace root `Cargo.toml` if workspace versioning is in
   use.

2. **Validate before upload:**

   ```bash
   cd sdks/rust
   cargo +nightly publish --dry-run --allow-dirty
   ```

3. **Publish `nexus-graph-sdk`:**

   ```bash
   cd sdks/rust
   cargo +nightly publish
   ```

## Version bumps

Update `sdks/rust/Cargo.toml` top-level `version = "X.Y.Z"` (and the
workspace root if applicable).

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
