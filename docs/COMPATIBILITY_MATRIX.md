# Nexus Compatibility Matrix

Cross-reference for SDK ↔ server ↔ protocol versions.
Maintained per release; see
[`docs/development/RELEASE_PROCESS.md`](development/RELEASE_PROCESS.md)
for the bump procedure.

## Current (2026-04-28)

| Surface             | Version | Notes                                                 |
|---------------------|---------|-------------------------------------------------------|
| Workspace crate     | 1.15.0  | `Cargo.toml` `[workspace.package].version`            |
| `nexus-server`      | 1.15.0  | Docker image + binary; mirrors workspace              |
| `nexus-cli`         | 1.15.0  | mirrors workspace                                     |
| `nexus-protocol`    | 1.15.0  | RPC + HTTP wire format                                |
| Rust SDK            | 1.15.0  | `nexus-sdk` on crates.io                              |
| Python SDK          | 1.15.0  | `hivehub-nexus-sdk` on PyPI                           |
| TypeScript SDK      | 1.15.0  | `@hivehub/nexus-sdk` on npm                           |
| Go SDK              | 1.15.0  | `github.com/hivellm/nexus-go`                         |
| C# SDK              | 1.15.0  | `Nexus.SDK` on NuGet                                  |
| PHP SDK             | 1.15.0  | `hivellm/nexus-php` on Packagist                      |
| Neo4j diff suite    | 300/300 | reference Neo4j 2025.09.0 (verified 2026-04-19)       |
| openCypher TCK spatial | 22/22 | Nexus-authored corpus (no upstream spatial features) |

**Single train policy:** all SDKs and the server move in lockstep
on the same X.Y.Z. An SDK at version A.B.C is required to speak
to a server at A.B.C; older / newer SDKs are best-effort and may
miss recently-added wire fields or refuse a removed one. The CI
gate `scripts/ci/check_version_consistency.sh` enforces lockstep
on every push.

## Compatibility rules

- **SDK ↔ server** — match `MAJOR.MINOR.PATCH` exactly for
  guaranteed full feature parity. Mismatches within the same
  `MAJOR.MINOR` are usually fine (patch-level fixes only) but the
  test matrix only validates exact-match.
- **Protocol** — `nexus-protocol` carries a wire-format version
  byte in every RPC frame; servers and SDKs negotiate down to the
  highest commonly understood version on connect.
- **Bolt shim** — when shipped (see
  `phase8_bolt-protocol-shim`), the shim will independently
  advertise the Bolt protocol version it speaks (target: Bolt
  v5).
- **Persisted data** — record-store + WAL formats are
  forward-compatible within the same `MAJOR`. Cross-major
  upgrades require the documented migration script.

## Older releases

| Date       | Version | Status     |
|------------|---------|------------|
| 2026-04-28 | 1.15.0  | current    |
| 2026-04-22 | 1.13.0  | superseded |
| 2026-03-?? | 1.0.0   | superseded |

For per-version detail see [`CHANGELOG.md`](../CHANGELOG.md).
