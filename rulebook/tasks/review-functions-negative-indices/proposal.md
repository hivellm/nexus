# Proposal: Review Functions with Negative Index and Related Issues

## Why

This proposal addresses critical compatibility and correctness issues in Nexus that prevent full Neo4j compatibility and cause test failures. The system currently has multiple functions with incorrect negative index handling, transaction rollback issues that leave inconsistent state, and query execution bugs that return incorrect results. These issues block several test suites from running and prevent users from relying on standard Cypher functionality. Fixing these issues is essential for achieving 100% Neo4j compatibility and ensuring data integrity in transaction rollback scenarios.

## What Changes

This proposal will fix:

1. **String Functions - Negative Index Handling**: Fix `substring()` function to correctly handle negative indices for start position and length parameters, ensuring compatibility with Neo4j behavior.

2. **Transaction Rollback Issues**: Fix rollback mechanism to properly remove nodes and relationships from indexes and storage when transactions are rolled back, maintaining index consistency.

3. **Query Execution Bugs**: Fix multiple query execution issues including:
   - DELETE with RETURN count(*) returning incorrect counts
   - Directed relationship matching with labels returning wrong counts
   - Multiple relationship types with RETURN clause not working correctly

4. **Array and String Function Review**: Review and verify all array and string functions for proper negative index support, including array slicing, array indexing, and related string manipulation functions.

5. **Test Suite Enablement**: Enable all currently ignored tests once fixes are implemented and verified.

## Impact

- **Affected specs**: 
  - `docs/specs/cypher-subset.md` - String and array function specifications
  - Transaction handling specifications
  - Query execution specifications

- **Affected code**:
  - `nexus-core/src/executor/mod.rs` - Main executor logic for functions and queries
  - `nexus-core/src/executor/parser.rs` - Parser for function calls
  - Transaction rollback handling code
  - Index management code

- **Breaking change**: NO - These are bug fixes that restore intended behavior

- **User benefit**: 
  - Full Neo4j compatibility for string and array functions
  - Correct transaction rollback behavior ensuring data consistency
  - Accurate query results for DELETE, relationship matching, and multi-type queries
  - All test suites passing, providing confidence in system correctness

