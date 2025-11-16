# Tasks - Achieve 100% Neo4j Compatibility

**Status**: **IN PROGRESS** - Fixing remaining compatibility issues

**Priority**: **HIGH** - Critical for Neo4j compatibility and migration support

**Current Compatibility**: ~82% (166/199+ tests passing)

**Target**: 100% compatibility

## Overview

This task covers fixing all remaining compatibility issues identified through comprehensive testing (199+ tests) to achieve 100% compatibility with Neo4j query results.

## Implementation Checklist

### Phase 1: Aggregation Function Fixes

- [ ] 1.1 Fix `min()` without MATCH

  - [ ] 1.1.1 Issue: Returns null instead of literal value
  - [ ] 1.1.2 Root cause: Virtual row handling for min() with literals
  - [ ] 1.1.3 Fix: Ensure min() handles literal values correctly in virtual row
  - [ ] 1.1.4 Add test: `RETURN min(5) AS min_val` should return `5`
  - [ ] 1.1.5 Verify compatibility

- [ ] 1.2 Fix `max()` without MATCH

  - [ ] 1.2.1 Issue: Returns null instead of literal value
  - [ ] 1.2.2 Root cause: Virtual row handling for max() with literals
  - [ ] 1.2.3 Fix: Ensure max() handles literal values correctly in virtual row
  - [ ] 1.2.4 Add test: `RETURN max(15) AS max_val` should return `15`
  - [ ] 1.2.5 Verify compatibility

- [ ] 1.3 Fix `collect()` without MATCH

  - [ ] 1.3.1 Issue: Array comparison/return issue
  - [ ] 1.3.2 Root cause: Array serialization or comparison logic
  - [ ] 1.3.3 Fix: Ensure collect() returns proper array format
  - [ ] 1.3.4 Add test: `RETURN collect(1) AS collected` should return `[1]`
  - [ ] 1.3.5 Verify compatibility

- [ ] 1.4 Fix `sum()` with empty MATCH
  - [ ] 1.4.1 Issue: Returns null instead of 0
  - [ ] 1.4.2 Root cause: Empty result handling in sum()
  - [ ] 1.4.3 Fix: Return 0 for sum() on empty results
  - [ ] 1.4.4 Add test: `MATCH (n:NonExistent) RETURN sum(n.age) AS total` should return `0`
  - [ ] 1.4.5 Verify compatibility

### Phase 2: WHERE Clause Fixes

- [ ] 2.1 Fix WHERE with IN operator (data duplication)

  - [ ] 2.1.1 Issue: Count mismatch (Neo4j: 2, Nexus: 6) - likely data duplication
  - [ ] 2.1.2 Root cause: Test environment data not properly cleared or duplicated
  - [ ] 2.1.3 Fix: Ensure proper data isolation in tests OR fix actual IN operator logic
  - [ ] 2.1.4 Add test: `MATCH (n:Person) WHERE n.name IN ['Alice', 'Bob'] RETURN count(n)`
  - [ ] 2.1.5 Verify compatibility

- [ ] 2.2 Fix WHERE with empty IN list

  - [ ] 2.2.1 Issue: Returns all rows instead of 0
  - [ ] 2.2.2 Root cause: Empty list handling in IN operator
  - [ ] 2.2.3 Fix: Return false for `x IN []` (empty list)
  - [ ] 2.2.4 Add test: `MATCH (n:Person) WHERE n.name IN [] RETURN count(n)` should return `0`
  - [ ] 2.2.5 Verify compatibility

- [ ] 2.3 Fix WHERE with list contains (`IN` on property)
  - [ ] 2.3.1 Issue: `'dev' IN n.tags` not working correctly
  - [ ] 2.3.2 Root cause: Array property access and IN operator combination
  - [ ] 2.3.3 Fix: Support `value IN property_array` syntax
  - [ ] 2.3.4 Add test: `MATCH (n:Person) WHERE 'dev' IN n.tags RETURN count(n)`
  - [ ] 2.3.5 Verify compatibility

### Phase 3: ORDER BY Fixes

- [ ] 3.1 Fix ORDER BY DESC

  - [ ] 3.1.1 Issue: Not ordering correctly in descending order
  - [ ] 3.1.2 Root cause: DESC ordering logic in executor
  - [ ] 3.1.3 Fix: Implement proper DESC ordering
  - [ ] 3.1.4 Add test: `MATCH (n:Person) RETURN n.age ORDER BY n.age DESC LIMIT 3`
  - [ ] 3.1.5 Verify compatibility

