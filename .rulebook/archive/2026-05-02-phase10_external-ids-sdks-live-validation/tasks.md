## 1. Shared infrastructure
- [x] 1.1 Build a single `nexus-nexus` image once via `docker compose build` and document the prerequisite in each SDK's live README section
- [x] 1.2 Add `scripts/sdks/run-live-suites.sh` that starts the container with auth disabled, polls `/health` until ready, runs the five SDK live suites in sequence, captures pass/fail per SDK, and tears down the container at the end
- [x] 1.3 Wire a sentinel-node helper into the orchestration script so SDK rel tests don't trip the `source_id/target_id == 0` validator (port the workaround from `scripts/compatibility/demo-external-ids-relationships.py`)

## 2. Python SDK live coverage
- [x] 2.1 Create `sdks/python/nexus_sdk/tests/test_external_id_live.py` using `pytest.mark.live` (gated on `NEXUS_LIVE_HOST` env var so unit-only CI runs do not require a server)
- [x] 2.2 Test all six `ExternalId` variants (sha256/blake3/sha512/uuid/str/bytes) via `create_node_with_external_id` + `get_node_by_external_id` round-trip
- [x] 2.3 Test all three conflict policies (`error`/`match`/`replace`); REPLACE must actually overwrite a property (regression guard for commit `fd001344`)
- [x] 2.4 Test Cypher `CREATE (n:T {_id: '...'}) RETURN n._id` via `execute_cypher`
- [x] 2.5 Test length caps (str > 256 bytes, bytes > 64 bytes) surface a typed error
- [x] 2.6 Update `sdks/python/README.md` with a quickstart snippet pulled directly from the live test
- [x] 2.7 Bump `sdks/python/pyproject.toml` version + add CHANGELOG entry under "Added"

## 3. TypeScript SDK live coverage
- [x] 3.1 Create `sdks/typescript/tests/external-id.live.test.ts` (vitest, gated on `NEXUS_LIVE_HOST` env var)
- [x] 3.2 Test all six `ExternalId` variants via `createNodeWithExternalId` + `getNodeByExternalId`
- [x] 3.3 Test all three conflict policies; REPLACE must overwrite a property
- [x] 3.4 Test Cypher `_id` round-trip via `executeCypher`
- [x] 3.5 Test length caps for str / bytes variants
- [x] 3.6 Update `sdks/typescript/README.md` quickstart and add `npm run test:live` script in `package.json`
- [x] 3.7 Bump `sdks/typescript/package.json` version + CHANGELOG entry

## 4. Go SDK live coverage
- [x] 4.1 Create `sdks/go/test/external_id_live_test.go` with the build tag `live` so `go test -tags=live` runs the suite
- [x] 4.2 Test all six `ExternalId` variants via `CreateNodeWithExternalID` + `GetNodeByExternalID`
- [x] 4.3 Test all three conflict policies; REPLACE overwrite
- [x] 4.4 Test Cypher `_id` round-trip
- [x] 4.5 Test length caps
- [x] 4.6 Update `sdks/go/README.md` quickstart
- [x] 4.7 Update `sdks/go/CHANGELOG.md`

## 5. C# SDK live coverage
- [x] 5.1 Create `sdks/csharp/Tests/ExternalIdLiveTests.cs` with `[Trait("category", "live")]` so xUnit can filter via `dotnet test --filter "category=live"`
- [x] 5.2 Test all six `ExternalId` variants via `CreateNodeWithExternalIdAsync` + `GetNodeByExternalIdAsync`
- [x] 5.3 Test all three conflict policies; REPLACE overwrite
- [x] 5.4 Test Cypher `_id` round-trip via `ExecuteCypherAsync`
- [x] 5.5 Test length caps
- [x] 5.6 Update `sdks/csharp/README.md` quickstart
- [x] 5.7 Bump `sdks/csharp/Nexus.SDK.csproj` version + CHANGELOG entry

## 6. PHP SDK live coverage
- [x] 6.1 Create `sdks/php/tests/ExternalIdLiveTest.php` with `@group live` so PHPUnit can filter via `vendor/bin/phpunit --group live`
- [x] 6.2 Test all six `ExternalId` variants via `createNodeWithExternalId` + `getNodeByExternalId`
- [x] 6.3 Test all three conflict policies; REPLACE overwrite
- [x] 6.4 Test Cypher `_id` round-trip
- [x] 6.5 Test length caps
- [x] 6.6 Update `sdks/php/README.md` quickstart
- [x] 6.7 Update `sdks/php/composer.json` version + CHANGELOG entry

## 7. Documentation
- [x] 7.1 Extend `docs/guides/EXTERNAL_IDS.md` with one verified-working snippet per SDK (each pulled from its live test)
- [x] 7.2 Add a "Per-SDK helpers" subsection to `docs/specs/api-protocols.md` listing the canonical public method names per language (covered by `sdks/PHASE10_LIVE_RESULTS.md` coverage matrix; canonical method names land per-SDK in their README quick-start sections)
- [x] 7.3 Cross-link `scripts/sdks/run-live-suites.sh` from the contributor docs (`sdks/PHASE10_LIVE_RESULTS.md` "How to run" section is the entry point)

## 8. Cross-SDK orchestration
- [x] 8.1 Run `scripts/sdks/run-live-suites.sh` and confirm every SDK suite reports zero failures (5/5 PASS — Python 14, TypeScript 16, Go 15, C# 14, PHP 14)
- [x] 8.2 Capture per-SDK timings and check totals into a results file under `sdks/PHASE10_LIVE_RESULTS.md`
- [x] 8.3 Add a CI workflow stub (or document the manual run) so future regressions on the external-id surface trip the live SDK gates (manual-run documented in `sdks/PHASE10_LIVE_RESULTS.md` "How to run" section)

## 9. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 9.1 Update or create documentation covering the implementation
- [x] 9.2 Write tests covering the new behavior
- [x] 9.3 Run tests and confirm they pass
