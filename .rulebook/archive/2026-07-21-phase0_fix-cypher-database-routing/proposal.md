# Proposal: phase0_fix-cypher-database-routing

**Priority: HIGH — silent cross-database data leakage, and two APIs that report
success while doing nothing.** Found and empirically confirmed while implementing
`phase7_ldbc-snb-benchmark` item 1.2 (LDBC SNB schema prep); not previously reported
and not tracked by any GitHub issue. Should be fixed before 2.6.0 ships.

## Why

Multi-database isolation is **non-functional over the REST API**. Databases can be
created, but every Cypher query is executed against the same underlying store
regardless of which database the client asked for — and both of the mechanisms that
are supposed to select a database silently no-op while reporting success.

`CLAUDE.md` lists this as CRITICAL constraint #4:

> **Database Isolation** — Each database is completely isolated: separate LMDB
> catalog, separate record stores, separate indexes.

That guarantee does not hold for anything that goes through `POST /cypher`.

### Confirmed empirically (Nexus 2.5.0, debug build, single server instance)

Create a node while asking for database `probe1`, then read it back from three other
database names — including one that was never created:

```
POST /cypher {"query":"CREATE (:IsolationCanary {marker:'from-probe1'})","database":"probe1"}

POST /cypher {"query":"MATCH (n:IsolationCanary) RETURN n.marker","database":"probe1"}
    -> [["from-probe1"]]
POST /cypher {"query":"MATCH (n:IsolationCanary) RETURN n.marker","database":"novidx"}
    -> [["from-probe1"]]     FAIL: leaked across databases
POST /cypher {"query":"MATCH (n:IsolationCanary) RETURN n.marker","database":"ldbc"}
    -> [["from-probe1"]]     FAIL: leaked across databases
POST /cypher {"query":"MATCH (n:IsolationCanary) RETURN n.marker","database":"nosuchdb_xyz"}
    -> [["from-probe1"]]     FAIL: nonexistent database is not even rejected
```

The session-scoped switch is equally inert, and worse, it lies:

```
GET /session/database                       -> {"database":"neo4j"}
PUT /session/database {"name":"novidx"}     -> {"success":true,"message":"Switched to database 'novidx'"}
GET /session/database                       -> {"database":"neo4j"}     FAIL: never switched
```

### Mechanism

Two independent defects, one per routing mechanism.

**1. The per-request `database` field is dead.** `CypherRequest` declares it at
`crates/nexus-server/src/api/cypher/mod.rs:166-168`:

```rust
/// Database name (optional, defaults to "neo4j")
#[serde(default)]
pub database: Option<String>,
```

but the field is **never read anywhere in the handler** — grepping `database` across
that module returns only the declaration itself and unrelated doc comments on `:3`
and `:18`. Serde parses it and the handler discards it. This is the same failure
shape as issue #3 (the `parameters` alias that serde silently dropped), which is
documented in a comment 8 lines above the offending field.

**2. `PUT /session/database` does not persist.** The route is wired at
`crates/nexus-server/src/main.rs:825-836` to
`api::database::switch_session_database` with a `DatabaseState` built fresh from
`server.database_manager.clone()` per request. It returns success, but the
subsequent `GET /session/database` (`main.rs:813-823`) still reports `neo4j`, so
whatever the switch mutates is not the state the getter reads — and in any case
`/cypher` never consults it.

The storage layer is not the problem: `NexusServer` already owns a
`database_manager`, and `POST /databases` genuinely creates databases. The gap is
entirely in the REST layer, which never asks the manager to resolve the target
database before executing.

## What Changes

- Make `POST /cypher` resolve `CypherRequest.database` through `database_manager`
  and execute against that database's engine, falling back to the session database
  and then to the default.
- Reject a request naming a database that does not exist with a typed error instead
  of silently serving the default store.
- Make `PUT /session/database` persist, so `GET /session/database` reflects it and
  `/cypher` honours it; or, if per-connection session state cannot be represented in
  the current stateless server model, remove both endpoints rather than ship an API
  that reports success and does nothing.
- Apply the same routing to the other Cypher-executing paths that accept a database
  (`/ingest`, `/knn_traverse`, `/graphql`, RPC `CYPHER`) — the admission middleware
  at `middleware/admission.rs:291` already enumerates them.

## Impact

- Affected specs: `docs/specs/api-protocols.md`; the CRITICAL constraint #4 wording
  in `CLAUDE.md` is currently false for the REST path
- Affected code: `crates/nexus-server/src/api/cypher/mod.rs`,
  `crates/nexus-server/src/api/database.rs`, `crates/nexus-server/src/main.rs`
- Breaking change: **YES, deliberately** — clients relying on the accidental
  behaviour (every database resolving to the default store) will start seeing real
  isolation, and requests naming a nonexistent database will start failing instead
  of silently succeeding. Both are the documented contract.
- User benefit: the documented isolation guarantee becomes real; a client can no
  longer write to a database it did not intend, and a typo in a database name fails
  loudly instead of quietly corrupting the default store.
- Blocks: `phase7_ldbc-snb-benchmark` cannot isolate its dataset in a dedicated
  database and must currently assume one database per server process.
