# Tasks: phase19_fix-cypher-database-routing

Fix non-functional multi-database isolation over REST. `CypherRequest.database`
(`api/cypher/mod.rs:166-168`) is parsed and never read, so every query hits the
default store — including queries naming a database that does not exist.
`PUT /session/database` (`main.rs:825-836`) returns `{"success":true}` but the
following `GET` still reports `neo4j`. Confirmed empirically: a node created under
`database:"probe1"` is readable under `novidx`, `ldbc`, and `nosuchdb_xyz`.

Order matters: pin the behaviour with failing tests first, then route, then reject
unknown databases, then reconcile the session endpoints, then extend to the other
Cypher-executing paths. Do not start §3 before §2 — rejecting unknown names is only
meaningful once resolution actually happens.

## 1. Pin the bug with failing tests first (TDD)
- [ ] 1.1 Add a failing server test: a node created via `POST /cypher` with `"database":"alpha"` must NOT be visible from `POST /cypher` with `"database":"beta"`. Assert on both directions so a fix that merely swaps which store is shared still fails
- [ ] 1.2 Add a failing test that `POST /cypher` with `"database":"<never-created>"` returns an error rather than results from the default store
- [ ] 1.3 Add a failing test for the session endpoints: `PUT /session/database {"name":"alpha"}` followed by `GET /session/database` must report `alpha`, and a subsequent `/cypher` with no `database` field must execute against `alpha`
- [ ] 1.4 Add a regression guard for the default path: `/cypher` with no `database` field and no session switch still resolves to the default database, so existing single-database clients are unaffected

## 2. Route the per-request database field
- [ ] 2.1 Resolve `CypherRequest.database` through `NexusServer::database_manager` in the `/cypher` handler and execute against the resolved database's engine; the field is currently declared at `api/cypher/mod.rs:166-168` and read nowhere
- [ ] 2.2 Define the precedence explicitly and document it in the handler: request `database` field → session database → server default (`neo4j`). Precedence must be a single helper, not duplicated per call site, so the other endpoints in §5 reuse it
- [ ] 2.3 Confirm the resolved engine is used for the whole request including the planner cache — a per-database plan cache keyed only by query text would leak plans across databases; check `executor/planner/cache.rs` keying and fix if it is global

## 3. Reject unknown databases
- [ ] 3.1 Return a typed error (not the default store's results) when the named database does not exist; match the existing error envelope so SDKs surface it as a query error
- [ ] 3.2 Verify the error path does not create the database implicitly — a typo must fail, never silently provision

## 4. Reconcile the session endpoints
- [ ] 4.1 Determine whether per-connection session state is representable in the current stateless server model. `switch_session_database` builds `DatabaseState` fresh from `server.database_manager.clone()` per request (`main.rs:827-835`), which is why the mutation does not survive to the next `GET`
- [ ] 4.2 Either make the switch persist so `GET /session/database` reflects it and `/cypher` honours it, or **remove both endpoints**. Do not leave an API that reports `{"success":true,"message":"Switched to database 'X'"}` while changing nothing — per `no-shortcuts`, an endpoint that lies is worse than an absent one. If removing, document the per-request `database` field as the supported mechanism

## 5. Extend to the other Cypher-executing paths
- [ ] 5.1 Apply the §2.2 resolution helper to `/ingest`, `/knn_traverse`, `/graphql` and RPC `CYPHER` — the set is already enumerated in `middleware/admission.rs:291`. Each either honours the database or explicitly documents that it is default-only; silently ignoring the field again is not acceptable
- [ ] 5.2 Audit the SDKs for a `database` parameter that has been silently inert (the Python/TypeScript/Rust/Go/C#/PHP clients under `sdks/`) and make sure their behaviour now matches the server; note any that need a release

## 6. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 6.1 Update or create documentation covering the implementation (`docs/specs/api-protocols.md` for the resolution precedence and the unknown-database error; CHANGELOG entry flagging the deliberate breaking change; reconcile the CRITICAL constraint #4 claim in `CLAUDE.md` with what actually ships)
- [ ] 6.2 Write tests covering the new behavior (the §1 tests now passing, plus isolation coverage for whichever §5 endpoints gained routing, written 1–3 at a time and run immediately)
- [ ] 6.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green)

## Related
- Discovered by `phase7_ldbc-snb-benchmark` item 1.2, which wanted a dedicated `ldbc`
  database for the SNB dataset and found its `--database` flag had no effect. That
  harness assumes one database per server process until this ships.
- Same failure shape as issue #3 (`parameters` silently dropped by serde), documented
  in a comment 8 lines above the dead field.
