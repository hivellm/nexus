# Section 7 Relationship Tests Investigation Report

**Date**: 2025-01-20  
**Status**: In Progress  
**Issue**: 3 tests failing in Section 7 (Relationship Queries)

## Executive Summary

Three tests in Section 7 are failing with row count mismatches:
- **7.19**: `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person` — Expected: 2, Got: 1
- **7.25**: `MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name` — Expected: 2, Got: 1  
- **7.30**: `MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year` — Expected: 3, Got: 1

**Root Cause Identified**: Relationships exist in catalog (40 relationships reported) but are not being found by Expand operator during MATCH queries.

## Tests Performed

### 1. Relationship Count Tests

| Test Query | Expected | Actual | Status |
|------------|----------|--------|--------|
| `MATCH ()-[r:WORKS_AT]->() RETURN count(r)` | > 0 | 0 | ❌ FAIL |
| `MATCH ()-[r]->() RETURN count(r)` | > 0 | 0 | ❌ FAIL |
| `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN count(r)` | > 0 | 0 | ❌ FAIL |
| `MATCH (a:Person {name: 'Alice'})-[r]->() RETURN count(r)` | > 0 | 0 | ❌ FAIL |
| `MATCH (a:Person)-[r]->() RETURN count(r)` | > 0 | 0 | ❌ FAIL |

**Observation**: All relationship count queries return 0, even though catalog reports 40 relationships exist.

### 2. Relationship Retrieval Tests

| Test Query | Expected | Actual | Status |
|------------|----------|--------|--------|
| `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name` | 2 rows | 2 rows | ✅ PASS |
| `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a, r, b` | 2+ rows with r,b | 2 rows, r/b are null | ❌ FAIL |
| `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name, type(r), b.name` | 2+ rows | 2 rows, type/b are null | ❌ FAIL |

**Observation**: Node queries work correctly, but relationship objects are null when returned.

### 3. Node Query Tests

| Test Query | Expected | Actual | Status |
|------------|----------|--------|--------|
| `MATCH (a:Person) RETURN a.name ORDER BY a.name` | 2 rows (Alice, Bob) | 2 rows (Alice, Bob) | ✅ PASS |
| `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name` | 2 rows | 2 rows | ✅ PASS |
| `MATCH (a:Person {name: 'Alice'}) RETURN id(a)` | 285 | 285 | ✅ PASS |

**Observation**: Node retrieval works correctly. Alice has node_id 285.

### 4. CREATE Relationship Tests

| Test Query | Expected | Actual | Status |
|------------|----------|--------|--------|
| `MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1) RETURN count(*)` | 1 | 1 | ✅ PASS |
| After CREATE: `MATCH ()-[r:WORKS_AT]->() RETURN count(r)` | > 0 | 0 | ❌ FAIL |

**Observation**: CREATE relationship returns success, but relationships are not found by subsequent MATCH queries.

### 5. Stats Verification Tests

| Endpoint | Expected | Actual | Status |
|----------|----------|--------|--------|
| `/stats` catalog.rel_count | > 0 | 40 | ✅ PASS |
| `/stats` catalog.rel_type_count | > 0 | 2 | ✅ PASS |
| `/stats` catalog.node_count | > 0 | 288 | ✅ PASS |

**Observation**: Catalog correctly reports 40 relationships exist, but MATCH queries cannot find them.

## Code Changes Made

### 1. Aggregate Operator Fixes

**File**: `nexus-core/src/executor/mod.rs`

**Change 1**: Fixed `count(r)` when column not in result_set
- **Location**: Line ~3770
- **Change**: Modified `execute_aggregate_with_projections` to use `effective_row_count` when column for aggregation is not found in result_set
- **Reason**: Project operator removes relationship columns before Aggregate, so Aggregate needs to count rows instead
- **Status**: ✅ Committed

**Change 2**: Fixed GROUP BY when Project deferred
- **Location**: Lines ~3364-3373, ~3456-3483, ~3506-3530
- **Change**: 
  - Materialize rows from variables when GROUP BY and rows are empty
  - Evaluate projection expressions for GROUP BY columns when column not found
- **Reason**: When Project is deferred (because Aggregate exists in pipeline), Aggregate needs to materialize rows and evaluate expressions for GROUP BY
- **Status**: ✅ Committed

