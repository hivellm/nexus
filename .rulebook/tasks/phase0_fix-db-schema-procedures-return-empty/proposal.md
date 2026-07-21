# Proposal: phase0_fix-db-schema-procedures-return-empty

## Why

`CALL db.labels()`, `CALL db.relationshipTypes()`, and `CALL db.propertyKeys()`
return **zero rows** on a live `nexus-server`, even though the underlying
catalog data is present and correct. Discovered 2026-07-21 during a serial
benchmark comparison: the harness's content-divergence check flagged Nexus
returning `count=0` vs Neo4j's correct `5` labels / `1` relationship type on
all 4 runs. This is a P1 correctness regression — schema introspection is a
core client/driver workflow (browsers, ORMs, admin tooling all call these) —
and it silently returns a wrong-but-well-formed answer instead of erroring.

Evidence collected (all reproducible):

- On the live server, all three procedures return zero rows on BOTH a
  long-lived database and a brand-new database created and seeded with 2
  labels + 1 relationship type in the same session.
- Point lookups of the same catalog data succeed on the same server:
  `MATCH (n) RETURN labels(n)` and `type(r)` resolve real names, and survive
  a full server restart — the data is genuinely persisted in LMDB.
- A standalone example binary (run directly, NOT via `cargo run`/`cargo test`,
  so `Catalog::new`'s test-detection branch at
  `crates/nexus-core/src/catalog/store.rs:200-209` is provably inert) calling
  the exact same `Catalog::new()` → `get_or_create_label()` →
  `list_all_labels()` sequence returns correct entries every time.
- Therefore the defect sits between server request dispatch (HTTP `/cypher`
  and/or RPC `CYPHER`) and the `Catalog::list_all_labels()` /
  `list_all_types()` calls made by
  `crates/nexus-core/src/executor/operators/procedures/db_schema.rs:11-111`
  (which iterate `label_id_to_name` / `type_id_to_name` via `.iter(&rtxn)`,
  `crates/nexus-core/src/catalog/mappings.rs:191-201`, `:364-374`) — NOT
  inside LMDB iteration itself. Plausible suspects to verify, not assume:
  the executor instance serving procedure calls holding a different/empty
  catalog reference than the engine's live one (e.g. a stale clone installed
  before first writes, or a per-database routing mismatch), or the procedure
  dispatch path reading from a catalog opened on the wrong path.

## What Changes

- Root-cause with evidence: instrument or trace which `Catalog` instance
  (path + env pointer) the procedure operator sees at call time on a live
  server vs the one writes go through; compare engine-side vs executor-side
  references across `refresh_executor` boundaries and across the
  multi-database routing (default `neo4j` DB served by `server.engine` vs
  manager-map databases).
- Fix so the three schema procedures read the same live catalog the write
  path maintains, on every database and on both transports (HTTP + RPC).
- Regression tests at the server integration level (spawned server or
  engine-level equivalent of the live path) asserting non-empty, correct
  results for all three procedures after creating labeled nodes/typed
  relationships — including on a freshly created named database.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (procedures section)
- Affected code: `crates/nexus-core/src/executor/operators/procedures/db_schema.rs`,
  executor/engine catalog wiring (`crates/nexus-core/src/engine/mod.rs`,
  `crates/nexus-core/src/executor/shared.rs`), possibly server dispatch
  (`crates/nexus-server/src/api/cypher/`, `crates/nexus-server/src/protocol/rpc/`)
- Breaking change: NO — restores correct output for queries that currently
  return a silently wrong empty result
- User benefit: schema introspection (`db.labels`, `db.relationshipTypes`,
  `db.propertyKeys`) works again for drivers, browsers, and admin tooling;
  unblocks the serial-benchmark schema-procedure gate (blocked on this bug,
  see phase9_store-lock-read-concurrency §4.1)
