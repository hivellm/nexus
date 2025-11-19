# Tasks - Review Functions with Negative Index and Related Issues

**Status**: 🔴 IN PROGRESS  
**Priority**: HIGH  
**Created**: 2025-11-18  
**Target**: Fix all functions with negative index handling issues and related bugs

---

## Overview

This task tracks all functions and features that need review and fixes related to:
1. Negative index handling (similar to substring issue)
2. Test failures marked with `#[ignore]`
3. Known bugs documented in `docs/bugs/`
4. Functions that accept length/count parameters that might need negative index support

---

## Phase 1: String Functions - Negative Index Review

### 1.1 substring() - Fix Negative Index Calculation

**Status**: 🔴 BLOCKED  
**Location**: `nexus-core/src/executor/mod.rs:6429-6468`  
**Tests**: `nexus-core/tests/test_substring_negative.rs:34, 51`

**Problem**:
- Negative indices not working correctly in substring()
- Test `test_substring_negative_index` is ignored
- Test `test_substring_negative_index_no_length` is ignored
- Current implementation handles negative start but may have edge cases

**Tasks**:
- [ ] 1.1.1 Review current negative index implementation in substring()
  - [ ] Verify Neo4j compatibility for negative indices
  - [ ] Check edge cases (very large negative values, boundary conditions)
  - [ ] Document expected behavior vs actual behavior
- [ ] 1.1.2 Fix negative index calculation
  - [ ] Fix start position calculation with negative index
  - [ ] Fix length parameter when start is negative
  - [ ] Handle case when negative index + length exceeds string bounds
- [ ] 1.1.3 Enable and fix ignored tests
  - [ ] Remove `#[ignore]` from `test_substring_negative_index`
  - [ ] Remove `#[ignore]` from `test_substring_negative_index_no_length`
  - [ ] Verify all substring tests pass
- [ ] 1.1.4 Add comprehensive tests
  - [ ] Test negative index with length parameter
  - [ ] Test negative index without length parameter
  - [ ] Test edge cases (boundary values, empty strings)
  - [ ] Verify compatibility with Neo4j behavior

**Related Code**:
```rust
// Current implementation (lines 6443-6465)
// Handle negative indices (count from end)
let start = if start_i64 < 0 {
    ((char_len + start_i64).max(0)) as usize
} else {
    start_i64.min(char_len) as usize
};
```

---

## Phase 2: Transaction and Rollback Issues

### 2.1 Fix Rollback - Node Removal from Index/Storage

**Status**: 🔴 BLOCKED  
**Location**: Transaction rollback handling  
**Tests**: 
- `nexus-core/tests/transaction_session_test.rs:61, 192, 223`
- `nexus-core/tests/test_index_consistency.rs:160`

**Problem**:
- Rollback not removing nodes from index/storage correctly
- 3 tests ignored due to this issue
- Index consistency not maintained after rollback

**Tasks**:
- [ ] 2.1.1 Analyze rollback implementation
  - [ ] Review transaction rollback logic
  - [ ] Identify where nodes should be removed from index
  - [ ] Identify where nodes should be removed from storage
  - [ ] Document current behavior vs expected behavior
- [ ] 2.1.2 Fix node removal on rollback
  - [ ] Remove nodes from label index on rollback
  - [ ] Remove nodes from property indexes on rollback
  - [ ] Remove nodes from relationship indexes on rollback
  - [ ] Remove nodes from storage on rollback
- [ ] 2.1.3 Fix relationship removal on rollback
  - [ ] Remove relationships from indexes on rollback
  - [ ] Remove relationships from storage on rollback
  - [ ] Maintain referential integrity
- [ ] 2.1.4 Enable and fix ignored tests
  - [ ] Remove `#[ignore]` from `test_transaction_rollback_persists_across_queries`
  - [ ] Remove `#[ignore]` from other rollback tests in `transaction_session_test.rs`
  - [ ] Remove `#[ignore]` from `test_index_consistency_after_rollback`
  - [ ] Verify all rollback tests pass
