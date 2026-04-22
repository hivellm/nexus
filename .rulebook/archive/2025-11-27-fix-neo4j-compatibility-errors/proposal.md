# Proposal: Fix Neo4j Compatibility Errors

## Why

The compatibility test suite revealed 23 test failures that prevent Nexus from achieving full Neo4j compatibility. These failures impact critical query patterns including MATCH with property filters, GROUP BY aggregations, DISTINCT operations, UNION queries, and relationship queries. Fixing these issues is essential to ensure Nexus can serve as a drop-in replacement for Neo4j in production environments. The current pass rate is 88.21% (172/195 tests), and addressing these failures will bring us closer to the 95%+ compatibility target required for production use.

## What Changes

This task addresses 23 specific compatibility issues identified in the test suite:

1. **MATCH queries with property filters** - Fix queries that match nodes by properties returning incorrect results (4 tests)
2. **GROUP BY aggregation** - Fix GROUP BY returning incorrect row counts (5 tests)
3. **DISTINCT operations** - Fix DISTINCT not properly filtering duplicate values (1 test)
4. **UNION queries** - Fix UNION and UNION ALL returning incorrect row counts (4 tests)
5. **Relationship queries** - Fix relationship aggregation and complex relationship queries (3 tests)
6. **Function calls with properties** - Fix functions like `properties()`, `labels()`, `keys()` not returning correct results (3 tests)
7. **NULL property access** - Fix handling of non-existent properties (1 test)
8. **String operations with properties** - Fix string functions with node properties (1 test)
9. **Array operations with properties** - Fix array operations with node properties (1 test)

Each issue requires investigation into the executor, query planner, or storage layer to identify the root cause and implement the fix while maintaining compatibility with existing functionality.

## Impact

- **Affected specs**: Cypher query execution specification, aggregation functions specification, relationship query specification
- **Affected code**: 
  - `nexus-core/src/executor/mod.rs` - Query execution and aggregation logic
  - `nexus-core/src/query/planner.rs` - Query planning and optimization
  - `nexus-core/src/storage/` - Storage layer for property access and indexing
  - `nexus-core/src/relationship/` - Relationship traversal and aggregation
- **Breaking change**: NO - This is a bug fix that improves compatibility
- **User benefit**: Enables Nexus to run more Neo4j-compatible queries, improving adoption and reducing migration friction

## Status

**Current Progress**: **192/195 passing (98.46%)** ✅ **EXCELLENT** - Above 95% target!

### Test Results Summary

- **Total Tests**: 195 compatibility tests
- **Passed**: 192
- **Failed**: 3 (all in Section 7 - Relationships)
- **Pass Rate**: **98.46%** ✅ **EXCELLENT** (above 95% target!)

### Progress by Phase

- ✅ **Phase 1** (MATCH Property Filters): 4/4 (100%) - COMPLETED
- ✅ **Phase 2** (GROUP BY Aggregation): 5/5 (100%) - COMPLETED
- ✅ **Phase 3** (UNION Queries): 10/10 (100%) - COMPLETED
- ✅ **Phase 4** (DISTINCT): 1/1 (100%) - COMPLETED
- ✅ **Phase 5** (Function Calls): 3/3 (100%) - COMPLETED
- ⚠️ **Phase 6** (Relationship Queries): 0/3 (0%) - IN PROGRESS
- ✅ **Phase 7** (Property Access): 2/2 (100%) - COMPLETED
- ✅ **Phase 8** (Array Operations): 1/1 (100%) - COMPLETED

### Remaining Issues

**Section 7: Relationship Queries** (3 tests failing):
- 7.19: `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person` — Expected: 2, Got: 1
- 7.25: `MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name` — Expected: 2, Got: 1
- 7.30: `MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year` — Expected: 3, Got: 1

**Root Causes Identified**:
- Expand operator may not be receiving all source nodes correctly
- Deduplication may be removing valid relationship rows when same source node has multiple relationships

**Fixes Applied**:
- Improved deduplication in `update_result_set_from_rows` to include relationship ID
- Added comprehensive debug logging to Expand operator
- Simplified database cleanup to use DETACH DELETE
- Fixed Aggregate `count(r)` to use `effective_row_count` when column not in result_set
- Fixed Aggregate GROUP BY when Project deferred - materialize rows from variables and evaluate projection expressions

**Next Steps**:
- Investigate why Expand is not finding relationships (40 relationships exist in catalog but MATCH finds 0)
- Verify CREATE relationship is persisting correctly
- Analyze why relationships are not being expanded correctly

**Documentation**:
- Comprehensive investigation report created: `docs/section7-relationship-tests-investigation-report.md`
- Report includes all tests performed, code changes made, investigation findings, and next steps

