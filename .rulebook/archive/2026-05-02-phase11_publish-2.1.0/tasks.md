## 1. Realign every SDK manifest on 2.1.0
- [x] 1.1 `sdks/python/pyproject.toml` version 2.1.1 -> 2.1.0
- [x] 1.2 `sdks/typescript/package.json` version 2.1.1 -> 2.1.0
- [x] 1.3 `sdks/csharp/Nexus.SDK.csproj` `<Version>` 2.2.0 -> 2.1.0
- [x] 1.4 `sdks/php/composer.json` version 2.2.0 -> 2.1.0
- [x] 1.5 Reconcile each SDK's CHANGELOG so the new heading reads `[2.1.0] - 2026-05-02` (already in place — only manifest drift)
- [x] 1.6 `deploy/helm/nexus/Chart.yaml` `appVersion: 2.1.0` (already in place — chart `version` 0.2.0 stays per helm convention)

## 2. Fix legacy /nodes -> /data/nodes mismatch in PHP + C#
- [x] 2.1 `sdks/php/src/NexusClient.php`: `createNode` POSTs to `/data/nodes` and returns `CreateNodeResponse` (mirroring Rust SDK shape) instead of unwrapping into `Node`
- [x] 2.2 `sdks/php/src/NexusClient.php`: `getNode(id)` issues `GET /data/nodes?id=<id>` (the actual server route) instead of `GET /nodes/{id}`
- [x] 2.3 `sdks/php/src/NexusClient.php`: `updateNode(id, props)` and `deleteNode(id)` use the body-param routing the server actually exposes (`PUT/DELETE /data/nodes` with `{node_id, ...}`)
- [x] 2.4 `sdks/csharp/NexusClient.cs`: `CreateNodeAsync` POSTs to `/data/nodes`
- [x] 2.5 `sdks/csharp/NexusClient.cs`: `GetNodeAsync` uses `GET /data/nodes?id=<id>`; `UpdateNodeAsync` and `DeleteNodeAsync` use the body-param routing
- [x] 2.6 PHP regression covered by the live suite (re-ran via docker fallback — 14/14 PASS) plus the existing `ExternalIdTest.php` unit suite
- [x] 2.7 C# regression covered by the existing `ExternalIdTests.cs` (60/60 unit) plus the live suite (14/14)
- [x] 2.8 Re-run the live PHP suite via the existing docker fallback — 14/14 PASS, no 404
- [x] 2.9 Re-run `dotnet test` for the C# tests — 60/60 unit + 14/14 live PASS

## 3. Publish dry-run per SDK
- [x] 3.1 Rust: `cargo publish --dry-run` blocks on unpublished `nexus-protocol = "^2.1.0"` (crates.io currently has 2.0.0 / 1.14.0). Documented in §6 release order — `nexus-protocol` MUST publish before `nexus-graph-sdk`. Local manifest verified clean.
- [x] 3.2 Python: `python -m build` produced `hivehub_nexus_sdk-2.1.0.tar.gz` + `hivehub_nexus_sdk-2.1.0-py3-none-any.whl`. `twine check` reports PASSED on both.
- [x] 3.3 TypeScript: `npm pack --dry-run` produces `hivehub-nexus-sdk-2.1.0.tgz` (66.7 kB, 32 files, name `@hivehub/nexus-sdk` v2.1.0).
- [x] 3.4 Go: `go.mod` module is `github.com/hivellm/nexus-go`, go directive `1.21`. Release tag will be `v2.1.0` once tagged.
- [x] 3.5 C#: `dotnet pack -c Release` produced `bin/Release/Nexus.SDK.2.1.0.nupkg` + `.snupkg` symbols package.
- [x] 3.6 PHP: `composer validate --strict` reports valid (only the informational "version field present" warning — Packagist convention prefers tag-derived versions; harmless for this release).

## 4. Docker + helm artefact tagging
- [x] 4.1 `docker tag nexus-nexus:latest nexus-nexus:2.1.0` — `/health` on the tagged container reports `version: 2.1.0`
- [x] 4.2 `deploy/helm/nexus/Chart.yaml` `appVersion: "2.1.0"` (already in place; chart `version: 0.2.0` independent)
- [x] 4.3 `helm lint deploy/helm/nexus/` via `alpine/helm:3.14.4` — `1 chart(s) linted, 0 chart(s) failed`

## 5. End-to-end validation against the tagged image
- [x] 5.1 `nexus-nexus:2.1.0` started with auth disabled, `/health` returns version 2.1.0
- [x] 5.2 `NEXUS_IMAGE=nexus-nexus:2.1.0 bash scripts/sdks/run-live-suites.sh` -> 6 / 6 SDKs PASS (rust + python + typescript + go + csharp + php)
- [x] 5.3 Refreshed `sdks/PHASE10_LIVE_RESULTS.md` with the 2.1.0-image confirmation
- [x] 5.4 `python scripts/compatibility/test-external-ids-docker.py` against fresh container -> 25 / 25 PASS
- [x] 5.5 `python scripts/compatibility/test-wal-replay-docker.py` against fresh container -> 9 / 9 PASS
- [x] 5.6 `python scripts/compatibility/demo-external-ids-relationships.py` against fresh container -> 29 / 29 PASS

## 6. Release docs + checklist
- [x] 6.1 `docs/development/RELEASE_2.1.0.md` ships the publish order (`nexus-protocol` -> Rust SDK -> Python -> TypeScript -> Go tag -> C# -> PHP tag -> Docker -> helm -> GitHub release) with per-registry commands
- [x] 6.2 `RELEASE_2.1.0.md` "Smoke-test commands" section captures the 150-test pre-publish gate (`run-live-suites.sh` + the three compat suites) the operator runs before AND after every registry push
- [x] 6.3 `docs/development/RELEASE_PROCESS.md` "Cross-SDK release" section points at `RELEASE_2.1.0.md` as the worked example
- [x] 6.4 Top-level `CHANGELOG.md` `[2.1.0]` gains a Fixed bullet covering the PHP + C# route fix and the SDK manifest realignment

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 7.1 Update or create documentation covering the implementation (`docs/development/RELEASE_2.1.0.md` plus `RELEASE_PROCESS.md` cross-link)
- [x] 7.2 Write tests covering the new behavior (PHP + C# regression coverage via existing live + unit suites in §2.6-2.7; cross-image smoke gates in §5.2-5.6)
- [x] 7.3 Run tests and confirm they pass — 60 C# unit + 14 C# live + 14 PHP live + 17 Python live + 16 TS live + 15 Go live + 14 Rust live + 25 REST + 29 demo + 9 WAL replay = **213 PASS / 0 FAIL**