### 2. Deduplication Fixes

**File**: `nexus-core/src/executor/mod.rs`

**Change 3**: Improved deduplication for relationship rows
- **Location**: Lines ~8440-8490 (`update_result_set_from_rows`)
- **Change**: Modified deduplication to use combination of all entity IDs (source node + target node + relationship) for relationship rows
- **Reason**: Previous deduplication used only source node ID, causing valid distinct relationship rows to be removed
- **Status**: ✅ Committed

**Change 4**: Added `_nexus_id` to relationship objects
- **Location**: `read_relationship_as_value` function
- **Change**: Ensures `_nexus_id` is inserted into relationship objects for correct deduplication
- **Status**: ✅ Committed

### 3. Expand Operator Debug Logging

**File**: `nexus-core/src/executor/mod.rs`

**Change 5**: Added extensive debug logging
- **Location**: Lines ~2492-2527, ~2748-2752, ~2834-2890
- **Change**: Added `tracing::debug!` logs to track:
  - Number of input rows
  - Source node IDs being processed
  - Number of relationships found per source node
  - Number of expanded rows created
  - Final result_set row count
- **Status**: ✅ Committed (but logs not yet analyzed in production)

### 4. Database Cleanup Fixes

**File**: `scripts/test-neo4j-nexus-compatibility-200.ps1`

**Change 6**: Improved database cleanup
- **Location**: `Clear-Databases` function
- **Change**: Changed to use `MATCH (n) DETACH DELETE n` for both Neo4j and Nexus
- **Reason**: `DETACH DELETE` automatically removes relationships, preventing orphaned relationships
- **Status**: ✅ Committed

**Change 7**: Added cleanup before Section 2 and 3
- **Location**: Before Section 2 and Section 3 test execution
- **Change**: Added `Clear-Databases` and `Setup-TestData` calls to ensure clean state
- **Status**: ✅ Committed

## Investigation Findings

### 1. Expand Operator Flow Analysis

The Expand operator (`execute_expand`) has the following flow:

1. **Get input rows** (line 2475-2490):
   - Uses `result_set_as_rows` if `context.result_set.rows` is not empty
   - Otherwise materializes from `context.variables`

2. **For each input row**:
   - Extracts source node ID from `source_var`
   - Calls `find_relationships(source_id, type_ids, direction, cache)` (line 2737/2741/2745)

3. **find_relationships** tries multiple methods in order:
   - **Method 1**: `relationship_storage` (if optimizations enabled) - Line 6672-6701
   - **Method 2**: Adjacency list via `get_outgoing_relationships_adjacency` - Line 6705-6748
   - **Method 3**: Relationship index via cache - Line 6750-6827
   - **Method 4**: Linked list traversal using `first_rel_ptr` - Line 6829-6900+

### 2. CREATE Relationship Flow Analysis

When a relationship is created (`create_relationship` in `storage/mod.rs`):

1. **Creates relationship record** (line 616)
2. **Updates source node** `first_rel_ptr` (line 646)
3. **Updates target node** `first_rel_ptr` (line 659)
4. **Updates adjacency store** (line 680-697) if enabled
5. **Updates relationship_storage** (in executor, line 1869) if optimizations enabled

**Observation**: Multiple storage mechanisms are updated, but none seem to be working for retrieval.

### 3. Root Cause Hypothesis

**Primary Hypothesis**: The `find_relationships` function is not finding relationships because:

1. **relationship_storage** may not be initialized or populated correctly
2. **adjacency_store** may not be initialized or relationships not linked correctly
3. **Linked list traversal** may have incorrect `first_rel_ptr` values or broken chains
4. **Transaction visibility** - relationships may be created in a transaction that's not committed when MATCH queries run

**Secondary Hypothesis**: Relationships are being created but immediately deleted or marked as deleted.

### 4. Test Results Summary

| Component | Status | Notes |
|-----------|--------|-------|
| Node creation | ✅ Working | 288 nodes created successfully |
| Node queries | ✅ Working | Can find nodes by label and properties |
| Relationship creation | ⚠️ Partial | CREATE returns success, catalog shows 40 relationships |
| Relationship queries | ❌ Failing | MATCH cannot find any relationships |
| Catalog tracking | ✅ Working | Catalog correctly reports 40 relationships exist |

