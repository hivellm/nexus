## 1. Shared infrastructure
- [ ] 1.1 Build a single `nexus-nexus` image once via `docker compose build` and document the prerequisite in each SDK's live README section
- [ ] 1.2 Add `scripts/sdks/run-live-suites.sh` that starts the container with auth disabled, polls `/health` until ready, runs the five SDK live suites in sequence, captures pass/fail per SDK, and tears down the container at the end
- [ ] 1.3 Wire a sentinel-node helper into the orchestration script so SDK rel tests don't trip the `source_id/target_id == 0` validator (port the workaround from `scripts/compatibility/demo-external-ids-relationships.py`)

## 2. Python SDK live coverage
- [ ] 2.1 Create `sdks/python/nexus_sdk/tests/test_external_id_live.py` using `pytest.mark.live` (gated on `NEXUS_LIVE_HOST` env var so unit-only CI runs do not require a server)
- [ ] 2.2 Test all six `ExternalId` variants (sha256/blake3/sha512/uuid/str/bytes) via `create_node_with_external_id` + `get_node_by_external_id` round-trip
- [ ] 2.3 Test all three conflict policies (`error`/`match`/`replace`); REPLACE must actually overwrite a property (regression guard for commit `fd001344`)
- [ ] 2.4 Test Cypher `CREATE (n:T {_id: '...'}) RETURN n._id` via `execute_cypher`
- [ ] 2.5 Test length caps (str > 256 bytes, bytes > 64 bytes) surface a typed error
- [ ] 2.6 Update `sdks/python/README.md` with a quickstart snippet pulled directly from the live test
- [ ] 2.7 Bump `sdks/python/pyproject.toml` version + add CHANGELOG entry under "Added"

## 3. TypeScript SDK live coverage
- [ ] 3.1 Create `sdks/typescript/tests/external-id.live.test.ts` (vitest, gated on `NEXUS_LIVE_HOST` env var)
- [ ] 3.2 Test all six `ExternalId` variants via `createNodeWithExternalId` + `getNodeByExternalId`
- [ ] 3.3 Test all three conflict policies; REPLACE must overwrite a property
- [ ] 3.4 Test Cypher `_id` round-trip via `executeCypher`
- [ ] 3.5 Test length caps for str / bytes variants
- [ ] 3.6 Update `sdks/typescript/README.md` quickstart and add `npm run test:live` script in `package.json`
- [ ] 3.7 Bump `sdks/typescript/package.json` version + CHANGELOG entry

## 4. Go SDK live coverage
- [ ] 4.1 Create `sdks/go/test/external_id_live_test.go` with the build tag `live` so `go test -tags=live` runs the suite
- [ ] 4.2 Test all six `ExternalId` variants via `CreateNodeWithExternalID` + `GetNodeByExternalID`
- [ ] 4.3 Test all three conflict policies; REPLACE overwrite
- [ ] 4.4 Test Cypher `_id` round-trip
- [ ] 4.5 Test length caps
- [ ] 4.6 Update `sdks/go/README.md` quickstart
- [ ] 4.7 Update `sdks/go/CHANGELOG.md`

## 5. C# SDK live coverage
- [ ] 5.1 Create `sdks/csharp/Tests/ExternalIdLiveTests.cs` with `[Trait("category", "live")]` so xUnit can filter via `dotnet test --filter "category=live"`
- [ ] 5.2 Test all six `ExternalId` variants via `CreateNodeWithExternalIdAsync` + `GetNodeByExternalIdAsync`
- [ ] 5.3 Test all three conflict policies; REPLACE overwrite
- [ ] 5.4 Test Cypher `_id` round-trip via `ExecuteCypherAsync`
- [ ] 5.5 Test length caps
- [ ] 5.6 Update `sdks/csharp/README.md` quickstart
- [ ] 5.7 Bump `sdks/csharp/Nexus.SDK.csproj` version + CHANGELOG entry

## 6. PHP SDK live coverage
- [ ] 6.1 Create `sdks/php/tests/ExternalIdLiveTest.php` with `@group live` so PHPUnit can filter via `vendor/bin/phpunit --group live`
- [ ] 6.2 Test all six `ExternalId` variants via `createNodeWithExternalId` + `getNodeByExternalId`
- [ ] 6.3 Test all three conflict policies; REPLACE overwrite
- [ ] 6.4 Test Cypher `_id` round-trip
- [ ] 6.5 Test length caps
- [ ] 6.6 Update `sdks/php/README.md` quickstart
- [ ] 6.7 Update `sdks/php/composer.json` version + CHANGELOG entry

## 7. Documentation
- [ ] 7.1 Extend `docs/guides/EXTERNAL_IDS.md` with one verified-working snippet per SDK (each pulled from its live test)
- [ ] 7.2 Add a "Per-SDK helpers" subsection to `docs/specs/api-protocols.md` listing the canonical public method names per language
- [ ] 7.3 Cross-link `scripts/sdks/run-live-suites.sh` from the contributor docs (root `CONTRIBUTING.md` if present, or `docs/guides/`)

## 8. Cross-SDK orchestration
- [ ] 8.1 Run `scripts/sdks/run-live-suites.sh` and confirm every SDK suite reports zero failures
- [ ] 8.2 Capture per-SDK timings and check totals into a results file under `sdks/PHASE10_LIVE_RESULTS.md`
- [ ] 8.3 Add a CI workflow stub (or document the manual run) so future regressions on the external-id surface trip the live SDK gates

## 9. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 9.1 Update or create documentation covering the implementation
- [ ] 9.2 Write tests covering the new behavior
- [ ] 9.3 Run tests and confirm they pass
