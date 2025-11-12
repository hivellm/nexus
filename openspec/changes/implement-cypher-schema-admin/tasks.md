# Tasks - Schema & Administration

## 1. Index Management ✅ PARSING IMPLEMENTED (Execution placeholder)
- [x] 1.1 CREATE INDEX parsing (Cypher syntax implemented)
- [x] 1.2 DROP INDEX parsing (Cypher syntax implemented)
- [x] 1.3 Index creation in catalog (Catalog entries created, index structure managed automatically)
- [ ] 1.4 Add tests

**Status**: Parsing implemented. Execution creates catalog entries. Index structure is managed automatically when properties are indexed. Future: Explicit index structure management.

## 2. Constraint Management ✅ PARSING IMPLEMENTED (Execution placeholder)
- [x] 2.1 CREATE CONSTRAINT parsing (Cypher syntax implemented)
- [x] 2.2 DROP CONSTRAINT parsing (Cypher syntax implemented)
- [ ] 2.3 Constraint enforcement (Constraint system not yet implemented - returns error)
- [ ] 2.4 Add tests

**Status**: Parsing implemented. Execution returns error as constraint system not yet implemented. Future: Implement uniqueness, existence, and type constraints.

## 3. Transaction Commands ✅ PARSING IMPLEMENTED (Execution placeholder)
- [x] 3.1 BEGIN/COMMIT/ROLLBACK parsing (Cypher syntax implemented)
- [x] 3.2 Explicit transaction support (Placeholder - transactions are currently automatic)
- [ ] 3.3 Add tests

**Status**: Parsing implemented. BEGIN/COMMIT are placeholders (transactions are automatic). ROLLBACK returns error. Future: Implement explicit transaction context management.

## 4. Database Management ✅ PARSING IMPLEMENTED (Needs server-level execution)
- [x] 4.1 SHOW DATABASES (Implemented via REST API: GET /management/databases)
- [x] 4.2 CREATE/DROP DATABASE (Implemented via REST API: POST/DELETE /management/databases)
- [x] 4.3 Cypher syntax (CREATE DATABASE, DROP DATABASE, SHOW DATABASES parsing implemented)
- [x] 4.4 Add tests (12 tests for DatabaseManager, 100% passing)

**Status**: Parsing implemented. Execution must be handled at server level (Engine doesn't have access to DatabaseManager). Future: Implement server-level execution handler.

## 5. User Management ✅ PARSING IMPLEMENTED (Needs server-level execution)
- [x] 5.1 SHOW USERS (Cypher syntax implemented)
- [x] 5.2 CREATE USER (Cypher syntax implemented)
- [x] 5.3 GRANT/REVOKE (Cypher syntax implemented)
- [ ] 5.4 Add tests

**Status**: Parsing implemented. Execution must be handled at server level (Engine doesn't have access to auth module). Future: Implement server-level execution handler.

## 6. Quality ⚠️ IN PROGRESS
- [ ] 6.1 95%+ coverage (Parsing implemented, execution placeholders added, tests needed)
- [x] 6.2 No clippy warnings (All warnings fixed)
- [ ] 6.3 Update documentation (Documentation update needed)
