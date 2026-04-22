# Proposal: System Procedures (`db.*`, `dbms.*`)

## Why

Neo4j clients (drivers, Cypher Shell, APOC, most BI tools, Bloom, and
every openCypher introspection tool) rely on the `db.*` and `dbms.*`
procedure families to enumerate schema, inspect indexes, read cluster
state, and discover what the server supports. Nexus currently exposes
**zero** system procedures. That makes Nexus invisible to any tool
that does `CALL db.schema.visualization()` or `CALL db.labels()` before
issuing real queries — which is most of them.

Without these procedures:

- Neo4j Desktop, Neo4j Browser, and Bloom cannot render a schema view.
- Official drivers fail on `Session.getServerInfo()` because
  `dbms.components()` is missing.
- Third-party BI adapters (Hackolade, yFiles, linkurious) reject the
  connection during capability discovery.
- Cypher Shell cannot autocomplete label or relationship-type names.

This task brings the surface parity with Neo4j 5.x's documented system
procedure set — the stable subset, not deprecated spellings. Roughly
30 procedures across five namespaces.

## What Changes

- New module `nexus-core/src/procedures/system/` containing five
  sub-modules: `db_schema`, `db_indexes`, `db_constraints`, `db_labels`,
  `dbms_info`.
- Procedure registry extended so that `CALL db.foo()` dispatches to
  the appropriate Rust function, exactly like the existing GDS
  wrappers.
- Every procedure returns a well-typed `Stream<Row>` matching Neo4j's
  column names and types so that drivers deserialise them unchanged.
- New system meta-procedure `dbms.procedures()` that self-describes the
  full catalogue (needed by Cypher Shell tab completion).
- The CLI's `nexus procedures` command is rewired to call
  `dbms.procedures()` instead of hard-coded lists.

**BREAKING**: none. Procedures are new symbols; no existing query
semantics change.

## Impact

### Affected Specs

- NEW capability: `system-procedures-db-schema`
- NEW capability: `system-procedures-db-indexes`
- NEW capability: `system-procedures-db-constraints`
- NEW capability: `system-procedures-db-labels`
- NEW capability: `system-procedures-dbms`

### Affected Code

- `nexus-core/src/procedures/system/mod.rs` (NEW, ~60 lines, registry)
- `nexus-core/src/procedures/system/db_schema.rs` (NEW, ~320 lines)
- `nexus-core/src/procedures/system/db_indexes.rs` (NEW, ~220 lines)
- `nexus-core/src/procedures/system/db_constraints.rs` (NEW, ~180 lines)
- `nexus-core/src/procedures/system/db_labels.rs` (NEW, ~200 lines)
- `nexus-core/src/procedures/system/dbms_info.rs` (NEW, ~240 lines)
- `nexus-core/src/executor/operators/procedures.rs` (~80 lines modified, dispatch)
- `nexus-cli/src/commands/procedures.rs` (~40 lines modified)
- `nexus-core/tests/system_procedures.rs` (NEW, ~900 lines)

### Dependencies

- Requires: none (self-contained read-only introspection)
- Unblocks: `phase6_opencypher-apoc-ecosystem` (APOC frequently delegates
  to `db.*` — implementing APOC without `db.*` leads to duplicated code).

### Timeline

- **Duration**: 2–3 weeks
- **Complexity**: Low–Medium (data exists; wiring is mechanical)
- **Risk**: Low — read-only access to structures already in memory
