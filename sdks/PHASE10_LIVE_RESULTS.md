# Phase 10 — External-IDs SDK Live Validation Results

| SDK        | File                                                | Tests   | Status | Runtime          |
|------------|-----------------------------------------------------|---------|--------|------------------|
| Python     | `sdks/python/nexus_sdk/tests/test_external_id_live.py` | 14 / 14 | PASS   | ~4.4s (pytest)   |
| TypeScript | `sdks/typescript/tests/external-id.live.test.ts`    | 16 / 16 | PASS   | ~98ms (vitest)   |
| Go         | `sdks/go/test/external_id_live_test.go`             | 15 / 15 | PASS   | ~270ms (go test) |
| C#         | `sdks/csharp/Tests/ExternalIdLiveTests.cs`          | 14 / 14 | PASS   | ~140ms (xunit)   |
| PHP        | `sdks/php/tests/ExternalIdLiveTest.php`             | 14 / 14 | PASS   | ~570ms (phpunit) |

Total live cases: **73**, all green against a single nexus-nexus container running at `http://localhost:15474` with `NEXUS_AUTH_ENABLED=false`.

## Coverage matrix

Every SDK suite exercises the same surface against the live server:

- All 6 `ExternalId` variants (`sha256`, `blake3`, `sha512`, `uuid`, `str`, `bytes`).
- All 3 `ConflictPolicy` values:
  - `Error` (default) — duplicate must be rejected.
  - `Match` — duplicate must return existing internal id with no property writes.
  - `Replace` — duplicate must reuse the internal id and overwrite properties (regression guard for commit `fd001344`).
- Cypher `CREATE (n:T {_id: '...'}) RETURN n._id` round-trip via the SDK's `executeCypher` helper.
- Length-cap validation — `str > 256 bytes`, `bytes > 64 bytes`, `uuid:` empty payload.
- Absent external id resolves to a non-error `node: null` response.

## Bug-fixes shipped during phase 10

- **Go (`sdks/go/client.go`)** — `doRequest` was passing query strings through `url.JoinPath`, which percent-encoded `?` and folded the query into the path segment, producing 404 against `/data/nodes/by-external-id?external_id=...`. The path is now split on `?` before `JoinPath`, query reattached afterward. Also added a dedicated `ExternalIDNode` type with `id uint64` because the by-external-id endpoint returns the internal id as a JSON number, while the Cypher-row `Node.id` is a string.
- **PHP (`sdks/php/src/Models.php`)** — `QueryResult` gained an optional `?string $error` field so the server's "200 OK with `error` JSON field" pattern (used for length-cap rejections) is reachable from PHP callers via `$result->error`.

## How to run

```bash
# Build the Nexus image once (covers every server-side fix landed in phase 9
# + 10, including the REPLACE-prop-ptr fix from commit fd001344).
docker compose build

# Run every SDK live suite in series against a fresh container.
bash scripts/sdks/run-live-suites.sh

# Filter to a single SDK while iterating:
SDKS="python" bash scripts/sdks/run-live-suites.sh
SDKS="typescript" bash scripts/sdks/run-live-suites.sh

# Keep the container running for ad-hoc probing:
KEEP_CONTAINER=1 bash scripts/sdks/run-live-suites.sh
```

Each SDK suite gates on `NEXUS_LIVE_HOST` so unit-only CI runs (e.g.
`pytest`, `npx vitest`, `go test`, `dotnet test`, `phpunit`) without that
env var still pass while ignoring the live group.

## Pre-existing issues observed but out of scope

These are documented for follow-up phases — they do not affect external-id
behaviour:

1. **PHP / C# legacy `createNode` posts to `/nodes`** — the server only
   exposes `/data/nodes`. The phase-9 helpers `createNodeWithExternalId` /
   `CreateNodeWithExternalIdAsync` correctly use `/data/nodes`, so phase-10
   tests are unaffected. A future phase should retire the `/nodes` path or
   add a router alias.
2. **Server's `MATCH (n {prop: $param})` ignores parameter substitution
   inside property maps** — works with literals only. Phase-10 demo scripts
   inline literals as a workaround.
3. **`POST /data/relationships` validator rejects `source_id == 0` /
   `target_id == 0`** — pre-phase-9 quirk; the orchestration script burns
   id 0 with a `_Sentinel` node so SDK rel tests start at id ≥ 1.
