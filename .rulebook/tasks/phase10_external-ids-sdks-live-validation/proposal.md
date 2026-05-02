# Proposal: phase10_external-ids-sdks-live-validation

## Why

Phase 9 (`phase9_external-node-ids`, archived) shipped the external-id primitive across the engine, REST, RPC, docs, and a reference Rust SDK with end-to-end coverage validated against a live Docker container (29/29 demo checks pass). The Python, TypeScript, Go, C#, and PHP SDKs received a quick port in commit `48337c77` so the wire contract is already in place — but those ports were never exercised against a running server. They have:

- Type signatures (e.g. `external_id: Optional[str]`, `externalId?: string`, `string externalId`).
- Body composition (request DTOs serialise `external_id` and `conflict_policy` with snake_case JSON binding).
- Local unit tests on serialisation / URL encoding (no server required).

What's missing is the live-server validation that proves each SDK actually round-trips the new fields through `POST /data/nodes`, `GET /data/nodes/by-external-id`, and the Cypher `_id` / `ON CONFLICT` surface. Without it we cannot ship any of the five SDK packages with confidence and the documentation we wrote in phase 9 is unverified outside Rust.

The Phase 9 Docker validation also surfaced two pre-existing server quirks that affect SDK ergonomics:
- The `POST /data/relationships` validator rejects `source_id == 0` and `target_id == 0` (long-standing — node id 0 is valid; documented in code as issue #2 for nodes, fixed there but not for rels).
- Property-map predicates with parameter substitution (`MATCH (n {name: $x})`) silently match nothing through the server's Cypher dispatcher (a separate gap in `crates/nexus-server/src/api/cypher/execute.rs`).

These don't block phase 10 strictly but each SDK's integration test must either work around them or document them so callers don't hit the same wall.

## What Changes

For every non-Rust SDK (`sdks/{python,typescript,go,csharp,php}`), add a **live integration test** that:

1. Boots (or assumes a running) Nexus container at `http://localhost:15474`.
2. Creates nodes with all six `ExternalId` variants (`sha256`, `blake3`, `sha512`, `uuid`, `str`, `bytes`).
3. Round-trips each external id via `getNodeByExternalId` / equivalent and asserts `node.id` matches `create.node_id`.
4. Exercises all three conflict policies (`error` default, `match`, `replace`) — including verifying that `replace` actually updates properties (the bug fixed in commit `fd001344`).
5. Runs a Cypher `CREATE (n:T {_id: '...'}) RETURN n._id` round-trip via the SDK's `executeCypher` helper.
6. Validates length-cap rejection for `str` > 256 bytes and `bytes` > 64 bytes.
7. Validates that an absent external id returns `node: null` (not an HTTP error).

Each SDK also needs:

- The local-only DTO / serialisation tests landed in `48337c77` to stay green (regression guard).
- The `README.md` quick-start section to show the new helpers.
- The CHANGELOG / version bump to reflect the new public API surface.
- A CI hook (or at least a documented `make test-live` / `npm run test:live` / `pytest -m live` invocation) that callers can run against a running container.

A workspace-level orchestration script `scripts/sdks/run-live-suites.sh` boots the Docker container once, runs every SDK's live suite in series, and tears the container down at the end — equivalent of `sdks/run-all-comprehensive-tests.ps1` but scoped to phase-10 coverage.

## Impact

- **Affected specs**:
  - `docs/guides/EXTERNAL_IDS.md` — extend the SDK-examples section so each language's snippet is copy-pasteable from a working test (no more language-by-language drift).
  - `docs/specs/api-protocols.md` — add a "Per-SDK helpers" subsection that lists the canonical public surface for `external_id` / `getByExternalId` per SDK.

- **Affected code**:
  - `sdks/python/nexus_sdk/tests/test_external_id_live.py` (new live suite; existing `test_external_id.py` stays as a unit test).
  - `sdks/typescript/tests/external-id.live.test.ts` (new live vitest suite; existing `external-id.test.ts` stays as unit).
  - `sdks/go/test/external_id_live_test.go` (new live integration runner alongside existing `test_sdk.go`).
  - `sdks/csharp/Tests/ExternalIdLiveTests.cs` (new live xUnit suite; existing `ExternalIdTests.cs` stays as unit).
  - `sdks/php/tests/ExternalIdLiveTest.php` (new live PHPUnit suite; existing `ExternalIdTest.php` stays as unit).
  - Each SDK's README + CHANGELOG.
  - `scripts/sdks/run-live-suites.sh` (new orchestration script).

- **Breaking change**: NO. Public API surface is already shipped in phase 9. Phase 10 only adds tests and docs.

- **User benefit**:
  - Every SDK port is end-to-end validated, not just type-checked.
  - Five new copy-pasteable usage examples that callers can run before adopting Nexus.
  - The known-good regression baseline survives any future server-side change to the external-id surface — a regression in `POST /data/nodes` or `GET /data/nodes/by-external-id` will surface in five language test runs, not just the Rust one.

## Source

- `crates/nexus-server/src/api/data.rs:84-102` (CreateNodeRequest fields shipped in phase 9)
- `scripts/compatibility/test-external-ids-docker.py` (25-test live REST suite, full pass on phase 9 — reference shape)
- `scripts/compatibility/test-wal-replay-docker.py` (WAL replay validation across container restart)
- `scripts/compatibility/demo-external-ids-relationships.py` (29-step end-to-end demo including REPLACE + projection + traversal)
- `sdks/rust/tests/integration_test.rs:test_create_node_with_external_id_round_trip` (Rust reference test)
- Phase 9 commit range: `1bf7247e..fd001344`
