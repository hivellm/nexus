# Proposal: phase13_sdk-release-trusted-publishing

## Why

Nexus SDKs have per-PR test CI (`.github/workflows/sdk-*-test.yml`) but no release
pipeline — publishing to registries is manual and would require long-lived tokens
stored as secrets. Thunder already runs the model we want
(`e:\HiveLLM\Thunder\.github\workflows\release.yml`): a tag-driven release train with
**Trusted Publishing (OIDC)** on all four registries — zero stored publishing tokens,
provenance attached automatically (npm), short-lived keys minted per run (crates.io
30 min, NuGet 1 h), a single gate job blocking every publisher, and a post-publish
verify + weekly drift monitor.

**Scope: Rust (crates.io), Python (PyPI), TypeScript (npm), C# (NuGet) only.**
The PHP and Go SDKs will be extracted to their own repositories (user decision,
mirroring `thunder-go`/`thunder-php`, which publish from VCS tags in their own repos)
— they are explicitly OUT of this pipeline and out of this task's gate.

## What Changes

New `.github/workflows/sdk-release.yml` mirroring Thunder's `release.yml` DAG:

- **Trigger**: push on tag `sdk-v*` + `workflow_dispatch`. A dedicated tag pattern
  (not `v*`) because the Nexus repo also releases the server; SDK versions must be
  able to move independently. (Assumption stated — if the team prefers lockstep
  `v*` tags, only the trigger and version-check step change.)
- **Top-level `permissions: contents: read`**; each publish job re-grants
  `id-token: write` locally and uses a named GitHub environment.
- **`gate` job**: fmt/lint/tests for the 4 in-scope SDKs + "tag must match the
  manifests" check (strip `sdk-v`, compare against `sdks/rust/Cargo.toml`,
  `sdks/typescript/package.json`, `sdks/python/pyproject.toml`, the C# `.csproj`
  `<Version>`). Fail the whole train on any mismatch.
- **4 publish jobs**, `needs: gate`, parallel:
  - crates.io: `rust-lang/crates-io-auth-action@v1` → `CARGO_REGISTRY_TOKEN` →
    `cargo publish` (environment `crates`).
  - npm: Node 22 + `npm install -g npm@latest` (trusted publishing needs npm ≥ 11.5.1),
    bare `npm publish --access public` — **no** `NODE_AUTH_TOKEN`, **no** `--provenance`
    flag (attached automatically) (environment `npm`).
  - PyPI: `python -m build` + `twine check dist/*` →
    `pypa/gh-action-pypi-publish@release/v1` with `packages-dir` (environment `pypi`).
  - NuGet: `dotnet pack -c Release` → `NuGet/login@v1` (OIDC → 1-hour key, needs the
    non-secret `NUGET_USER` profile-name variable) → `dotnet nuget push` with exact
    filename guard + `--skip-duplicate` (environment `nuget`).
- **`verify` job** (`needs` all four, `if: always()`): sleep for registry settle, then
  a ported `scripts/ci/check_published_sdk_versions.py` asserts all 4 registries report
  the tag version (querying crates.io / npm / PyPI / NuGet APIs directly, handling
  yanked versions and unreachable-vs-lagging registries — port of Thunder's
  `scripts/check_published_versions.py`).
- **Weekly drift workflow** `sdk-release-train.yml` (cron + dispatch) running the same
  script in `drift` mode: registries must agree with each other between releases;
  repo being ahead is normal/passing.

Registry-side setup (manual, done by the repo owner; the task documents exact values):
4 GitHub environments (`crates`, `npm`, `pypi`, `nuget`) and 4 trusted-publisher
registrations pointing at `hivellm/nexus` + workflow filename + environment, following
Thunder's owner-setup notes.

### Risks / notes

- Registry package names/versions must be publish-ready before the first tag: crate
  name availability on crates.io, npm scope `@hivehub` (established convention),
  PyPI and NuGet names — first checklist item audits and aligns the 4 manifests.
- `cargo publish` requires the SDK crate to depend only on registry crates (no
  path/git deps) — same constraint Synap hit; verify `sdks/rust/Cargo.toml`.
- Trusted publishing must be registered on each registry BEFORE the first tagged run,
  or all four lanes fail auth; do a `workflow_dispatch` dry-run first.
- PHP/Go extraction to separate repos is out of scope here; until extracted they
  simply don't appear in this workflow (their existing test CI is untouched).

## Impact

- Affected specs: none (CI-only); `sdks/README.md` release section
- Affected code: new `.github/workflows/sdk-release.yml`, new
  `.github/workflows/sdk-release-train.yml`, new
  `scripts/ci/check_published_sdk_versions.py`, possible manifest alignment in
  `sdks/{rust,python,typescript,csharp}` (names/versions/metadata)
- Breaking change: NO
- User benefit: one-command SDK releases (`git tag sdk-vX.Y.Z && git push origin
  sdk-vX.Y.Z`) with zero stored publishing secrets, gated by lint+tests, verified
  post-publish, and drift-monitored weekly.

## References

- Thunder pipeline: `e:\HiveLLM\Thunder\.github\workflows\release.yml`,
  `release-train.yml`, `scripts\check_published_versions.py`.
- Thunder registry names for contrast: `thunder-rpc` / `@hivehub/thunder` /
  `hivellm-thunder` / `HiveLLM.Thunder`.
- Nexus existing SDK CI: `.github/workflows/sdk-{rust,python,typescript,csharp}-test.yml`.
