# Fix Remaining Neo4j Compatibility Issues

**Date**: 2025-11-16  
**Status**: Draft  
**Priority**: HIGH

## Why

Current compatibility testing shows **96.5% Neo4j compatibility** (112/116 tests passing). While this is excellent, achieving **100% compatibility** is critical for:

1. **Drop-in replacement**: Applications migrating from Neo4j must work without code changes
2. **Enterprise adoption**: High-stakes deployments require complete compatibility guarantees
3. **Complex query support**: Edge cases often appear in production workloads
4. **Test coverage**: Comprehensive test suite validates all query patterns

The remaining 4 edge cases prevent 100% compatibility and represent real-world query patterns that need to work correctly:

1. **UNWIND with aggregation** - Common pattern for processing collections
2. **LIMIT after UNION** - Standard query composition pattern
3. **ORDER BY after UNION** - Essential for result sorting in combined queries
4. **Complex multi-label queries** - Production queries often use multiple labels with relationships

## What Changes

### 1. Fix UNWIND with Aggregation (test_distinct_labels)

**Problem**: `UNWIND` followed by aggregation requires operator reordering in the planner. Currently, the planner doesn't correctly handle the sequence when `UNWIND` creates rows that need to be aggregated.

**Solution**:
- Modify planner to detect `UNWIND` before aggregation patterns
- Reorder operators: execute `UNWIND` first, then apply aggregation
- Ensure `DISTINCT` works correctly with `UNWIND` results
- Fix `ORDER BY` application after `UNWIND` + aggregation

**Test Query**:
```cypher
MATCH (n) UNWIND labels(n) AS label
RETURN DISTINCT label
ORDER BY label
```

### 2. Fix LIMIT after UNION (test_union_with_limit)

**Problem**: `LIMIT` clause applied after `UNION` doesn't correctly limit the combined result set. The planner may be applying `LIMIT` to each branch separately instead of the final combined result.

**Solution**:
- Ensure `LIMIT` operator is placed after `UNION` in the operator chain
- Apply `LIMIT` to the combined result set, not individual branches
- Verify `LIMIT` works with `UNION ALL` (preserves duplicates)

**Test Query**:
```cypher
MATCH (a:A) RETURN a.n AS n
UNION
MATCH (b:B) RETURN b.n AS n
LIMIT 5
```

### 3. Fix ORDER BY after UNION (test_union_with_order_by)

**Problem**: `ORDER BY` clause applied after `UNION` doesn't correctly sort the combined result set. Similar to `LIMIT`, the planner may not be applying sorting to the final combined results.

**Solution**:
- Ensure `ORDER BY` operator is placed after `UNION` in the operator chain
- Apply sorting to the combined result set from both branches
- Verify sorting works with multiple columns and `DESC` ordering
- Support `ORDER BY` with `LIMIT` after `UNION`

**Test Query**:
```cypher
MATCH (a:A) RETURN a.name AS name
UNION
MATCH (b:B) RETURN b.name AS name
ORDER BY name
```

### 4. Fix Complex Multi-Label Queries (test_complex_multiple_labels_query)

**Problem**: `MATCH` queries with multiple labels (`:Person:Employee`) combined with relationships cause result duplication. The executor may be creating cartesian products instead of proper joins.

**Solution**:
- Fix label matching logic to correctly filter nodes with ALL specified labels
- Ensure relationship traversal works correctly with multi-label nodes
- Prevent duplicate results in multi-hop patterns
- Verify property access on relationships in WHERE clauses

**Test Query**:
```cypher
MATCH (p:Person:Employee)-[r:WORKS_AT]->(c:Company)
WHERE r.role = 'Developer'
RETURN p.name AS employee, c.name AS company, r.since AS started
```

### 5. Complete Nested Aggregations (head/tail/reverse with collect)

**Problem**: Partial implementation exists but tests are ignored. The planner detects nested aggregations but projection evaluation needs refinement.

**Solution**:
- Complete post-aggregation projection evaluation
- Ensure `head()`, `tail()`, and `reverse()` correctly access aggregation results
- Fix variable reference resolution in post-aggregation context
- Re-enable ignored tests once working

**Test Queries**:
```cypher
MATCH (n:Person) RETURN head(collect(n.name)) AS first_name
MATCH (n:Person) RETURN tail(collect(n.name)) AS remaining
MATCH (n:Person) RETURN reverse(collect(n.name)) AS reversed
```

## Impact

- **Affected specs**: `nexus-core` Cypher query execution
- **Affected code**:
  - `nexus-core/src/executor/planner.rs` - Operator ordering and UNION/LIMIT/ORDER BY placement
  - `nexus-core/src/executor/mod.rs` - UNWIND execution, aggregation with UNWIND, multi-label matching
  - `nexus-core/tests/neo4j_compatibility_test.rs` - Re-enable 4 ignored tests
  - `nexus-core/tests/test_collect_aggregation.rs` - Re-enable 3 ignored tests

- **Breaking**: No
- **Compatibility improvement**: From 96.5% (112/116) to 100% (116/116) Neo4j compatibility
- **Test impact**: 7 previously ignored tests will pass, no regressions expected

## Success Criteria

- ✅ All 4 ignored compatibility tests pass
- ✅ All 3 ignored collect aggregation tests pass
- ✅ 100% Neo4j compatibility achieved (116/116 tests)
- ✅ No regressions in existing tests
- ✅ Complex query patterns work correctly in production scenarios

