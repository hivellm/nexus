# Proposal: phase7_reconcile-version-strings

## Why

Version-string drift across the repo signals release-process incoherence to outside readers. Current state at 2026-04-28: README badge says **v1.13.0**, top `CHANGELOG.md` entry is **v1.2.0**, all 6 SDKs are at **v1.15.0**, and `nexus-protocol` crate is at **v1.14.0**. None of these are necessarily wrong individually (SDKs ride a faster cadence than the engine on purpose) but the public-facing surface is confusing and there is no documented mapping. Outsiders cannot tell which server version a given SDK targets, and contributors cannot tell which version string to bump on a PR.

## What Changes

- Decide and document the canonical release-train policy in `docs/development/RELEASE_PROCESS.md`: server (`nexus-core`/`nexus-server`/`nexus-cli`) + protocol + SDK version trains, the rules that bind them, and the SemVer compatibility matrix.
- Reconcile observable surfaces for the next release cut:
  - README badge matches latest published server tag.
  - Top `CHANGELOG.md` entry matches the same server version.
  - Each SDK README states "compatible with server ≥ X.Y" and "uses protocol ≥ A.B".
- Add a `docs/COMPATIBILITY_MATRIX.md` listing server↔SDK↔protocol versions per release.
- Add a CI step that fails the build if README badge ≠ Cargo.toml workspace version ≠ top CHANGELOG entry.

## Impact

- Affected specs: `docs/development/RELEASE_PROCESS.md` (new), `docs/COMPATIBILITY_MATRIX.md` (new), `README.md`, `CHANGELOG.md`, every `sdks/*/README.md`.
- Affected code: CI workflow, version-check script under `scripts/ci/`.
- Breaking change: NO.
- User benefit: predictable release cadence; integrators can pick correct SDK for their server.
