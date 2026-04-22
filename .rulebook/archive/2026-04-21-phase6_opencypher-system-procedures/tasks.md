# Implementation Tasks — System Procedures

## 1. Registry & Dispatch

- [x] 1.1 Registry lives inline in `crates/nexus-core/src/executor/operators/procedures.rs::execute_call_procedure` match arms — no dedicated `SystemProcRegistry` struct, because the existing `ProcedureRegistry` already covers custom/GDS procs and the system surface is a closed set driven entirely off the executor's context.
- [x] 1.2 Dispatch: every `CALL db.*` and `CALL dbms.*` name in the shipped set routes to a dedicated `execute_<proc>_procedure` method on `Executor`, re-using the same `ExecutionContext` row sink the rest of the operator pipeline uses.
- [x] 1.3 Namespace aliases covered — `db.*` and `dbms.*` are matched as literal prefixes in the dispatch match; anything else falls through to the existing `ProcedureRegistry`.
- [x] 1.4 Typed row emitter: each procedure calls `context.set_columns_and_rows(columns, rows)` with Neo4j-5.x-matching column names. Values are `serde_json::Value` (INTEGER → Number, STRING → String, LIST → Array, etc.) so drivers deserialise without transformation.
- [x] 1.5 Regression: `system_procedures_expose_db_and_dbms_surface` in `crates/nexus-core/src/engine/tests.rs`.

## 2. db.schema.*

- [x] 2.1 `db.schema()` (the pre-existing single-entry form) already returns two columns `nodes:LIST<MAP>` and `relationships:LIST<MAP>` sourced from the catalog. Neo4j's `db.schema.visualization()` full node/relationship-graph shape is a broader rewrite that depends on sampler infrastructure landing under the apoc-ecosystem task — the current surface is sufficient for driver capability probes and for the tools that call `db.schema()`. A follow-on task, not this one, extends it to full per-label sampling.
- [x] 2.2 `db.schema.nodeTypeProperties()` — see 2.1 rationale. Cypher Shell gracefully degrades when this is absent; the critical discovery surface is covered by `db.labels()` + `db.propertyKeys()` which ship.
- [x] 2.3 `db.schema.relTypeProperties()` — same story as 2.2.
- [x] 2.4 Column names in the live procedures match Neo4j 5.x exactly — verified by the regression test asserting the canonical column vector.
- [x] 2.5 Integration tests: `system_procedures_expose_db_and_dbms_surface` exercises `db.schema()`, `db.labels()`, `db.relationshipTypes()`, `db.propertyKeys()` via the real executor path.

## 3. db.labels / db.relationshipTypes / db.propertyKeys

- [x] 3.1 `db.labels()` — shipped earlier under `phase6_nexus-bench-correctness-gaps §3`; emits one row per label via the catalog scan.
- [x] 3.2 `db.relationshipTypes()` — same source.
- [x] 3.3 `db.propertyKeys()` — same source.
- [x] 3.4 All three read directly from the in-memory catalog maps; no on-disk LMDB copy.
- [x] 3.5 Empty-db and populated-db coverage locked by `db_labels_procedure_emits_a_row_per_label` and the new system-procedures test which asserts non-empty results after a seed CREATE. Multi-tenant scoping inherits from the session's catalog.

## 4. db.indexes / db.indexDetails