- [ ] 2.1.5 Add comprehensive rollback tests
  - [ ] Test rollback with multiple nodes
  - [ ] Test rollback with relationships
  - [ ] Test rollback with indexes
  - [ ] Test rollback with properties
  - [ ] Verify index consistency after rollback

---

## Phase 3: Query Execution Issues

### 3.1 Fix DELETE with RETURN count(*)

**Status**: 🔴 BLOCKED  
**Location**: DELETE query execution with aggregation  
**Tests**: `nexus-core/tests/test_regression_fixes.rs:222`

**Problem**:
- DELETE with RETURN count(*) returns 0 instead of actual deleted count
- Test `regression_delete_with_return_count` is ignored

**Tasks**:
- [ ] 3.1.1 Analyze DELETE execution flow
  - [ ] Review DELETE query execution logic
  - [ ] Identify where count should be calculated
  - [ ] Document current behavior vs expected behavior
- [ ] 3.1.2 Fix count calculation in DELETE
  - [ ] Track deleted nodes count during DELETE execution
  - [ ] Track deleted relationships count during DELETE execution
  - [ ] Return correct count in RETURN clause
- [ ] 3.1.3 Enable and fix ignored test
  - [ ] Remove `#[ignore]` from `regression_delete_with_return_count`
  - [ ] Verify DELETE count tests pass
- [ ] 3.1.4 Add comprehensive DELETE count tests
  - [ ] Test DELETE nodes with count
  - [ ] Test DELETE relationships with count
  - [ ] Test DELETE with WHERE clause and count
  - [ ] Test DELETE with multiple deletions and count

---

### 3.2 Fix Directed Relationship Matching with Labels

**Status**: 🔴 BLOCKED  
**Location**: Relationship pattern matching  
**Tests**: `nexus-core/tests/test_relationship_counting.rs:183`

**Problem**:
- Directed relationship matching with labels returns count of 0 when should be 1
- Test `test_relationship_direction_with_labels` is ignored

**Tasks**:
- [ ] 3.2.1 Analyze relationship pattern matching
  - [ ] Review relationship direction matching logic
  - [ ] Review label filtering in relationship patterns
  - [ ] Document current behavior vs expected behavior
- [ ] 3.2.2 Fix relationship direction matching
  - [ ] Fix directed relationship matching (`->`)
  - [ ] Fix bidirectional relationship matching (`-`)
  - [ ] Ensure label filtering works with direction
- [ ] 3.2.3 Enable and fix ignored test
  - [ ] Remove `#[ignore]` from `test_relationship_direction_with_labels`
  - [ ] Verify relationship direction tests pass
- [ ] 3.2.4 Add comprehensive relationship direction tests
  - [ ] Test single directed relationship
  - [ ] Test bidirectional relationship
  - [ ] Test with labels
  - [ ] Test with properties
  - [ ] Test with multiple relationship types

---

### 3.3 Fix Multiple Relationship Types with RETURN Clause

**Status**: 🔴 BLOCKED  
**Location**: Multiple relationship type matching  
**Tests**: `nexus-core/tests/test_multiple_relationship_types.rs:112`

**Problem**:
- Multiple relationship types with RETURN clause not working correctly
- Test `test_multiple_relationship_types_with_return` is ignored

**Tasks**:
- [ ] 3.3.1 Analyze multiple relationship type matching
  - [ ] Review relationship type pattern matching
  - [ ] Review RETURN clause processing with multiple types
  - [ ] Document current behavior vs expected behavior
- [ ] 3.3.2 Fix multiple relationship type handling
  - [ ] Fix pattern matching with multiple types: `[:KNOWS|LIKES]`
  - [ ] Ensure RETURN clause works with multiple types
  - [ ] Handle variable binding correctly
