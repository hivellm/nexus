# Tasks: phase0_fix-multi-database-persistence-and-default

Three pre-existing multi-database gaps surfaced once `phase0_fix-cypher-database-routing`
made per-request routing real: G1 no restart discovery, G2 split-brain default
(`server.engine` root vs the manager's phantom `neo4j` at `data_dir/neo4j`), G3
database management is unauthenticated. Do G2 before G1: unifying the default engine
identity changes what "the default database dir" is, which the discovery in G1 must
account for.

## 1. Pin the gaps with failing tests first (TDD)
- [x] 1.1 G1: a server test that creates database `alpha`, writes a node under it,
  drops+rebuilds the `DatabaseManager` from the SAME base_dir (simulating restart),
  and asserts `alpha` still exists and its node is readable. Fails today (alpha is
  not rediscovered).
- [x] 1.2 G2: a test asserting the default is a single store — a node created via the
  default route (no `database` field) is counted in the `neo4j` row of
  `SHOW DATABASES` / `list_databases`. Fails today (phantom `neo4j` reports 0).
- [x] 1.3 G3: a test that a non-Admin caller cannot `CREATE DATABASE` / `POST
  /databases` / `DROP DATABASE` / `DELETE /databases/{name}` (403), and an Admin can.
  Fails today (no auth gating).

## 2. G2 — unify the default database identity
- [x] 2.1 Make `server.engine` and the `DatabaseManager`'s default database be the
  SAME engine/store (no second phantom engine at `data_dir/neo4j`). Either register
  `server.engine` into the manager as the default at construction, or have the server
  take its default engine from the manager. Ensure only ONE live `Engine` covers the
  default store.
- [x] 2.2 Confirm `SHOW DATABASES` / `list_databases` / `GET /databases` now report
  the default's real node/relationship counts, and that routing (`resolve_engine`
  Default arm) targets that same engine.

## 3. G1 — restart discovery / persistence
- [x] 3.1 On `DatabaseManager` construction (or a startup hook wired in `main.rs`),
  discover existing database directories under `base_dir` and re-open each as a
  database, so `exists`/routing/`SHOW DATABASES` see databases created in prior runs.
- [x] 3.2 Define and document what qualifies as a database directory (e.g. contains a
  catalog) so unrelated subdirectories (the default/root store, `auth`, `audit`,
  logs) are not mistaken for databases. Reconcile with the G2 default-dir layout.
- [x] 3.3 Make `create_database` behave sanely when the dir already exists (re-adopt
  vs error) now that discovery runs — no accidental reinit of a populated dir.

## 4. G3 — authenticate database management
- [x] 4.1 Gate `CREATE DATABASE` / `DROP DATABASE` (Cypher `execute_database_commands`)
  and REST `POST /databases` / `DELETE /databases/{name}` behind Admin/Super, mirroring
  `phase0_fix-auth-management-authorization`'s `require_admin` pattern. Decide whether
  listing (`SHOW DATABASES` / `GET /databases`) also requires a permission.
- [x] 4.2 Preserve the auth-disabled bootstrap path (no identity present -> no-op),
  as the auth-management fix did.

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 Update or create documentation covering the implementation
  (`docs/specs/api-protocols.md`: the default is a single store, databases persist
  across restarts, database management requires Admin; CHANGELOG entry).
- [x] 5.2 Write tests covering the new behavior (the §1 tests now passing, plus a
  drop-then-recreate-after-restart case and a stretch test for `/cypher/stream`
  routing if that optional item is taken).
- [x] 5.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` green).

## Related
- `phase0_fix-cypher-database-routing` — made per-request routing real; this task
  makes the databases it routes to durable, consistent, and access-controlled.
- `phase0_fix-auth-management-authorization` — the Admin/Super gating pattern to reuse.
