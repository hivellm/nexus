# Proposal: phase0_fix-multi-database-persistence-and-default

**Priority: HIGH — multi-database is now routable (`phase0_fix-cypher-database-routing`
made the `POST /cypher` `database` field real) but not yet durable or consistent.**
Found during the audit that immediately followed that fix. Three pre-existing gaps
that were invisible while routing was dead (every query hit the default engine) and
became user-visible the moment routing started working.

## Why

### G1 — No restart discovery (data becomes unreachable across restarts)

`DatabaseManager::new(base_dir)` (`crates/nexus-core/src/database/mod.rs:66-82`)
only calls `create_database("neo4j")`. It never scans `base_dir` for database
directories left by a previous run, and there is no manifest on disk — the
database list is purely the in-memory `databases: HashMap<String, Arc<RwLock<Engine>>>`
(`:55`). So after a server restart, a database `alpha` created last run is a live
directory at `base_dir/alpha` but `manager.exists("alpha")` returns `false`; every
query naming it now errors (`"Database 'alpha' does not exist"`) and its data is
orphaned. `create_database("alpha")` after restart checks only `dbs.contains_key`
(`:99-105`), so it re-`create_dir_all`s the existing dir and opens a fresh `Engine`
over the existing files — reusing them by accident rather than by design.

### G2 — Split-brain default: `server.engine` (root) vs the manager's phantom `neo4j`

`server.engine` is opened on the ROOT `data_dir` (`crates/nexus-server/src/main.rs:234`),
while `DatabaseManager::new` independently opens a SECOND engine for `"neo4j"` at
`data_dir/neo4j` (`main.rs:252` -> `database/mod.rs:78,108,112`). Query routing with no
`database` field (and an explicit `database:"neo4j"`) resolves to `server.engine`
(the root store), but `SHOW DATABASES` / `GET /databases` read their node/relationship
counts from the manager's `neo4j` engine (`database/mod.rs:218-267`) — the phantom at
`data_dir/neo4j`, which is empty. So a node created through the default route does not
appear in the `neo4j` row of `SHOW DATABASES`, and two independent live `Engine`
instances exist for "the default database" with no cross-store consistency.

### G3 — Database management is unauthenticated

`execute_database_commands` (`crates/nexus-server/src/api/cypher/commands.rs`) and the
REST `POST /databases` / `DELETE /databases/{name}` handlers
(`crates/nexus-server/src/api/database.rs`) take no `auth_context` and enforce no
permission — any authenticated caller (or anyone, with auth disabled) can create,
drop, and enumerate databases. This mirrors the `/auth/*` gap already closed by
`phase0_fix-auth-management-authorization`, but for the database-management surface.

## What Changes

- **G1**: on startup, discover existing database directories under `base_dir` and
  re-open them (or persist and reload a manifest of database names + state), so a
  created database survives a restart and `SHOW DATABASES` / routing see it. Decide
  and document what counts as a valid database dir (e.g. presence of a catalog) so
  unrelated subdirectories are not mistaken for databases.
- **G2**: unify the default. Make `server.engine` BE the `DatabaseManager`'s default
  database (one engine, one store for "neo4j"/root) instead of opening a second
  phantom engine — so query routing, `SHOW DATABASES`, and stats all agree on the
  default. Either register `server.engine` into the manager as the default, or have
  the server obtain its default engine from the manager. Avoid two live engines on
  overlapping paths.
- **G3**: gate create/drop/(optionally list) database management behind an
  appropriate permission (Admin/Super), consistent with
  `phase0_fix-auth-management-authorization`, on BOTH the Cypher DDL path and the
  REST endpoints.
- **Optional (stretch)**: extend the per-request `database` selection to
  `/cypher/stream` (and evaluate `/ingest`, `/knn_traverse`) using the same
  `resolve_engine` / `with_write_engine` helpers already in
  `api/cypher/execute/handler.rs`, if in scope.

## Impact

- Affected specs: `docs/specs/api-protocols.md` (default identity, persistence
  guarantees, auth on management)
- Affected code: `crates/nexus-core/src/database/mod.rs` (discovery/persistence +
  default identity), `crates/nexus-server/src/main.rs` (engine/manager wiring),
  `crates/nexus-server/src/api/database.rs` + `api/cypher/commands.rs` (auth gating)
- Breaking change: NO for query semantics; changes startup behavior (databases now
  rediscovered) and management authorization (previously open)
- User benefit: created databases survive restarts and are reachable; `SHOW
  DATABASES` reports the real default; only privileged callers can manage databases
- Related: `phase0_fix-cypher-database-routing` (made routing real; surfaced these
  gaps), `phase0_fix-auth-management-authorization` (same auth pattern for `/auth/*`)