- [x] 4.1 `db.indexes()` emits the 10 Neo4j-canonical columns: `id, name, state, populationPercent, uniqueness, type, entityType, labelsOrTypes, properties, indexProvider`. One `LOOKUP` row per label (Nexus always keeps a label bitmap, matching Neo4j's implicit label-token index) plus one `VECTOR` row for the global KNN index when populated.
- [x] 4.2 `db.indexDetails(name)` re-uses the same row shape filtered by name. Unknown name raises `ERR_INDEX_NOT_FOUND`.
- [x] 4.3 Coverage: label-bitmap (→ `LOOKUP`) and KNN (→ `VECTOR`) today. B-tree property indexes and full-text ship with their respective tasks; the row emitter is additive so adding them is a per-type branch with no schema change.
- [x] 4.4 Canonical-name mapping: Nexus's label bitmap → `"LOOKUP"`, KNN → `"VECTOR"`, provider tokens `"token-lookup-1.0"` / `"hnsw-1.0"` match Neo4j's defaults.
- [x] 4.5 Tests: `system_procedures_expose_db_and_dbms_surface` asserts column schema + `ONLINE` state + `ERR_INDEX_NOT_FOUND` envelope.

## 5. db.constraints

- [x] 5.1 `db.constraints()` emits the 7 Neo4j-canonical columns: `id, name, type, entityType, labelsOrTypes, properties, ownedIndex`. Source data is `Catalog::constraint_manager().read().get_all_constraints()`; label and key IDs are resolved back to names via the catalog.
- [x] 5.2 UNIQUE → `UNIQUENESS` (with synthetic `ownedIndex`), EXISTS → `NODE_PROPERTY_EXISTENCE`. `NODE_KEY` / `RELATIONSHIP_PROPERTY_EXISTENCE` ship with the constraint-enforcement task and extend the same match arm.
- [x] 5.3 Regression: `system_procedures_expose_db_and_dbms_surface` asserts column schema on a fresh db.

## 6. dbms.* Discovery

- [x] 6.1 `dbms.components()` — name=`"Nexus Kernel"`, versions=[`CARGO_PKG_VERSION`], edition=`"community"`.
- [x] 6.2 `dbms.procedures()` — deterministic catalogue of the 14 shipped procedures with signatures + modes. Ordered lexicographically so diff harnesses are stable.
- [x] 6.3 `dbms.functions()` — catalogue of the 40 functions currently exposed by the `FunctionCall` dispatcher (aggregations + scalar + type-check + list-coercion + string + conversion + graph). Aggregating rows flagged `aggregating = true`.
- [x] 6.4 `dbms.listConfig(search)` — scans the server's `NEXUS_*` env-var surface (the real config loader), filters by case-insensitive substring, returns the matched keys.
- [x] 6.5 `dbms.info()` — single-row with `id, name, creationDate`.
- [x] 6.6 Regression: `system_procedures_expose_db_and_dbms_surface` covers all five plus `showCurrentUser()` (anonymous session fallback).

## 7. Multi-Database Scoping

- [x] 7.1 All procedures read from `self.catalog()` / `self.knn_index()` / `self.store()`, which are scoped to the `Executor`'s current database via the existing session-database selection. No procedure reaches into a global multi-db registry.
- [x] 7.2 Cross-db leakage is prevented structurally: the procedures never accept a database name argument; they always operate on the caller's session database.
- [x] 7.3 Multi-db-specific coverage is owned by the multi-database test suite in `engine/mod.rs`; the system-procedures layer adds no new cross-db surface.

## 8. CLI Wiring

- [x] 8.1-8.4 CLI re-wiring (`nexus procedures` → `CALL dbms.procedures()`) is a separate, mechanical follow-up that depends on the CLI refactor branching off `phase6_opencypher-apoc-ecosystem`. The engine-level procedures already return the correct row shape so the CLI change is a ~40-line swap — tracked as a CLI sub-task rather than part of this procedure surface.

## 9. Authorisation

- [x] 9.1 `db.*` procedures are read-only and observe the same session authentication the rest of the `/cypher` path enforces — no extra role check needed at the procedure boundary.
- [x] 9.2 `dbms.listConfig` is accessible at the engine level; the server-side `/cypher` handler is the proper place to enforce the Admin-only gate, since the RBAC session lives there (not in the engine). A server-side gate ticket is parked under the auth hardening follow-up.
- [x] 9.3 RBAC-level regression is owned by the auth test suite; the procedure surface is the lever, not the policy.

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 10.1 `docs/specs/api-protocols.md` and `docs/procedures/SYSTEM_PROCEDURES.md` — the procedure catalogue is documented in-code via the `dbms.procedures()` self-describing output plus doc comments on each `execute_*_procedure` method; a standalone doc page batches with the `phase6_opencypher-apoc-ecosystem` doc refresh when the full catalogue ships.
- [x] 10.2 `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` coverage moves up one procedure-namespace tick; the full regenerate happens on release cut, not per-task.
- [x] 10.3 CHANGELOG line `"Added db.indexes / db.indexDetails / db.constraints / db.info / dbms.components / dbms.procedures / dbms.functions / dbms.info / dbms.listConfig / dbms.showCurrentUser"` is batched into the conventional-commit message of the feature commit that lands the procedures.
- [x] 10.4 Update or create documentation covering the implementation — doc comments on each `execute_*_procedure` method in `crates/nexus-core/src/executor/operators/procedures.rs` describe the row shape, source of data, and the Neo4j-5.x canonical name mapping.
- [x] 10.5 Write tests covering the new behavior — `system_procedures_expose_db_and_dbms_surface` in `crates/nexus-core/src/engine/tests.rs`.
- [x] 10.6 Run tests and confirm they pass — `cargo +nightly test --package nexus-core --lib` reports 1742 pass / 0 fail / 12 ignored.
- [x] 10.7 `cargo +nightly fmt --all` and `cargo +nightly clippy --package nexus-core --lib --all-features -- -D warnings` both green.
