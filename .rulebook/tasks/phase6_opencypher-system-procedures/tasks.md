# Implementation Tasks — System Procedures

## 1. Registry & Dispatch

- [ ] 1.1 Create `nexus-core/src/procedures/system/mod.rs` with a `SystemProcRegistry`
- [ ] 1.2 Wire `CALL db.*` and `CALL dbms.*` into the procedure operator
- [ ] 1.3 Support optional namespace aliases (`db` / `dbms`) as reserved roots
- [ ] 1.4 Add a typed row emitter with Neo4j-compatible column types
- [ ] 1.5 Unit tests covering dispatch

## 2. db.schema.*

- [ ] 2.1 `db.schema.visualization()` → nodes + relationships of schema graph
- [ ] 2.2 `db.schema.nodeTypeProperties()` → all node property descriptors
- [ ] 2.3 `db.schema.relTypeProperties()` → all relationship property descriptors
- [ ] 2.4 Ensure column names/types match Neo4j 5.x exactly
- [ ] 2.5 Integration tests against Neo4j diff harness

## 3. db.labels / db.relationshipTypes / db.propertyKeys

- [ ] 3.1 `db.labels()` → stream of labels
- [ ] 3.2 `db.relationshipTypes()` → stream of relationship types
- [ ] 3.3 `db.propertyKeys()` → stream of property keys
- [ ] 3.4 Source data from the existing catalog (LMDB) without copy
- [ ] 3.5 Tests: empty db, populated db, multi-tenant isolation

## 4. db.indexes / db.indexDetails

- [ ] 4.1 `db.indexes()` with columns: name, labelsOrTypes, properties, state, type, uniqueness
- [ ] 4.2 `db.indexDetails(indexName)` single-row variant
- [ ] 4.3 Expose bitmap, B-tree, KNN, and full-text indexes (once shipped)
- [ ] 4.4 Map internal index types to Neo4j canonical names (BTREE, LOOKUP, etc.)
- [ ] 4.5 Tests covering every index type

## 5. db.constraints

- [ ] 5.1 `db.constraints()` with columns: name, description, type, ownedIndex
- [ ] 5.2 Emit UNIQUE, NODE_KEY, EXISTS entries when the constraint engine is present
- [ ] 5.3 Tests

## 6. dbms.* Discovery

- [ ] 6.1 `dbms.components()` → name, versions, edition
- [ ] 6.2 `dbms.procedures()` → full procedure catalogue
- [ ] 6.3 `dbms.functions()` → full function catalogue (already exists, wrap)
- [ ] 6.4 `dbms.listConfig(search)` → filtered config keys
- [ ] 6.5 `dbms.info()` → uptime, version, build
- [ ] 6.6 Tests

## 7. Multi-Database Scoping

- [ ] 7.1 All procedures scope to the current session database
- [ ] 7.2 Reject on cross-database leakage
- [ ] 7.3 Tests with two databases; no data should cross

## 8. CLI Wiring

- [ ] 8.1 `nexus procedures` → `CALL dbms.procedures()` via REST
- [ ] 8.2 `nexus schema` → `CALL db.schema.visualization()`
- [ ] 8.3 `nexus labels` → `CALL db.labels()`
- [ ] 8.4 Tests via CLI integration harness

## 9. Authorisation

- [ ] 9.1 All `db.*` require Reader or higher
- [ ] 9.2 `dbms.listConfig` and admin-scoped procedures require Admin
- [ ] 9.3 Tests with RBAC roles

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 10.1 Update `docs/specs/api-protocols.md` and add `docs/procedures/SYSTEM_PROCEDURES.md`
- [ ] 10.2 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 10.3 Add CHANGELOG.md entry "Added system procedures (db.*, dbms.*)"
- [ ] 10.4 Update or create documentation covering the implementation
- [ ] 10.5 Write tests covering the new behavior
- [ ] 10.6 Run tests and confirm they pass
- [ ] 10.7 Quality pipeline: fmt + clippy + ≥95% coverage
