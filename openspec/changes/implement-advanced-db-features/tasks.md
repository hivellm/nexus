# Tasks - Advanced DB Features

## 1. USE DATABASE
- [x] 1.1 USE DATABASE parsing ✅
- [x] 1.2 Database context switching ✅ (validates database exists, returns success message)
- [x] 1.3 Session state management ✅ (client uses database field in subsequent requests)
- [x] 1.4 Add tests ✅ (3 tests: success, nonexistent error, default database)

## 2. CREATE OR REPLACE
- [ ] 2.1 CREATE OR REPLACE procedures (deferred - procedures not implemented yet)
- [x] 2.2 CREATE OR REPLACE indexes ✅
- [x] 2.3 Upsert semantics ✅ (MERGE already supports upsert)
- [x] 2.4 Add tests ✅ (3 tests: replace existing, replace nonexistent, parsing)

## 3. Subquery Support
- [x] 3.1 CALL {...} subquery parsing ✅
- [x] 3.2 Correlated subqueries ✅ (basic support - subqueries can access outer context variables)
- [x] 3.3 CALL {...} IN TRANSACTIONS ✅ (parsing and execution with batching implemented)
- [x] 3.4 Add tests ✅ (7 tests: parsing, IN TRANSACTIONS parsing, batch size parsing, execution)

## 4. Named Paths
- [x] 4.1 Path variable assignment (p = (a)-[*]-(b)) ✅
- [x] 4.2 Path operations on named paths ✅ (path variable stored and accessible in RETURN)
- [x] 4.3 Add tests ✅ (3 tests: parsing, variable-length parsing, execution)

## 5. Transaction Session Management
- [x] 5.1 Session-based transaction persistence ✅
- [x] 5.2 Multiple operations in transaction ✅
- [x] 5.3 Error handling (COMMIT/ROLLBACK without BEGIN, double BEGIN) ✅
- [x] 5.4 Transaction rollback verification ✅
- [x] 5.5 Add comprehensive tests ✅ (10 tests covering all scenarios)

## 6. Quality
- [ ] 6.1 95%+ coverage (pending - need to run coverage check)
- [x] 6.2 No clippy warnings ✅
- [ ] 6.3 Update documentation (pending)