- [ ] 3.2 Fix ORDER BY multiple columns

  - [ ] 3.2.1 Issue: Not ordering by multiple columns correctly
  - [ ] 3.2.2 Root cause: Multi-column ordering logic
  - [ ] 3.2.3 Fix: Implement proper multi-column ordering
  - [ ] 3.2.4 Add test: `MATCH (n:Person) RETURN n.name, n.age ORDER BY n.age, n.name LIMIT 3`
  - [ ] 3.2.5 Verify compatibility

- [ ] 3.3 Fix ORDER BY with WHERE

  - [ ] 3.3.1 Issue: Ordering not working correctly with WHERE clause
  - [ ] 3.3.2 Root cause: ORDER BY execution after WHERE filtering
  - [ ] 3.3.3 Fix: Ensure ORDER BY works correctly after WHERE
  - [ ] 3.3.4 Add test: `MATCH (n:Person) WHERE n.age > 25 RETURN n.name ORDER BY n.age DESC LIMIT 2`
  - [ ] 3.3.5 Verify compatibility

- [ ] 3.4 Fix ORDER BY with aggregation
  - [ ] 3.4.1 Issue: Ordering by aggregation result not working
  - [ ] 3.4.2 Root cause: ORDER BY with aggregated values
  - [ ] 3.4.3 Fix: Support ORDER BY with aggregation aliases
  - [ ] 3.4.4 Add test: `MATCH (n:Person) RETURN n.city, count(n) AS count ORDER BY count DESC LIMIT 2`
  - [ ] 3.4.5 Verify compatibility

### Phase 4: Property Access Fixes

- [ ] 4.1 Implement array property indexing

  - [ ] 4.1.1 Issue: `n.tags[0]` not working
  - [ ] 4.1.2 Root cause: Array indexing not implemented in property access
  - [ ] 4.1.3 Fix: Implement `property[index]` syntax in property access
  - [ ] 4.1.4 Add test: `MATCH (n:Person {name: 'Alice'}) RETURN n.tags[0] AS first_tag`
  - [ ] 4.1.5 Verify compatibility

- [ ] 4.2 Fix size() with array properties
  - [ ] 4.2.1 Issue: `size(n.tags)` returns null
  - [ ] 4.2.2 Root cause: size() function not handling array properties correctly
  - [ ] 4.2.3 Fix: Support size() on array properties
  - [ ] 4.2.4 Add test: `MATCH (n:Person {name: 'Alice'}) RETURN size(n.tags) AS size`
  - [ ] 4.2.5 Verify compatibility

### Phase 5: Aggregation with Collect Fixes

- [ ] 5.1 Fix collect() with head()/tail()/reverse()
  - [ ] 5.1.1 Issue: Row count mismatch (Neo4j: 1, Nexus: 5)
  - [ ] 5.1.2 Root cause: Aggregation not properly grouping when using collect() with list functions
  - [ ] 5.1.3 Fix: Ensure proper aggregation grouping
  - [ ] 5.1.4 Add test: `MATCH (n:Person) RETURN head(collect(n.name)) AS first_name`
  - [ ] 5.1.5 Verify compatibility

### Phase 6: Relationship Query Fixes

- [ ] 6.1 Fix relationship direction counting

  - [ ] 6.1.1 Issue: Count differences in directed vs undirected relationships
  - [ ] 6.1.2 Root cause: Relationship direction handling or data duplication
  - [ ] 6.1.3 Fix: Ensure correct relationship direction handling
  - [ ] 6.1.4 Add test: `MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS count`
  - [ ] 6.1.5 Verify compatibility

- [ ] 6.2 Fix bidirectional relationship counting

  - [ ] 6.2.1 Issue: Undirected relationship count mismatch (Neo4j: 2, Nexus: 6)
  - [ ] 6.2.2 Root cause: Bidirectional pattern matching or data duplication
  - [ ] 6.2.3 Fix: Ensure correct bidirectional relationship counting
  - [ ] 6.2.4 Add test: `MATCH (a)-[r:KNOWS]-(b) RETURN count(r) AS count`
  - [ ] 6.2.5 Verify compatibility

- [ ] 6.3 Fix multiple relationship types counting
  - [ ] 6.3.1 Issue: Count mismatch with multiple relationship types
  - [ ] 6.3.2 Root cause: Relationship type filtering in WHERE clause
  - [ ] 6.3.3 Fix: Ensure correct relationship type filtering
  - [ ] 6.3.4 Add test: `MATCH ()-[r]->() WHERE type(r) IN ['KNOWS', 'WORKS_AT'] RETURN count(r)`
  - [ ] 6.3.5 Verify compatibility

### Phase 7: Mathematical Operator Fixes

