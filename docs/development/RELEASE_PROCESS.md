# Release Process

**Last updated:** 2026-04-28 (`phase7_reconcile-version-strings`).

This document is the canonical reference for how Nexus versions
move. It binds the workspace crate version, the SDK package
versions, the protocol crate version, the README badge, the
`CHANGELOG.md` heading, and the release branch name into one
coherent surface so a contributor never has to guess "which
version is the project on?".

## Trains

Nexus ships **one canonical version train** for the Rust workspace
+ all six SDKs:

| Surface | Source of truth | Current value |
|---------|-----------------|---------------|
| Workspace crate version | `Cargo.toml` `[workspace.package].version` | `2.0.0` |
| `nexus-server` Docker image | server release workflow → mirrors workspace | `2.0.0` |
| Rust SDK (`nexus-sdk` on crates.io) | `sdks/rust/Cargo.toml` | `2.0.0` |
| Python SDK (`hivehub-nexus-sdk` on PyPI) | `sdks/python/pyproject.toml` | `2.0.0` |
| TypeScript SDK (`@hivehub/nexus-sdk` on npm) | `sdks/typescript/package.json` | `2.0.0` |
| Go SDK (`github.com/hivellm/nexus-go`) | `sdks/go/go.mod` + git tag | `2.0.0` |
| C# SDK (`Nexus.SDK` on NuGet) | `sdks/csharp/Nexus.SDK.csproj` | `2.0.0` |
| PHP SDK (`hivellm/nexus-php` on Packagist) | `sdks/php/composer.json` | `2.0.0` |
| `nexus-protocol` crate | `crates/nexus-protocol/Cargo.toml` | tracks workspace |
| README status badge | `README.md` line 8 | `v2.0.0` |
| `CHANGELOG.md` top heading | `## [X.Y.Z] — YYYY-MM-DD` | `[2.0.0] — 2026-04-30` |

The CI gate `scripts/ci/check_version_consistency.sh` verifies the
top three (workspace, README badge, CHANGELOG heading) match on
every push and PR. Drift is a hard failure.

## Branch naming

Release branches use the pattern `release/vX.Y` (minor train, no
patch). The branch name **may lag** the workspace version when a
multi-minor cycle ships under one branch (e.g. the
`release/v1.2.0` branch carries `1.13`–`1.15` workspace bumps as
the SDK train accelerated past the marketing minor). When this
happens it is documented in `CHANGELOG.md` next to the affected
heading.

A new branch is cut whenever the workspace version crosses a major
boundary (`X` bump) or whenever the SDK train falls more than two
minor versions out of sync with the branch name.

## Bumping

1. **Pick the bump kind** — `patch` (0.0.x), `minor` (0.x.0), or
   `major` (x.0.0). Apply SemVer: breaking public-API changes are
   always major; new public APIs are always minor; bug-fixes only
   are patch.
2. **Bump in lockstep**:
   - `Cargo.toml` `[workspace.package].version`
   - Every SDK manifest (`sdks/*/Cargo.toml`, `pyproject.toml`,
     `package.json`, `go.mod`, `*.csproj`, `composer.json`)
   - README badge URL (line 8) and the "Highlights (vX.Y.Z)"
     heading + any "Roadmap → X.Y.Z — current" subsection title
   - `CHANGELOG.md` — new top heading `## [X.Y.Z] — YYYY-MM-DD`
3. **Run** `bash scripts/ci/check_version_consistency.sh` locally.
   If it fails, fix the missing surface before committing.
4. **Tag** the release commit `vX.Y.Z` after the merge lands on
   the release branch.
5. **Publish** SDK packages via the dedicated workflows
   (`release-cli.yml`, `release-server.yml`, plus per-SDK
   workflows). Each workflow already pins the version it reads
   from the matching manifest.

## Pre-1.0 vs post-1.0

Nexus is post-1.0 (`1.x`). Every minor bump is feature-additive
and source-compatible for the public Cypher surface and the SDK
APIs. Patch bumps are bug-fix-only. Major bumps require an ADR.

## Compatibility matrix

A live cross-reference of which SDK versions speak to which
server versions lives in
[`docs/COMPATIBILITY_MATRIX.md`](../COMPATIBILITY_MATRIX.md).

## Why one train

Earlier releases experimented with two trains (a "marketing"
minor for the server image and a faster minor for the SDKs) but
the cost of the divergence — confused integrators, mismatched
release notes, branch names that lag the engine — was higher
than the benefit. From `phase7_reconcile-version-strings` (2026-04-28)
onward, the workspace + SDK + CHANGELOG + README badge all march
in lockstep. The branch-name lag is the one residual artefact
and is acceptable because Git references are public history.