- [ ] 3.3.3 Enable and fix ignored test
  - [ ] Remove `#[ignore]` from `test_multiple_relationship_types_with_return`
  - [ ] Verify multiple relationship type tests pass
- [ ] 3.3.4 Add comprehensive multiple relationship type tests
  - [ ] Test with 2 relationship types
  - [ ] Test with 3+ relationship types
  - [ ] Test with RETURN clause
  - [ ] Test with WHERE clause
  - [ ] Test with properties

---

## Phase 4: Array and String Function Review

### 4.1 Review Array Slicing Negative Index

**Status**: 🟡 REVIEW NEEDED  
**Location**: `nexus-core/src/executor/mod.rs:6214-6280, 4382-4445`

**Status Note**: Implementation exists but needs review for Neo4j compatibility

**Tasks**:
- [ ] 4.1.1 Review array slicing negative index implementation
  - [ ] Compare with Neo4j behavior
  - [ ] Verify start index negative handling
  - [ ] Verify end index negative handling
  - [ ] Check edge cases (boundary conditions, empty arrays)
- [ ] 4.1.2 Test array slicing with negative indices
  - [ ] Test `array[1..-1]` (should exclude last element)
  - [ ] Test `array[-2..]` (should start from second-to-last)
  - [ ] Test `array[..-1]` (should exclude last element)
  - [ ] Test with empty arrays
  - [ ] Test with single element arrays
- [ ] 4.1.3 Document array slicing behavior
  - [ ] Document negative index semantics
  - [ ] Document edge case handling
  - [ ] Add examples to documentation

---

### 4.2 Review Other String Functions for Negative Index Support

**Status**: 🟡 REVIEW NEEDED  
**Location**: `nexus-core/src/executor/mod.rs:6470-6537`

**Functions to Review**:
- `trim()`, `ltrim()`, `rtrim()` - May not need negative indices
- `replace()` - May not need negative indices
- `split()` - May not need negative indices

**Tasks**:
- [ ] 4.2.1 Check if other string functions need negative index support
  - [ ] Review Neo4j documentation for `left()` function
  - [ ] Review Neo4j documentation for `right()` function
  - [ ] Verify if `left()` and `right()` are implemented
  - [ ] Check if `left()` and `right()` need negative index support
- [ ] 4.2.2 Implement `left()` and `right()` if missing
  - [ ] Add parser support for `left()` function
  - [ ] Add parser support for `right()` function
  - [ ] Implement execution logic with negative index support
  - [ ] Add tests
- [ ] 4.2.3 Document string function behavior
  - [ ] Document which functions support negative indices
  - [ ] Document behavior differences from Neo4j if any
  - [ ] Add examples to documentation

---

## Phase 5: Array Indexing Review

### 5.1 Review Array Indexing Negative Index

**Status**: 🟢 REVIEWED (Implementation exists)  
**Location**: `nexus-core/src/executor/mod.rs:6181-6212`

**Status Note**: Implementation exists and appears correct, but needs verification

**Tasks**:
- [ ] 5.1.1 Verify array indexing negative index implementation
  - [ ] Compare with Neo4j behavior
  - [ ] Test `array[-1]` (should return last element)
  - [ ] Test `array[-2]` (should return second-to-last element)
  - [ ] Test out of bounds with negative indices
- [ ] 5.1.2 Add edge case tests
  - [ ] Test with empty arrays
  - [ ] Test with single element arrays
  - [ ] Test with very large negative indices
  - [ ] Test boundary conditions

---

## Phase 6: Code Review and Documentation

### 6.1 Code Review Checklist

**Tasks**:
- [ ] 6.1.1 Review all functions that accept numeric parameters
  - [ ] Identify all functions with start/length/count parameters
  - [ ] Verify which ones should support negative indices
  - [ ] Document decision for each function
- [ ] 6.1.2 Create negative index handling utility
  - [ ] Consider creating helper function for negative index calculation
  - [ ] Ensure consistent behavior across all functions
  - [ ] Add unit tests for utility function