- [ ] 7.1 Fix power operator in WHERE clause

  - [ ] 7.1.1 Issue: `WHERE n.age = 2.0 ^ 5.0` not matching correctly
  - [ ] 7.1.2 Root cause: Power operator evaluation in WHERE clause
  - [ ] 7.1.3 Fix: Ensure power operator works correctly in WHERE
  - [ ] 7.1.4 Add test: `MATCH (n:Person) WHERE n.age = 2.0 ^ 5.0 RETURN count(n)`
  - [ ] 7.1.5 Verify compatibility

- [ ] 7.2 Fix modulo operator in WHERE clause

  - [ ] 7.2.1 Issue: `WHERE n.age % 5 = 0` count mismatch
  - [ ] 7.2.2 Root cause: Modulo operator evaluation in WHERE clause
  - [ ] 7.2.3 Fix: Ensure modulo operator works correctly in WHERE
  - [ ] 7.2.4 Add test: `MATCH (n:Person) WHERE n.age % 5 = 0 RETURN count(n)`
  - [ ] 7.2.5 Verify compatibility

- [ ] 7.3 Fix complex arithmetic expression precedence
  - [ ] 7.3.1 Issue: `(10 + 5) * 2 ^ 2` returns 900 instead of 60
  - [ ] 7.3.2 Root cause: Operator precedence not matching Neo4j
  - [ ] 7.3.3 Fix: Implement correct operator precedence (power before multiplication)
  - [ ] 7.3.4 Add test: `RETURN (10 + 5) * 2 ^ 2 AS result` should return `60`
  - [ ] 7.3.5 Verify compatibility

### Phase 8: String Function Edge Cases

- [ ] 8.1 Fix substring with negative start index
  - [ ] 8.1.1 Issue: `substring('hello', -2, 2)` returns row instead of empty result
  - [ ] 8.1.2 Root cause: Negative index handling in substring()
  - [ ] 8.1.3 Fix: Handle negative indices correctly (return empty string or error)
  - [ ] 8.1.4 Add test: `RETURN substring('hello', -2, 2) AS substr`
  - [ ] 8.1.5 Verify compatibility

### Phase 9: Test Environment Fixes

- [ ] 9.1 Fix data duplication in test environment
  - [ ] 9.1.1 Issue: Multiple tests showing data duplication (counts higher than expected)
  - [ ] 9.1.2 Root cause: Database not properly cleared between tests or test isolation issues
  - [ ] 9.1.3 Fix: Ensure proper database clearing and test isolation
  - [ ] 9.1.4 Verify: All tests run in clean environment
  - [ ] 9.1.5 Document: Test environment setup requirements

## Success Criteria

- ✅ All 199+ compatibility tests passing (100%)
- ✅ No regressions in existing functionality
- ✅ All edge cases handled correctly
- ✅ Performance benchmarks meet or exceed Neo4j for supported queries

## Progress Summary

**Last updated**: 2025-01-16

### Task Setup Completed

- ✅ Created `proposal.md` with overview of remaining compatibility fixes
- ✅ Created `specs/cypher-executor/spec.md` with detailed requirements
- ✅ Marked performance tests as slow tests (require `--features slow-tests`)
- ✅ Marked Neo4j comparison tests as slow tests (require `--features slow-tests`)
- ✅ Fixed all clippy warnings in test files

### Identified Issues (33 total)

**Aggregation Functions**: 4 issues

- min()/max() without MATCH
- collect() without MATCH
- sum() with empty MATCH

**WHERE Clause**: 3 issues

- IN operator data duplication
- Empty IN list
- List contains (IN on property)

**ORDER BY**: 4 issues

- DESC ordering
- Multiple columns
- With WHERE
- With aggregation

**Property Access**: 2 issues

- Array indexing
- size() with arrays

**Aggregation with Collect**: 1 issue

- collect() with list functions

**Relationships**: 3 issues

- Direction counting
- Bidirectional counting
- Multiple types

**Mathematical Operators**: 3 issues

- Power in WHERE
- Modulo in WHERE
- Operator precedence

**String Functions**: 1 issue

- Negative index handling

**Test Environment**: 1 issue

- Data duplication

### Next Steps

1. Prioritize fixes by impact (most common query patterns first)
2. Implement fixes incrementally
3. Run compatibility tests after each fix
4. Update documentation as fixes are completed

## Notes

- Focus on compatibility first, optimization can come later
- Use Neo4j as the reference implementation
- Test each fix individually before moving to the next
- Keep compatibility tests updated as fixes are implemented
- Document any intentional differences from Neo4j behavior
