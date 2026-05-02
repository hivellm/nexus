## 1. Realign every SDK manifest on 2.1.0
- [ ] 1.1 `sdks/python/pyproject.toml` version 2.1.1 -> 2.1.0
- [ ] 1.2 `sdks/typescript/package.json` version 2.1.1 -> 2.1.0
- [ ] 1.3 `sdks/csharp/Nexus.SDK.csproj` `<Version>` 2.2.0 -> 2.1.0
- [ ] 1.4 `sdks/php/composer.json` version 2.2.0 -> 2.1.0
- [ ] 1.5 Reconcile each SDK's CHANGELOG so the new heading reads `[2.1.0] - 2026-05-02`
- [ ] 1.6 `deploy/helm/nexus/Chart.yaml` `appVersion: 2.1.0` (chart `version` stays per helm convention)

## 2. Fix legacy /nodes -> /data/nodes mismatch in PHP + C#
- [ ] 2.1 `sdks/php/src/NexusClient.php`: `createNode` POSTs to `/data/nodes` and returns `CreateNodeResponse` (mirroring Rust SDK shape) instead of unwrapping into `Node`
- [ ] 2.2 `sdks/php/src/NexusClient.php`: `getNode(id)` issues `GET /data/nodes?id=<id>` (the actual server route) instead of `GET /nodes/{id}`
- [ ] 2.3 `sdks/php/src/NexusClient.php`: `updateNode(id, props)` and `deleteNode(id)` use the matching `/data/nodes?id=<id>` form (server uses query-param routing for these too)
- [ ] 2.4 `sdks/csharp/NexusClient.cs`: `CreateNodeAsync` POSTs to `/data/nodes`
- [ ] 2.5 `sdks/csharp/NexusClient.cs`: `GetNodeAsync` / `UpdateNodeAsync` / `DeleteNodeAsync` use `/data/nodes?id=<id>`
- [ ] 2.6 PHP regression test in `sdks/php/tests/NexusClientTest.php` covering create -> read round-trip via the corrected route
- [ ] 2.7 C# regression test in `sdks/csharp/Tests/ExternalIdTests.cs` (or a new `NodeCrudTests.cs`) covering the same round-trip
- [ ] 2.8 Re-run the live PHP suite via the existing docker fallback and assert createNode now returns a node id, not a 404 NexusApiException
- [ ] 2.9 Re-run `dotnet test` for the C# tests and confirm all green

## 3. Publish dry-run per SDK
- [ ] 3.1 Rust: `cargo publish --dry-run -p nexus-graph-sdk` (manifest path `sdks/rust/Cargo.toml`); capture the included file list
- [ ] 3.2 Python: `python -m build` from `sdks/python/`, then `twine check dist/*`; capture wheel + sdist names and sizes
- [ ] 3.3 TypeScript: `npm pack --dry-run` from `sdks/typescript/`; capture tarball file list and size
- [ ] 3.4 Go: confirm `sdks/go/go.mod` module path matches the intended import path (`github.com/hivellm/nexus-go`); record the would-be tag (`v2.1.0`)
- [ ] 3.5 C#: `dotnet pack -c Release` from `sdks/csharp/`; verify `bin/Release/Nexus.SDK.2.1.0.nupkg` produced and contains LICENSE + README + XML docs
- [ ] 3.6 PHP: `composer validate --strict` from `sdks/php/`; confirm composer.json shape is registry-clean

## 4. Docker + helm artefact tagging
- [ ] 4.1 `docker tag nexus-nexus:latest nexus-nexus:2.1.0` and confirm `/health` reports `2.1.0` from the tagged image
- [ ] 4.2 Update `deploy/helm/nexus/Chart.yaml` `appVersion` to `2.1.0`; chart `version` is independent and stays per helm convention
- [ ] 4.3 Run any existing helm-chart smoke test (`helm lint deploy/helm/nexus/`, `helm template`) to confirm the chart still renders

## 5. End-to-end validation against the tagged image
- [ ] 5.1 Bring up `nexus-nexus:2.1.0` with auth disabled
- [ ] 5.2 Run `bash scripts/sdks/run-live-suites.sh` and assert PASS for all six SDKs
- [ ] 5.3 Refresh the per-SDK timings table in `sdks/PHASE10_LIVE_RESULTS.md` with the 2.1.0-image numbers
- [ ] 5.4 Run `python scripts/compatibility/test-external-ids-docker.py` and assert 25/25 PASS against the tagged image
- [ ] 5.5 Run `python scripts/compatibility/test-wal-replay-docker.py` and assert 9/9 PASS
- [ ] 5.6 Run `python scripts/compatibility/demo-external-ids-relationships.py` and assert 29/29 PASS

## 6. Release docs + checklist
- [ ] 6.1 Create `docs/development/RELEASE_2.1.0.md` codifying the publish order: server image -> Rust -> Python -> TypeScript -> Go -> C# -> PHP, with the exact dry-run command per SDK
- [ ] 6.2 Add the smoke-test invocations (sections 5.1-5.6) to the release checklist so the human operator runs them before pushing
- [ ] 6.3 Extend `docs/development/RELEASE_PROCESS.md` with a "Cross-SDK release" section that points at `RELEASE_2.1.0.md` as the worked example
- [ ] 6.4 Append a top-level CHANGELOG bullet to the `[2.1.0]` section noting the PHP + C# `createNode` route fix landed in this phase

## 7. Tail (mandatory - enforced by rulebook v5.3.0)
- [ ] 7.1 Update or create documentation covering the implementation
- [ ] 7.2 Write tests covering the new behavior
- [ ] 7.3 Run tests and confirm they pass