- [ ] 6.1.3 Update documentation
  - [ ] Document negative index support in each function
  - [ ] Add examples showing negative index usage
  - [ ] Update CHANGELOG.md with fixes
  - [ ] Update API documentation

---

## Phase 7: Testing and Validation

### 7.1 Comprehensive Testing

**Tasks**:
- [ ] 7.1.1 Run all ignored tests
  - [ ] Remove `#[ignore]` attributes one by one
  - [ ] Fix issues as they are discovered
  - [ ] Verify all tests pass
- [ ] 7.1.2 Neo4j compatibility tests
  - [ ] Compare behavior with Neo4j for each function
  - [ ] Document any differences
  - [ ] Create compatibility test suite
- [ ] 7.1.3 Performance tests
  - [ ] Verify no performance regression
  - [ ] Test with large datasets
  - [ ] Profile negative index calculations
- [ ] 7.1.4 Edge case testing
  - [ ] Test with empty strings/arrays
  - [ ] Test with single element strings/arrays
  - [ ] Test with very large negative indices
  - [ ] Test boundary conditions

---

## Success Criteria

### Phase 1: String Functions
- [x] All substring() negative index tests passing
- [ ] Documentation updated
- [ ] Neo4j compatibility verified

### Phase 2: Transaction and Rollback
- [ ] All rollback tests passing
- [ ] Index consistency maintained after rollback
- [ ] Storage consistency maintained after rollback

### Phase 3: Query Execution
- [ ] DELETE with count(*) working correctly
- [ ] Relationship direction matching working correctly
- [ ] Multiple relationship types with RETURN working correctly

### Phase 4-5: Array and String Functions
- [ ] All functions reviewed for negative index support
- [ ] Missing functions implemented
- [ ] Documentation updated

### Phase 6-7: Code Review and Testing
- [ ] All ignored tests enabled and passing
- [ ] Code review completed
- [ ] Documentation updated
- [ ] Compatibility tests passing

---

## Files to Modify

### Core Files
- `nexus-core/src/executor/mod.rs` - Main executor logic
- `nexus-core/src/executor/parser.rs` - Parser (if new functions needed)
- `nexus-core/src/executor/planner.rs` - Planner (if needed)

### Test Files
- `nexus-core/tests/test_substring_negative.rs` - Enable ignored tests
- `nexus-core/tests/transaction_session_test.rs` - Enable ignored tests
- `nexus-core/tests/test_index_consistency.rs` - Enable ignored test
- `nexus-core/tests/test_regression_fixes.rs` - Enable ignored test
- `nexus-core/tests/test_relationship_counting.rs` - Enable ignored test
- `nexus-core/tests/test_multiple_relationship_types.rs` - Enable ignored test

### Documentation
- `CHANGELOG.md` - Update with fixes
- `docs/` - Update API documentation
- `docs/bugs/` - Mark bugs as fixed

---

## Notes

- **Priority Order**: Phase 1 (substring) → Phase 3 (query issues) → Phase 2 (rollback) → Phase 4-5 (review)
- **Test-Driven**: Always enable tests first, then fix issues
- **Neo4j Compatibility**: Always verify behavior matches Neo4j
- **Documentation**: Update documentation as fixes are implemented

---

## Related Issues

- substring() negative index: Similar to array slicing, but for strings
- Rollback issues: Related to transaction handling
- DELETE count: Related to aggregation in write operations
- Relationship matching: Related to pattern matching and query execution

---

## References

- Neo4j Cypher Manual: String Functions
- Neo4j Cypher Manual: Array Functions
- Neo4j Cypher Manual: Transaction Handling
- `docs/bugs/ARRAY-INDEXING-IMPLEMENTATION.md`
- `docs/bugs/CREATE-DUPLICATION-BUG.md`
- `docs/bugs/WHERE-IN-OPERATOR-BUG.md`
- `docs/bugs/ORDER-BY-IMPLEMENTATION.md`

