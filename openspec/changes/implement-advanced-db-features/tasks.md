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
- [ ] 3.4 Add tests (pending - basic parsing and execution works)

## 4. Named Paths
- [x] 4.1 Path variable assignment (p = (a)-[*]-(b)) ✅
- [x] 4.2 Path operations on named paths ✅ (path variable stored and accessible in RETURN)
- [ ] 4.3 Add tests (pending - parsing and execution works)

## 5. Quality
- [ ] 5.1 95%+ coverage
- [ ] 5.2 No clippy warnings
- [ ] 5.3 Update documentation
