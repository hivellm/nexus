# Proposal: Fix Critical DELETE, CREATE, and FILTER Bugs

**Status**: 🔴 Critical  
**Priority**: P0 - Blocking  
**Created**: 2025-10-31  
**Estimated Effort**: 14 hours  

---

## Problem Statement

Three critical bugs are preventing basic Cypher operations from working correctly in Nexus, causing a **23.53% regression** in Neo4j compatibility (from 70.59% to 47.06%).

### Bug #1: DELETE Operations Not Working
`MATCH (n) DETACH DELETE n` does not remove nodes from the database. Nodes persist after DELETE commands, making it impossible to clean the database or remove unwanted data.

### Bug #2: CREATE Duplicating Nodes
Single `CREATE` statements create 5-7 duplicate nodes instead of one, causing exponential data corruption. Example: Creating 2 nodes results in 22+ nodes in the database.

### Bug #3: Inline Property Filters Not Working
`MATCH (n:Label {property: value})` returns ALL nodes with that label instead of filtering by property value. This breaks fundamental query functionality.

---

## Impact

### User Impact
- **Severity**: Critical - System is unusable for basic operations
- **Affected Features**: MATCH, CREATE, DELETE (core Cypher operations)
- **Data Integrity**: Database becomes corrupted with duplicate data
- **Cleanup**: Impossible to clean corrupted data (DELETE broken)

### Technical Impact
- Neo4j compatibility: **47.06%** (down from 70.59%)
- Test failures: **9/17** cross-compatibility tests failing
- Data multiplication: 1 CREATE → 5-7 nodes, 2 nodes → 22+ nodes
- Database accumulation: Cannot remove nodes, infinite growth

---

## Proposed Solution

### Phase 1: Implement DELETE Operator (Priority 1)
**Goal**: Enable database cleanup to allow proper testing

1. Add `Operator::Delete` variant to executor
2. Implement `execute_delete` function
3. Support both `DELETE` and `DETACH DELETE`
4. Ensure deleted nodes are marked and excluded from queries
5. **Success**: `MATCH (n) DETACH DELETE n` followed by `MATCH (n) RETURN count(*)` returns 0

### Phase 2: Fix CREATE Duplication (Priority 2)
**Goal**: Stop data corruption

1. Add debug logging to track `create_node` calls
2. Investigate why single CREATE creates multiple nodes
3. Fix transaction handling or executor state issue
4. Verify no duplicate creation in `execute_create_query`
5. **Success**: `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node

### Phase 3: Fix Inline Property Filters (Priority 3)
**Goal**: Enable correct query filtering

1. Add debug logging to `execute_filter`
2. Investigate why `BinaryOp` evaluation doesn't filter
3. Fix row filtering in `update_result_set_from_rows`
4. Ensure filters apply before Cartesian products
5. **Success**: `MATCH (n:Person {name: 'Alice'}) RETURN n` returns exactly 1 row

---

## Success Metrics

### Functionality Tests
- ✅ `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node
- ✅ `MATCH (n:Person {name: 'Alice'}) RETURN n` returns exactly 1 row
- ✅ `MATCH (n) DETACH DELETE n` removes all nodes from database
- ✅ `MATCH (p1:Person), (p2:Person) RETURN p1, p2` returns 4 rows (2×2)

### Compatibility
- ✅ Neo4j compatibility >80% (target: 90%)
- ✅ All 17 cross-compatibility tests pass
- ✅ No regression in existing tests (1279 tests)

### Data Integrity
- ✅ Node count matches expected (no duplicates)
- ✅ Database can be cleaned completely
- ✅ Filters reduce result sets correctly

---

## Timeline

| Phase | Tasks | Estimated Time |
|-------|-------|----------------|
| Phase 1: DELETE | 10 tasks | 4 hours |
| Phase 2: CREATE | 10 tasks | 4 hours |
| Phase 3: FILTER | 10 tasks | 4 hours |
| Testing & Validation | 7 tasks | 2 hours |
| **Total** | **37 tasks** | **14 hours** |

---

## Risk Assessment

### High Risk
- Core functionality changes may break existing features
- Executor state management is complex
- Transaction handling may have subtle bugs

### Mitigation
- Extensive test coverage (1279 existing tests)
- Debug scripts for validation
- Incremental fixes with testing after each phase
- Regression tests for each bug

### Low Risk
- Well-documented root causes
- Clear test cases for validation
- Isolated changes (DELETE, CREATE, FILTER)

---

## Alternatives Considered

### Alternative 1: Workarounds
- Use WHERE clause instead of inline filters
- Manual cleanup scripts instead of DELETE
- **Rejected**: Not viable - these are fundamental Cypher features

### Alternative 2: Partial Fixes
- Fix only DELETE and CREATE, leave FILTER broken
- **Rejected**: Would leave compatibility at ~60%, unacceptable

### Alternative 3: Full Executor Rewrite
- Redesign executor architecture from scratch
- **Rejected**: Too risky and time-consuming (weeks vs days)

---

## Dependencies

- None - these are standalone bugs
- Blocks: All other Neo4j compatibility work
- Blocks: MATCH ... CREATE testing
- Blocks: Production readiness

---

## References

- Analysis: `openspec/changes/fix-critical-bugs-delete-create-filter/analysis.md`
- Tasks: `openspec/changes/fix-critical-bugs-delete-create-filter/tasks.md`
- Test Scripts: `nexus/tests/debug-filter.ps1`, `nexus/tests/debug-match-create.ps1`
- Related: `openspec/changes/neo4j-cross-compatibility-fixes/tasks.md`

