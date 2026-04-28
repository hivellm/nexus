# Proposal: phase6_sdk-rust-publish-1.15.0

Source: follow-up to `phase6_sdk-rust-getnode-listlabels-drift`
(GitHub issue #2)

## Why

`phase6_sdk-rust-getnode-listlabels-drift` landed all code-side fixes
for issue #2 (server `get_node` round-trip, `LabelInfo`/`RelTypeInfo`
typed structs across all SDKs, `/health` version pin test, SDK
`Cargo.toml` bump to `1.15.0`, CHANGELOG entry). Three external
publish/coordination items remained gated on operator credentials and
were carved out into this task so the parent could archive cleanly:

1. The rebuilt `hivehub/nexus:v1.14.0` Docker image still needs to be
   pushed so the live container's `/health` reports `1.14.0` instead
   of the stale `1.13.0` the issue reporter saw.
2. `nexus-graph-sdk = "1.15.0"` needs `cargo publish` so downstream
   consumers (notably `hivellm/cortex`) can drop the
   `MATCH (n) WHERE id(n) = $id` workaround.
3. The issue reporter (#2) needs to be notified once 1 and 2 land so
   they can verify the fix end-to-end and we can close the issue.

These are operator actions — they require Docker Hub credentials,
crates.io OTP, and a GitHub comment, none of which an autonomous
agent should execute without explicit per-publish authorization.

## What Changes

### 1. Re-build and re-publish `hivehub/nexus:v1.14.0`

- Verify workspace `Cargo.toml` is `1.14.0` (or higher) at the
  release commit so `env!("CARGO_PKG_VERSION")` bakes the right
  string.
- `docker buildx build --push --tag hivehub/nexus:v1.14.0 .` from a
  clean `main` checkout.
- Smoke-test: `docker run --rm -p 15474:15474 hivehub/nexus:v1.14.0`,
  then `curl http://localhost:15474/health` and assert `version` in
  the response equals the binary's `CARGO_PKG_VERSION`.

### 2. Publish `nexus-graph-sdk@1.15.0` to crates.io

- `cd sdks/rust && cargo publish --dry-run` first to surface any
  packaging issues without consuming the version slot.
- `cargo publish` once the dry-run is clean. Requires a crates.io
  API token with publish scope on `nexus-graph-sdk`.
- Tag the release commit `sdk-rust-v1.15.0` for traceability.

### 3. Close out issue #2

- Comment on https://github.com/hivellm/nexus/issues/2 with: the
  three fixes that landed, the crates.io version (`1.15.0`), the
  rebuilt image tag, and a code snippet showing the new
  `LabelInfo`/`RelTypeInfo` iteration shape.
- Ask the reporter to verify against their `hivellm/cortex`
  integration; close the issue once they confirm.

## Impact

- **Affected specs**: none — pure release/coordination work.
- **Affected code**: none. The release commit is whatever is on
  `main` at publish time.
- **Breaking change**: the SDK bump from `1.14.0` -> `1.15.0` is
  semver-major-equivalent under 1.x because `Vec<(String, u32)>`
  became `Vec<LabelInfo>` / `Vec<RelTypeInfo>`. Migration snippet
  is already in `sdks/rust/CHANGELOG.md` `[1.15.0]`.
- **User benefit**: `hivellm/cortex` and other downstream consumers
  can pull `nexus-graph-sdk = "1.15.0"` and drop their `get_node`
  workaround.

## Source

- Parent task: `.rulebook/tasks/phase6_sdk-rust-getnode-listlabels-drift/`
- GitHub issue: https://github.com/hivellm/nexus/issues/2
