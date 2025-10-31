# Implementation Tasks

## 1. IS NOT NULL Syntax (High Priority) ✅ COMPLETE
- [x] 1.1 Add `IS NULL` and `IS NOT NULL` to expression grammar in parser.rs
- [x] 1.2 Add new Expression variant for NULL checks (IsNull, IsNotNull)
- [x] 1.3 Implement evaluation logic in execute_filter
- [x] 1.4 Test `WHERE property IS NULL`
- [x] 1.5 Test `WHERE property IS NOT NULL`
- [x] 1.6 Re-enable test_where_null_check test
- [x] 1.7 Verify query returns count(5) instead of object
- [x] 1.8 Add expression_to_string support for IsNull in planner.rs
- [x] 1.9 Compatibility improved from 88.57% to 91.43%

## 2. WHERE Multiple AND Conditions (High Priority)
- [ ] 2.1 Verify parser handles `>=` and `<=` operators
- [ ] 2.2 Check AND expression parsing
- [ ] 2.3 Debug execute_filter for comparison predicates
- [ ] 2.4 Implement proper comparison logic for numeric values
- [ ] 2.5 Test single condition: `WHERE n.age >= 25`
- [ ] 2.6 Test AND condition: `WHERE n.age >= 25 AND n.age <= 35`
- [ ] 2.7 Verify query returns 5 nodes
- [ ] 2.8 Test all comparison operators: `<`, `<=`, `>`, `>=`, `=`, `<>`

## 3. Relationship Property Filtering (High Priority)
- [ ] 3.1 Verify relationship properties storage
- [ ] 3.2 Check relationship variable accessibility in WHERE
- [ ] 3.3 Debug filter evaluation for relationship properties
- [ ] 3.4 Implement comparison logic for relationship property access
- [ ] 3.5 Test equality: `WHERE r.since = 2015`
- [ ] 3.6 Test comparison: `WHERE r.since >= 2015`
- [ ] 3.7 Verify query returns 6 relationships

## 4. Two-Hop Graph Patterns (Medium Priority)
- [ ] 4.1 Analyze planner handling of multiple relationship patterns
- [ ] 4.2 Check intermediate node tracking
- [ ] 4.3 Debug why returning 5 results instead of 1
- [ ] 4.4 Verify no duplicate paths
- [ ] 4.5 Implement proper path tracking for multi-hop queries
- [ ] 4.6 Test two hops: `(a)-[]->(b)-[]->(c)`
- [ ] 4.7 Test three hops: `(a)-[]->(b)-[]->(c)-[]->(d)`
- [ ] 4.8 Verify query returns exactly 1 result

## 5. Testing and Validation
- [ ] 5.1 Add unit tests for comparison operators
- [ ] 5.2 Add unit tests for AND/OR logic
- [ ] 5.3 Add unit tests for relationship property access
- [ ] 5.4 Add unit tests for multi-hop patterns
- [ ] 5.5 Add unit tests for IS NULL / IS NOT NULL
- [ ] 5.6 Run extended-compatibility-test.ps1
- [ ] 5.7 Verify 100% compatibility (35/35 tests)
- [ ] 5.8 Check no regressions in existing tests

