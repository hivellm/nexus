# Tasks - Fix Remaining Neo4j Compatibility Issues

**Status**: 🔄 IN PROGRESS  
**Priority**: HIGH  
**Target**: 100% Neo4j compatibility (116/116 tests passing)

## 1. Fix UNWIND with Aggregation

- [ ] 1.1 Analyze current UNWIND operator execution flow
- [ ] 1.2 Identify where aggregation should be applied (before/after UNWIND)
- [ ] 1.3 Modify planner to detect UNWIND + aggregation patterns
- [ ] 1.4 Reorder operators: UNWIND → Aggregation → DISTINCT → ORDER BY
- [ ] 1.5 Fix DISTINCT application to UNWIND results
- [ ] 1.6 Fix ORDER BY application after UNWIND + aggregation
- [ ] 1.7 Re-enable `test_distinct_labels` test
- [ ] 1.8 Verify test passes and no regressions

## 2. Fix LIMIT after UNION

- [ ] 2.1 Analyze current UNION operator execution
- [ ] 2.2 Identify where LIMIT is currently applied (per branch vs combined)
- [ ] 2.3 Modify planner to place LIMIT after UNION operator
- [ ] 2.4 Ensure LIMIT applies to combined result set from both branches
- [ ] 2.5 Verify LIMIT works with UNION ALL (preserves duplicates)
- [ ] 2.6 Test LIMIT with different result sizes from each branch
- [ ] 2.7 Re-enable `test_union_with_limit` test
- [ ] 2.8 Verify test passes and no regressions

## 3. Fix ORDER BY after UNION

- [ ] 3.1 Analyze current ORDER BY operator placement
- [ ] 3.2 Identify where ORDER BY is currently applied (per branch vs combined)
- [ ] 3.3 Modify planner to place ORDER BY after UNION operator
- [ ] 3.4 Ensure ORDER BY applies to combined result set from both branches
- [ ] 3.5 Support ORDER BY with multiple columns after UNION
- [ ] 3.6 Support ORDER BY DESC after UNION
- [ ] 3.7 Support ORDER BY + LIMIT combination after UNION
- [ ] 3.8 Re-enable `test_union_with_order_by` test
- [ ] 3.9 Verify test passes and no regressions

## 4. Fix Complex Multi-Label Queries

- [ ] 4.1 Analyze current multi-label matching logic
- [ ] 4.2 Identify root cause of result duplication
- [ ] 4.3 Fix label matching to require ALL specified labels (AND logic, not OR)
- [ ] 4.4 Ensure relationship traversal works with multi-label nodes
- [ ] 4.5 Fix WHERE clause property access on relationships
- [ ] 4.6 Prevent duplicate results in multi-hop patterns
- [ ] 4.7 Add comprehensive tests for multi-label + relationship patterns
- [ ] 4.8 Re-enable `test_complex_multiple_labels_query` test
- [ ] 4.9 Verify test passes and no regressions

## 5. Complete Nested Aggregations

- [ ] 5.1 Analyze current post-aggregation projection implementation
- [ ] 5.2 Fix variable reference resolution in post-aggregation context
- [ ] 5.3 Ensure aggregation result aliases are accessible to subsequent operators
- [ ] 5.4 Fix `head()` function evaluation with aggregation results
- [ ] 5.5 Fix `tail()` function evaluation with aggregation results
- [ ] 5.6 Fix `reverse()` function evaluation with aggregation results
- [ ] 5.7 Add comprehensive tests for nested aggregation patterns
- [ ] 5.8 Re-enable `test_collect_with_head` test
- [ ] 5.9 Re-enable `test_collect_with_tail` test
- [ ] 5.10 Re-enable `test_collect_with_reverse` test
- [ ] 5.11 Verify all tests pass and no regressions

## 6. Comprehensive Testing

- [ ] 6.1 Run full Neo4j compatibility test suite
- [ ] 6.2 Verify 100% pass rate (116/116 tests)
- [ ] 6.3 Run extended compatibility tests (200+ queries)
- [ ] 6.4 Test complex production-like query patterns
- [ ] 6.5 Verify no performance regressions
- [ ] 6.6 Update compatibility documentation

## Progress Summary

**Last Updated**: 2025-11-16  
**Current Status**: 96.5% compatibility (112/116 tests passing)  
**Target**: 100% compatibility (116/116 tests passing)

### Known Issues

1. **UNWIND with aggregation** - Operator reordering needed
2. **LIMIT after UNION** - LIMIT not applied to combined results
3. **ORDER BY after UNION** - ORDER BY not applied to combined results
4. **Multi-label queries** - Result duplication bug
5. **Nested aggregations** - Post-aggregation projection needs completion

### Test Coverage

- **Current**: 112/116 tests passing (96.5%)
- **Ignored**: 4 compatibility tests + 3 aggregation tests = 7 total
- **Target**: 116/116 tests passing (100%)