## Code Paths Investigated

### 1. Expand Operator → find_relationships

- ✅ Verified `find_relationships` is called with correct parameters
- ❌ `find_relationships` returns empty vector
- ❌ All methods (relationship_storage, adjacency, index, linked list) fail

### 2. CREATE Relationship → Storage Update

- ✅ Verified `create_relationship` updates `first_rel_ptr` on nodes
- ✅ Verified `create_relationship` updates adjacency store (if enabled)
- ✅ Verified executor updates `relationship_storage` (if optimizations enabled)
- ❌ Relationships still not found after creation

### 3. Scan Direct Relationships (No Source Node)

- ✅ Verified scan code path exists (line 2543-2544)
- ✅ Verified `relationship_count()` returns correct value
- ❌ Scan does not find relationships (all marked as deleted or wrong type)

## Known Issues

1. **Relationship Visibility**: Relationships created via CREATE are not visible in subsequent MATCH queries
2. **Multiple Storage Mechanisms**: Relationships are stored in multiple places (RecordStore, adjacency_store, relationship_storage) but retrieval fails from all
3. **Transaction Isolation**: Possible issue with transaction visibility - relationships may be created in one transaction but not visible in another

## Next Steps

### High Priority

1. **Verify Transaction Commit**: Check if relationships are committed to disk after CREATE
2. **Check Relationship Deletion**: Verify relationships are not being deleted immediately after creation
3. **Test Linked List Traversal**: Manually verify `first_rel_ptr` values on nodes and follow linked list
4. **Check Adjacency Store Initialization**: Verify `adjacency_store` is initialized and relationships are being added

### Medium Priority

5. **Enable Debug Logging**: Run tests with debug logs enabled to see Expand operator behavior
6. **Direct Storage Test**: Test reading relationships directly from storage files
7. **Relationship Storage Verification**: Test if `relationship_storage` is being populated correctly

### Low Priority

8. **Performance Optimization**: Once relationships are found, optimize retrieval performance
9. **Error Handling**: Improve error messages when relationships are not found

## Test Execution Log

### 2025-01-20 - Initial Investigation

- Ran relationship count queries - all returned 0
- Verified catalog shows 40 relationships exist
- Tested CREATE relationship - returns success but not found by MATCH
- Verified nodes are found correctly (Alice has node_id 285)
- Created manual relationship - still not found

### 2025-01-20 - Aggregate Fixes

- Fixed Aggregate `count(r)` to use `effective_row_count`
- Fixed Aggregate GROUP BY when Project deferred
- Tested queries - still failing (relationships not found)

### 2025-01-20 - Expand Investigation

- Analyzed `find_relationships` code flow
- Verified multiple retrieval methods exist
- All methods fail to find relationships
- Created comprehensive test suite

### 2025-01-20 - Linked List Analysis

- **Code Review**: Verified linked list construction logic
  - `first_rel_ptr` stored as `rel_id + 1` in nodes
  - `next_src_ptr` stored as previous `rel_id + 1` in relationships
  - Traversal uses `rel_id = rel_ptr - 1` to read relationship records
  - Structure appears correct in code

- **Test Results**: 
  - Single relationship: WORKS (count returns 1)
  - Multiple relationships: FAILS (count returns 1 instead of 2)
  - First relationship is found, second is not

- **Debug Logging Enhanced**:
  - Added logging for `next_src_ptr`/`next_dst_ptr` values during relationship creation
  - Added logging for linked list traversal (moving from one relationship to next)
  - Added logging when reaching end of linked list (rel_ptr=0)

- **Next Steps**: 
  - Analyze debug logs to see actual pointer values
  - Verify if traversal is following `next_src_ptr` correctly
  - Check if there's a condition stopping traversal prematurely

### 2025-01-20 - Code Deep Analysis

- **Memory-Mapped File Writes**:
  - Writes use `mmap.copy_from_slice()` which should be immediately visible
  - No explicit memory barriers or synchronization found
  - `flush_async()` is called after transaction commit but doesn't wait for OS

