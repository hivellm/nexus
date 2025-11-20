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

