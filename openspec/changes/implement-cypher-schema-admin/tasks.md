# Tasks - Schema & Administration

## 1. Index Management ⚪ NOT STARTED
- [ ] 1.1 CREATE INDEX parsing (Cypher syntax not implemented)
- [ ] 1.2 DROP INDEX parsing (Cypher syntax not implemented)
- [ ] 1.3 Index creation in catalog (Index infrastructure exists, but no Cypher commands)
- [ ] 1.4 Add tests

**Note**: Index infrastructure exists (LabelIndex, PropertyIndex, KnnIndex) but no Cypher commands to manage them.

## 2. Constraint Management ⚪ NOT STARTED
- [ ] 2.1 CREATE CONSTRAINT parsing (Cypher syntax not implemented)
- [ ] 2.2 DROP CONSTRAINT parsing (Cypher syntax not implemented)
- [ ] 2.3 Constraint enforcement (No constraint system implemented)
- [ ] 2.4 Add tests

**Note**: No constraint system exists yet. Need to implement uniqueness, existence, and type constraints.

## 3. Transaction Commands ⚪ NOT STARTED (Infrastructure exists)
- [ ] 3.1 BEGIN/COMMIT/ROLLBACK parsing (Cypher syntax not implemented)
- [ ] 3.2 Explicit transaction support (TransactionManager exists but no Cypher commands)
- [ ] 3.3 Add tests

**Note**: TransactionManager exists with begin_read(), begin_write(), commit(), abort() methods, but no Cypher commands to use them explicitly. Transactions are currently automatic.

## 4. Database Management ✅ PARTIALLY IMPLEMENTED
- [x] 4.1 SHOW DATABASES (Implemented via REST API: GET /management/databases)
- [x] 4.2 CREATE/DROP DATABASE (Implemented via REST API: POST/DELETE /management/databases)
- [ ] 4.3 Cypher syntax (CREATE DATABASE, DROP DATABASE, SHOW DATABASES not implemented in parser)
- [x] 4.4 Add tests (12 tests for DatabaseManager, 100% passing)

**Status**: Database management works via REST API but not via Cypher commands. DatabaseManager fully functional.

## 5. User Management ⚪ NOT STARTED (Infrastructure exists)
- [ ] 5.1 SHOW USERS (Cypher syntax not implemented)
- [ ] 5.2 CREATE USER (Cypher syntax not implemented)
- [ ] 5.3 GRANT/REVOKE (Cypher syntax not implemented)
- [ ] 5.4 Add tests

**Note**: Authentication and RBAC infrastructure exists (auth module) but no Cypher commands to manage users/permissions.

## 6. Quality ⚪ NOT STARTED
- [ ] 6.1 95%+ coverage (No Cypher commands implemented yet)
- [ ] 6.2 No clippy warnings (N/A - no code changes)
- [ ] 6.3 Update documentation (N/A - no implementation yet)