- **Potential Issues Identified**:
  1. **Write Ordering**: When multiple relationships are created in the same transaction:
     - First relationship: writes node (first_rel_ptr=1), then relationship (next_src_ptr=0)
     - Second relationship: reads node (should see first_rel_ptr=1), writes node (first_rel_ptr=2), then relationship (next_src_ptr=1)
     - If node read happens before first relationship's node write is visible, source_prev_ptr would be 0 instead of 1
  
  2. **Transaction Isolation**: All relationships created in same transaction should be visible to each other, but mmap writes may need explicit synchronization

- **Hypothesis**: The node read for the second relationship may not see the updated `first_rel_ptr` from the first relationship's write, causing `source_prev_ptr` to be 0 instead of 1, which breaks the linked list chain.

- **Solution to Test**: Add explicit memory synchronization (e.g., `std::sync::atomic::fence`) or ensure reads happen after all writes in the transaction are complete.

## Files Modified

1. `nexus-core/src/executor/mod.rs` - Aggregate fixes, deduplication fixes, debug logging
2. `scripts/test-neo4j-nexus-compatibility-200.ps1` - Database cleanup improvements
3. `rulebook/tasks/fix-neo4j-compatibility-errors/proposal.md` - Status updates
4. `rulebook/tasks/fix-neo4j-compatibility-errors/tasks.md` - Task tracking

## Commits Made

1. `Fix: Aggregate count(r) when column not in result_set - use effective_row_count for relationship aggregations`
2. `Fix: Aggregate GROUP BY when Project deferred - materialize rows from variables and evaluate projection expressions for GROUP BY columns`

## Conclusion

The issue is not with the query execution logic (Expand, Aggregate, Project operators) but with the **relationship storage and retrieval mechanism**. Relationships are being created and tracked in the catalog, but the `find_relationships` function cannot locate them through any of its methods.

**Critical Finding (2025-01-20)**:
- Single relationship for a node: **WORKS** (count returns 1)
- Multiple relationships for the same node: **FAILS** (only first relationship found, count returns 1 instead of 2)
- This indicates the linked list traversal is working for the first relationship but failing to traverse to subsequent relationships

**Root Cause Hypothesis**:
The linked list structure appears correct in code (`first_rel_ptr` updated, `next_src_ptr` set), but when reading relationships, only the first one is found. Possible causes:
1. Linked list pointers not being persisted correctly after transaction commit
2. Linked list traversal breaking after first relationship (missing or incorrect `next_src_ptr`)
3. Transaction isolation issue - second relationship not visible to read transactions

**Debug Logging Added**:
- `[create_relationship]` logs for `first_rel_ptr` updates and `next_src_ptr`/`next_dst_ptr` values
- `[find_relationships]` logs for node reading, linked list traversal, and pointer following
- Logs should reveal if `first_rel_ptr` is being updated correctly and if linked list traversal is following pointers correctly

**Latest Investigation (2025-01-20 - Continued)**:
- Added additional debug logging to track `next_src_ptr` and `next_dst_ptr` values when creating relationships
- Added logging to track linked list traversal - when moving from one relationship to the next
- Testing with multiple relationships to see if linked list pointers are being followed correctly

