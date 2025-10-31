# Tasks: Fix Critical MATCH and CREATE Bugs

**Status**: 🔴 Critical  
**Priority**: Urgent  
**Started**: 2025-10-31  

---

## 1. Investigation & Setup

- [ ] 1.1 Add debug logging to `execute_filter` to track calls
- [ ] 1.2 Add debug logging to `evaluate_predicate_on_row` to track evaluations
- [ ] 1.3 Add debug logging to `create_node` to track node creation
- [ ] 1.4 Add debug logging to `execute_create_query` to track duplicate calls
- [ ] 1.5 Search codebase for `DELETE` implementation
- [ ] 1.6 Document exact code paths for each bug

## 2. Fix DELETE Operations

- [ ] 2.1 Verify `Clause::Delete` is parsed correctly
- [ ] 2.2 Add `Operator::Delete` enum variant to planner
- [ ] 2.3 Implement `plan_delete` in planner to generate Delete operator
- [ ] 2.4 Implement `execute_delete` in executor
- [ ] 2.5 Ensure `RecordStore::delete_node` marks records as deleted
- [ ] 2.6 Implement `DETACH DELETE` (delete relationships first)
- [ ] 2.7 Add unit test: `test_delete_single_node`
- [ ] 2.8 Add unit test: `test_delete_all_nodes`
- [ ] 2.9 Add unit test: `test_detach_delete_with_relationships`
- [ ] 2.10 Verify `MATCH (n) DETACH DELETE n` returns count 0

## 3. Fix CREATE Duplication

- [ ] 3.1 Add counter to track `execute_create_query` invocations
- [ ] 3.2 Add counter to track `create_node` invocations
- [ ] 3.3 Investigate why CREATE creates multiple nodes
- [ ] 3.4 Fix root cause of duplication
- [ ] 3.5 Verify transaction commits only once per CREATE
- [ ] 3.6 Verify `refresh_executor` doesn't trigger duplicates
- [ ] 3.7 Add unit test: `test_create_single_node_exact_count`
- [ ] 3.8 Add unit test: `test_create_multiple_nodes_exact_count`
- [ ] 3.9 Verify `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node
- [ ] 3.10 Clean up garbage nodes in test database

## 4. Fix Inline Property Filters

- [ ] 4.1 Add logging to `execute_filter` for inline property predicates
- [ ] 4.2 Add logging to show filter input/output row counts
- [ ] 4.3 Investigate why `materialize_rows_from_variables` returns wrong data
- [ ] 4.4 Investigate why `evaluate_predicate_on_row` returns wrong boolean
- [ ] 4.5 Investigate why `update_result_set_from_rows` doesn't filter
- [ ] 4.6 Fix root cause of filter failure
- [ ] 4.7 Add unit test: `test_filter_inline_property_string`
- [ ] 4.8 Add unit test: `test_filter_inline_property_integer`
- [ ] 4.9 Add unit test: `test_filter_multiple_inline_properties`
- [ ] 4.10 Verify `MATCH (n:Person {name: 'Alice'}) RETURN n` returns exactly 1 row

## 5. Fix MATCH with Multiple Patterns

- [ ] 5.1 Investigate Cartesian product size (expected 4, actual 33)
- [ ] 5.2 Verify `NodeByLabel` doesn't create duplicates
- [ ] 5.3 Ensure filters apply before Cartesian product
- [ ] 5.4 Add unit test: `test_match_two_patterns_cartesian`
- [ ] 5.5 Add unit test: `test_match_two_patterns_with_filters`
- [ ] 5.6 Verify `MATCH (p1:Person), (p2:Person)` returns 4 rows (2x2)

## 6. Fix MATCH ... CREATE

- [ ] 6.1 Verify `execute_match_create_query` extracts correct node IDs
- [ ] 6.2 Verify `create_from_pattern_with_context` doesn't create duplicate nodes
- [ ] 6.3 Ensure relationships are created only for matched nodes
- [ ] 6.4 Add unit test: `test_match_create_single_relationship`
- [ ] 6.5 Add unit test: `test_match_create_no_duplicate_nodes`
- [ ] 6.6 Verify `MATCH (p1), (p2) CREATE (p1)-[:KNOWS]->(p2)` creates correct count

## 7. Integration Testing

- [ ] 7.1 Run `debug-filter.ps1` - all tests should pass
- [ ] 7.2 Run `debug-match-create.ps1` - all tests should pass
- [ ] 7.3 Run `test-compatibility.ps1` - target >80% pass rate
- [ ] 7.4 Verify node count accuracy across all tests
- [ ] 7.5 Verify relationship count accuracy across all tests
- [ ] 7.6 Run full test suite: `cargo test --workspace`

## 8. Documentation

- [ ] 8.1 Update CHANGELOG.md with bug fixes
- [ ] 8.2 Update README.md with new compatibility percentage
- [ ] 8.3 Document DELETE operator in Cypher reference
- [ ] 8.4 Add examples for inline property filtering
- [ ] 8.5 Document known limitations (if any remain)

---

## Success Criteria

- ✅ `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node
- ✅ `MATCH (n:Person {name: 'Alice'}) RETURN n` returns exactly 1 row
- ✅ `MATCH (n) DETACH DELETE n` followed by `MATCH (n) RETURN count(*)` returns 0
- ✅ `MATCH (p1:Person), (p2:Person) RETURN p1, p2` returns 4 rows (2 nodes × 2 nodes)
- ✅ `MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)` creates exactly 1 relationship and 0 new nodes
- ✅ Neo4j compatibility tests pass at >80%
- ✅ All unit tests pass
- ✅ All integration tests pass

---

## Priority Order

1. **DELETE** (highest priority - enables clean testing)
2. **CREATE duplication** (stops data corruption)
3. **Inline filters** (enables correct queries)
4. **MATCH ... CREATE** (depends on above fixes)

---

## Notes

- These bugs are **blocking** - no other work should proceed until fixed
- Test database must be wiped after DELETE is implemented
- Consider creating a clean test fixture for repeatable testing

