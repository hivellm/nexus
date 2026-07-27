# Nexus SDK release runbook

The four in-repo SDKs — Rust, TypeScript, Python, C# — ship as **one release
train**: a single published GitHub Release fans out to all four registries via
OIDC **Trusted Publishing**, so there is no publishing token anywhere in this
repository. This document is the one-time owner setup and the per-release
procedure.

Workflows:

- `.github/workflows/sdk-release.yml` — the train (gate → 4 publishers → verify),
  triggered by a published GitHub Release.
- `.github/workflows/sdk-release-train.yml` — weekly drift check (the registries
  must agree with each other between releases).
- `scripts/ci/check_published_sdk_versions.py` — the version checker both use.

The Go and PHP SDKs are **out of scope** (moving to their own repositories); they
are not part of this train.

## Registries and package names

| Language   | Registry  | Package             | Directory        |
|------------|-----------|---------------------|------------------|
| Rust       | crates.io | `nexus-graph-sdk`   | `sdks/rust`      |
| TypeScript | npm       | `@hivehub/nexus-sdk`| `sdks/typescript`|
| Python     | PyPI      | `hivehub-nexus-sdk` | `sdks/python`    |
| C#         | NuGet     | `Nexus.SDK`         | `sdks/csharp`    |

## One-time owner setup (do this BEFORE the first release)

Until every registry's trusted publisher is registered, that lane's publish job
fails auth. Do a `workflow_dispatch` dry run first (see below) and expect the
gate green and the publish lanes to fail-auth until this is complete.

### 1. Create four GitHub environments

Repo → Settings → Environments → create **`crates`**, **`npm`**, **`pypi`**,
**`nuget`** (names must match the `environment:` field of each job). Optionally
add required reviewers to each so a human approves before any publish runs.

### 2. Register the trusted publisher on each registry

Everywhere the values are: **owner `hivellm`**, **repository `nexus`**, **workflow
file `sdk-release.yml`** (file name only, no path), and the **matching
environment**.

- **crates.io** → <https://crates.io/crates/nexus-graph-sdk/settings> → Trusted
  Publishing → GitHub → environment `crates`.
- **npm** → <https://www.npmjs.com/package/@hivehub/nexus-sdk/access> → Trusted
  Publishers → environment `npm`, allowed action `npm publish`. (Needs the
  package to exist; if it does not yet, do the first npm publish once by hand,
  then switch to trusted publishing.)
- **PyPI** → <https://pypi.org/manage/project/hivehub-nexus-sdk/settings/publishing/>
  → environment `pypi`. (For a brand-new project use PyPI's "pending publisher"
  form so the first automated publish creates it.)
- **NuGet** → <https://www.nuget.org/account/trustedpublishing> → add a policy for
  repository `hivellm/nexus`, workflow `sdk-release.yml`, environment `nuget`.

### 3. Set the `NUGET_USER` repo secret

NuGet's OIDC login needs the nuget.org **profile name** (not an email, not a
credential). Add it as a repo secret named `NUGET_USER` so the workflow need not
be edited if the account changes. (It lives in a secret only for that
convenience — it is not sensitive.)

### 4. crates.io prerequisite — `nexus-protocol`

`nexus-graph-sdk` depends on the workspace crate `nexus-protocol` via a path dep
that also carries `version = "<x>"`. `cargo publish` resolves that dependency
**from crates.io**, so `nexus-protocol <x>` must already be published there or the
crates lane fails with *"no matching package named `nexus-protocol`"*. Choose one
before the first release:

- **Publish `nexus-protocol` to crates.io** (it becomes a public crate; add a
  publish step ahead of the SDK in the `crates` job, or release it separately),
  **or**
- **Drop the path/RPC dependency** from `sdks/rust/Cargo.toml` (e.g. feature-gate
  the native RPC transport so the default SDK depends only on registry crates).

This is a real decision about whether `nexus-protocol` is a public API surface —
it must be made before the Rust lane can publish.

## Per-release procedure

1. **Bump all four SDK manifests to the same version** `X.Y.Z`:
   - `sdks/rust/Cargo.toml` → `version`
   - `sdks/typescript/package.json` → `version`
   - `sdks/python/pyproject.toml` → `version`
   - `sdks/csharp/Nexus.SDK.csproj` → `<Version>`
2. Merge that bump.
3. **Publish a GitHub Release** whose tag resolves to `X.Y.Z` (the trigger is the
   Release being *published*, not a bare tag push — this mirrors
   `release-server.yml`). The tag may be `vX.Y.Z`, `sdk-vX.Y.Z`, or `server-vX.Y.Z`;
   the gate strips the prefix and compares the version to all four manifests, and
   **fails the whole train on any mismatch** — so a release whose version does not
   match the SDK manifests is caught, never shipped.

The gate runs fmt/lint/tests for all four SDKs on the released commit; only if it
is green do the four publish jobs run (in parallel), each authenticating by OIDC;
then `verify` waits for the registries to settle and asserts all four report
`X.Y.Z`.

### Dry run

Run `sdk-release.yml` via **Run workflow** (`workflow_dispatch`) with an explicit
`tag`. The gate exercises the full quality bar; the publish lanes reach the
registries only if trusted publishing is registered. Use this to validate the
gate before the first real release.

## What the checks guarantee

- **Gate** — nothing publishes unless fmt/lint/tests pass on the released commit
  and the release version matches every manifest.
- **verify** (post-publish, `always()`) — reports which registries actually have
  the release, so a failed lane is visible rather than silently leaving the set
  inconsistent.
- **drift** (weekly) — the registries must agree with each other; the repo being
  ahead between releases is normal and passes.