**Code Analysis**:
- `RelationshipRecord::new()` initializes `next_src_ptr` and `next_dst_ptr` to `u64::MAX`
- These are then set to `source_prev_ptr` and `target_prev_ptr` (which should be the previous relationship's `rel_id + 1`)
- When first relationship is created: `source_prev_ptr = 0` (no previous), so `next_src_ptr = 0` (end of list) ✓
- When second relationship is created: `source_prev_ptr = first_rel_id + 1`, so `next_src_ptr = first_rel_id + 1` (should point to first) ✓
- Linked list structure appears correct in code

**Hypothesis**:
The linked list structure is being built correctly, but traversal may be stopping prematurely or not following `next_src_ptr` correctly when it's not 0.

### 2025-01-20 - Debug Log Analysis

- **Test Executed**: `test-linked-list-direct.ps1` - Direct linked list verification
- **Results**:
  - First relationship created: Count = 1 ✓ (expected: 1)
  - Second relationship created: Count = 1 ✗ (expected: 2)
  - **Confirmed**: Only first relationship is found, second is not

- **Debug Logs Status**:
  - Log file was empty (0 bytes) - logs may not be configured or written yet
  - Test confirmed the issue without logs: Count = 1 after 2 relationships created
  - **Critical Observation**: Only the first relationship is found

- **Critical Hypothesis to Verify**:
  When the second relationship is created, it reads the node to get `first_rel_ptr`. The question is:
  - **Does it read `first_rel_ptr=0` (old value) instead of `first_rel_ptr=1` (new value from first relationship)?**
  - If yes, this would cause `next_src_ptr` to be set to 0 instead of 1, breaking the linked list chain
  - This could happen if memory-mapped file writes are not immediately visible to subsequent reads in the same transaction

- **Code Flow Analysis**:
  When multiple relationships are created in the same transaction:
    1. **First relationship creation** (rel_id=0):
       - Reads node: `first_rel_ptr = 0` (no previous relationships)
       - Writes node: `first_rel_ptr = 1` (points to rel_id=0)
       - Writes relationship record: `next_src_ptr = 0` (end of list)
    
    2. **Second relationship creation** (rel_id=1):
       - Reads node: `first_rel_ptr = ?` (should be 1, but might be 0 if write not visible)
       - Writes node: `first_rel_ptr = 2` (points to rel_id=1)
       - Writes relationship record: `next_src_ptr = ?` (should be 1, but might be 0)
  
  **Critical Code Locations**:
  - `storage/mod.rs:644-645`: Node read happens HERE
  - `storage/mod.rs:656`: Node write with updated `first_rel_ptr`
  - `storage/mod.rs:690`: `write_node()` called (memory-mapped file write)
  - `storage/mod.rs:701`: `next_src_ptr` set to `source_prev_ptr` (which was read at line 645)
  
  **Potential Issue**: If `read_node()` at line 644 happens before `write_node()` at line 690 is visible (from first relationship), `source_prev_ptr` would be 0 instead of 1, breaking the linked list.

- **Memory-Mapped File Behavior**:
  - `write_node()` uses `copy_from_slice()` which should be immediately visible
  - `read_node()` reads directly from mmap which should see latest writes
  - However, there's no explicit memory barrier or synchronization
  - In same-thread/same-process, this usually works, but not guaranteed by spec

-   **Fix Applied**:
  Added memory barriers (`std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst)`) after `write_node()` and `write_rel()` to ensure writes are visible before subsequent reads. This guarantees that when the second relationship reads the node, it will see the updated `first_rel_ptr` from the first relationship.

  **Implementation**:
  - `storage/mod.rs:write_node()`: Added `std::sync::atomic::fence()` after `copy_from_slice()`
  - `storage/mod.rs:write_rel()`: Added `std::sync::atomic::fence()` after `copy_from_slice()`
  
  This ensures memory visibility across relationship creation operations in the same transaction.

### 2025-01-20 - Memory Barrier Fix Test Results

- **Fix Applied**: Memory barriers added after `write_node()` and `write_rel()`
- **Test Executed**: `test-linked-list-direct.ps1` and `test-multiple-relationships.ps1`
- **Results**:
  - First relationship created: Count = 1 ✓ (expected: 1)
  - Second relationship created: Count = 1 ✗ (expected: 2)
  - **Still failing**: Memory barriers did not resolve the issue

- **Analysis**:
  - Relationships are created in separate transactions (each query commits)
  - Each query has its own transaction that commits after CREATE
  - Memory barriers ensure visibility within same transaction, but transactions are separate
  - **Hypothesis**: The problem may not be visibility, but rather how `next_src_ptr` is being set

- **Code Flow Analysis**:
  When second relationship is created:
  1. Read node: `first_rel_ptr` = ? (should be 1 from first relationship, but might be 0)
  2. Set `source_prev_ptr` = `first_rel_ptr` (line 657)
  3. Update node: `first_rel_ptr` = 2 (line 668)
  4. Write node with updated `first_rel_ptr` (line 702)
  5. Set relationship: `next_src_ptr` = `source_prev_ptr` (line 713)
  6. Write relationship with `next_src_ptr` (line 730)

  **Critical Question**: When line 657 reads `first_rel_ptr`, does it get the value 1 (from first relationship) or 0 (old value)?

The next investigation should focus on:
1. Analyzing debug logs to see actual `next_src_ptr` values when creating relationships
2. Verifying if traversal is following `next_src_ptr` when it's not 0
3. Checking if there's a condition that stops traversal prematurely
4. Verifying if `next_src_ptr` values are being persisted correctly to disk

