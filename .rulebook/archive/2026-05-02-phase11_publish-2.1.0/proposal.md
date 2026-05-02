# Proposal: phase11_publish-2.1.0

## Why

Phases 9 and 10 land the external-node-id surface across the engine, REST, every SDK, and a live-validated test grid (87 live cases passing). The release commit `7403cf43` set the workspace to `2.1.0` and rewrote the top-level `CHANGELOG.md` accordingly — but the SDK manifests drifted during phase 10 (Python and TypeScript bumped to `2.1.1`; C# and PHP bumped to `2.2.0`). Two pre-phase-9 SDK regressions also still ship: PHP and C# `createNode` post to `/nodes`, which the server retired in favour of `/data/nodes` long before phase 9, so any caller of the legacy method receives an unconditional 404 from a 2.1.0 release.

A `2.1.0` release tag right now would publish:
- A workspace at `2.1.0`.
- Rust SDK at `2.1.0`.
- A Python wheel claiming `2.1.1`.
- An npm tarball claiming `2.1.1`.
- A NuGet package claiming `2.2.0`.
- A composer package claiming `2.2.0`.
- A Go module with no `v2.1.0` git tag at all.
- Two SDKs (PHP, C#) whose default `createNode` is broken against the server they're shipped alongside.

That release is non-coherent. Phase 11 makes it coherent.

## What Changes

A focused publish-readiness pass that:

1. Realigns every SDK manifest on `2.1.0` (workspace = Rust = npm = pypi = nuget = composer; Go gets a `v2.1.0` git tag).
2. Fixes the legacy `/nodes` → `/data/nodes` mismatch in `sdks/php/src/NexusClient.php` and `sdks/csharp/NexusClient.cs` so `createNode` works against a 2.1.0 server. Adds a unit + live regression test in each SDK.
3. Runs a publish-dry-run for every SDK and captures the manifest diff (size, included paths, repository/homepage URLs, LICENSE inclusion).
4. Builds and tags the Docker image at `nexus-nexus:2.1.0` plus the helm chart's `appVersion` (currently `0.2.0` chart version, but `appVersion` should track 2.1.0).
5. Re-runs the full live-SDK suite (`bash scripts/sdks/run-live-suites.sh`) against the tagged image, captures results into `sdks/PHASE10_LIVE_RESULTS.md` (refresh the timing table), and confirms 6/6 PASS.
6. Writes a release checklist (`docs/development/RELEASE_2.1.0.md`) that codifies the publish order and the smoke-test commands so the actual `cargo publish` / `npm publish` / etc. is a documented push-button operation.

The actual publish-to-public-registry step is intentionally NOT inside this phase — it requires registry credentials and is a destructive (irreversible) action. Phase 11 lands the artefacts and the documented procedure; the human operator pushes the buttons.

## Impact

- **Affected specs**:
  - `docs/development/RELEASE_PROCESS.md` — extend with the 6-SDK matrix and the 2.1.0-specific gating order.
  - `docs/development/RELEASE_2.1.0.md` (new) — the per-release checklist.

- **Affected code**:
  - `sdks/python/pyproject.toml` — version `2.1.1` → `2.1.0`.
  - `sdks/typescript/package.json` — version `2.1.1` → `2.1.0`.
  - `sdks/csharp/Nexus.SDK.csproj` — `<Version>2.2.0</Version>` → `2.1.0`.
  - `sdks/php/composer.json` — `version` `2.2.0` → `2.1.0`.
  - `sdks/python/CHANGELOG.md`, `sdks/typescript/CHANGELOG.md`, `sdks/csharp/CHANGELOG.md`, `sdks/php/CHANGELOG.md` — reconcile the section heading with the new version.
  - `sdks/php/src/NexusClient.php` — `createNode` POSTs to `/data/nodes`; `getNode`/`updateNode`/`deleteNode` use the `?id=` query form the server actually exposes.
  - `sdks/csharp/NexusClient.cs` — same fix on the C# side.
  - `sdks/php/tests/NexusClientTest.php`, `sdks/csharp/Tests/ExternalIdTests.cs` — regression unit tests covering the corrected paths.
  - `sdks/php/tests/CreateNodeLiveTest.php`, `sdks/csharp/Tests/CreateNodeLiveTests.cs` — live tests confirming `createNode` round-trips against a real server (additive — does not touch existing `ExternalId*` suites).
  - `deploy/helm/nexus/Chart.yaml` — `appVersion: 2.1.0` (chart version stays `0.2.0` per the existing helm convention).
  - `Dockerfile` / build pipeline — confirm the produced image, when tagged, reads `2.1.0` via `/health`.

- **Breaking change**: NO for users who already use `_id` helpers. **YES for users of PHP / C# `createNode`** because the previous behaviour was 404 → unusable; the fix makes it work, but the response shape changes from `NexusApiException(404)` to a real `Node`. Anyone whose code depends on the 404 is depending on a bug; we document this in the release notes as a fix, not a breakage.

- **User benefit**:
  - Coherent `2.1.0` release across every published artefact.
  - PHP and C# SDK users get a working `createNode` instead of a silent 404.
  - Public smoke-test procedure that any future contributor can re-run before cutting `2.x.0`.

## Source

- Release commit `7403cf43` (workspace bump to 2.1.0).
- `sdks/PHASE10_LIVE_RESULTS.md` (the "Pre-existing issues observed but out of scope" section enumerates the `/nodes` mismatch).
- `crates/nexus-server/src/main.rs:638-645` (the only registered `/data/nodes` routes — confirms `/nodes` is not exposed).
- `sdks/php/src/NexusClient.php:165` and `sdks/csharp/NexusClient.cs:186` (the broken POST sites).
