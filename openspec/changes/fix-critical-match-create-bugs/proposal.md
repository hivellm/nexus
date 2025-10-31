# Proposal: Fix Critical MATCH and CREATE Bugs

**Status**: 🔴 Critical  
**Priority**: Urgent  
**Created**: 2025-10-31  
**Author**: AI Assistant  

---

## Problem Statement

Three critical bugs were discovered during Neo4j compatibility testing that prevent basic Cypher queries from working correctly:

1. **Inline Property Filters Not Working**: `MATCH (n:Person {name: 'Alice'})` returns ALL Person nodes instead of filtering by name
2. **DETACH DELETE Not Working**: `MATCH (n) DETACH DELETE n` does not remove nodes from the database
3. **CREATE Duplicating Nodes**: `CREATE` statements are creating multiple duplicate nodes instead of one

### Impact

- **Compatibility**: Neo4j compatibility dropped from 70% to 47% due to these bugs
- **Data Integrity**: Database accumulates garbage data that cannot be cleaned
- **MATCH ... CREATE**: Broken completely - creates exponential duplicates (2 nodes → 22 nodes → 27 nodes)

---

## Current Situation

### Bug 1: Inline Filters (`{property: value}`)

**Expected Behavior**:
```cypher
CREATE (p:Person {name: 'Alice', age: 30})
CREATE (p:Person {name: 'Bob', age: 25})
MATCH (p:Person {name: 'Alice'}) RETURN p
-- Should return 1 row (Alice)
```

**Actual Behavior**:
```
Returns 7 rows (all Person nodes + duplicates)
```

**Root Cause**:
- Planner creates `Filter` operators with predicates like `"p.name = \"Alice\""`
- Executor's `evaluate_projection_expression` HAS `BinaryOp` support
- BUT filters are not being applied correctly to reduce the result set

### Bug 2: DETACH DELETE

**Expected Behavior**:
```cypher
MATCH (n) DETACH DELETE n
-- Should remove all nodes and relationships
```

**Actual Behavior**:
```
Nodes persist after DELETE
Database shows 31+ nodes after "clean" operation
```

**Root Cause**:
- `DELETE` operator may not be implemented
- Or node deletion is not marking records as deleted correctly

### Bug 3: CREATE Duplication

**Expected Behavior**:
```cypher
CREATE (p:Person {name: 'Alice'})
-- Should create 1 node
```

**Actual Behavior**:
```
Creates multiple nodes (observed: 5-7 nodes created for 1 CREATE)
```

**Root Cause**:
- Unknown - requires investigation
- May be related to executor state or transaction handling

---

## Proposed Solution

### Phase 1: Investigate & Document (2 hours)

1. Add debug logging to `execute_filter` to see if filters are being called
2. Add debug logging to `CREATE` to track node creation
3. Add debug logging to `DELETE` to verify it's implemented
4. Document exact code paths for each bug

### Phase 2: Fix Inline Filters (4 hours)

1. Verify `execute_filter` is correctly parsing `BinaryOp` expressions
2. Verify `evaluate_projection_expression` correctly evaluates `p.name = "Alice"`
3. Ensure filtered rows properly update `context.result_set`
4. Add unit tests for inline property filtering

### Phase 3: Fix DETACH DELETE (3 hours)

1. Implement `DELETE` operator if missing
2. Ensure `RecordStore::delete_node` marks nodes as deleted
3. Ensure deleted nodes are not returned in subsequent queries
4. Add unit tests for DELETE operations

### Phase 4: Fix CREATE Duplication (3 hours)

1. Investigate why `execute_create_query` creates duplicates
2. Check if `refresh_executor` is causing issues
3. Ensure transaction commit happens only once
4. Add unit tests for single CREATE statements

### Phase 5: Integration Testing (2 hours)

1. Run full Neo4j compatibility test suite
2. Verify `MATCH ... CREATE` works correctly
3. Verify clean database after `DETACH DELETE`
4. Update compatibility percentage

---

## Success Metrics

- ✅ `MATCH (n:Person {name: 'Alice'}) RETURN p` returns exactly 1 row
- ✅ `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node
- ✅ `MATCH (n) DETACH DELETE n` followed by `MATCH (n) RETURN count(*)` returns 0
- ✅ Neo4j compatibility tests pass at >80%
- ✅ `MATCH ... CREATE` creates correct number of relationships

---

## Alternatives Considered

1. **Workaround with WHERE clause**: Not viable - inline filters are standard Cypher
2. **Manual cleanup scripts**: Not viable - DELETE must work for data integrity
3. **Disable MATCH ... CREATE**: Not viable - critical Neo4j feature

---

## Timeline

- **Investigation**: 2 hours
- **Implementation**: 10 hours
- **Testing**: 2 hours
- **Total**: ~14 hours (2 days)

---

## Risks

- **High**: These bugs affect core functionality - any fix could break existing features
- **Medium**: May require significant refactoring of executor logic
- **Low**: Well-tested with existing test suite

---

## Dependencies

- None (critical path item)

---

## References

- Test failures: `nexus/tests/cross-compatibility/test-compatibility.ps1`
- Debug scripts: `nexus/tests/debug-filter.ps1`, `nexus/tests/debug-match-create.ps1`
- Related code: `nexus-core/src/executor/mod.rs`, `nexus-core/src/executor/planner.rs`

