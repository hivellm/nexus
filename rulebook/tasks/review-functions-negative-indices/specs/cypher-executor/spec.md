# Cypher Executor Specification - Negative Index and Function Fixes

## Purpose

This specification defines the requirements for fixing negative index handling in string and array functions, ensuring full Neo4j compatibility. The current implementation has incorrect behavior for negative indices in substring() and needs review for other string and array manipulation functions.

## MODIFIED Requirements

### Requirement: substring() Function with Negative Indices

The system SHALL correctly handle negative indices in the substring() function, calculating start position and length parameters according to Neo4j semantics where negative values count from the end of the string.

#### Scenario: Negative Start Index

Given a string "Hello World" with length 11
When substring() is called with start index -5
Then the system SHALL calculate start position as 11 + (-5) = 6
And the result SHALL be "World" (characters from index 6 to end)
And the behavior SHALL match Neo4j substring() function exactly

#### Scenario: Negative Start Index with Length

Given a string "Hello World" with length 11
When substring() is called with start index -5 and length 3
Then the system SHALL calculate start position as 11 + (-5) = 6
And the system SHALL extract 3 characters starting from position 6
And the result SHALL be "Wor"
And the behavior SHALL match Neo4j substring() function exactly

#### Scenario: Negative Start Index Exceeding Bounds

Given a string "Hello" with length 5
When substring() is called with start index -10 (very large negative)
Then the system SHALL clamp the start position to 0
And the result SHALL start from the beginning of the string
And no out-of-bounds error SHALL occur

#### Scenario: Negative Start Index with Length Exceeding Bounds

Given a string "Hello" with length 5
When substring() is called with start index -3 and length 10
Then the system SHALL calculate start position as 5 + (-3) = 2
And the system SHALL extract characters from position 2 to end (length 3)
And the result SHALL be "llo"
And no out-of-bounds error SHALL occur

#### Scenario: Empty String with Negative Index

Given an empty string ""
When substring() is called with any negative start index
Then the system SHALL return an empty string
And no error SHALL occur

### Requirement: Array Slicing with Negative Indices

The system SHALL correctly handle negative indices in array slicing operations, ensuring compatibility with Neo4j array slicing behavior.

#### Scenario: Array Slice with Negative Start

Given an array [1, 2, 3, 4, 5]
When array slicing is performed with start index -2
Then the system SHALL calculate start position as 5 + (-2) = 3
And the result SHALL be [4, 5]
And the behavior SHALL match Neo4j array slicing exactly

#### Scenario: Array Slice with Negative End

Given an array [1, 2, 3, 4, 5]
When array slicing is performed with end index -1
Then the system SHALL calculate end position as 5 + (-1) = 4
And the result SHALL be [1, 2, 3, 4] (excluding last element)
And the behavior SHALL match Neo4j array slicing exactly

#### Scenario: Array Slice with Both Negative Indices

Given an array [1, 2, 3, 4, 5]
When array slicing is performed with start -3 and end -1
Then the system SHALL calculate start as 5 + (-3) = 2 and end as 5 + (-1) = 4
And the result SHALL be [3, 4]
And the behavior SHALL match Neo4j array slicing exactly

### Requirement: Array Indexing with Negative Indices

The system SHALL correctly handle negative indices in array element access, where -1 refers to the last element.

#### Scenario: Negative Array Index Access

Given an array [10, 20, 30, 40, 50]
When accessing element at index -1
Then the system SHALL return 50 (last element)
And the behavior SHALL match Neo4j array indexing exactly

#### Scenario: Negative Array Index Out of Bounds

Given an array [10, 20, 30] with length 3
When accessing element at index -5 (very large negative)
Then the system SHALL return NULL or raise appropriate error
And the behavior SHALL match Neo4j array indexing exactly

## ADDED Requirements

### Requirement: Comprehensive Negative Index Testing

The system SHALL have comprehensive test coverage for all negative index scenarios across string and array functions.

#### Scenario: Test Coverage for substring() Negative Indices

Given the substring() function implementation
When test suite is executed
Then all negative index test cases SHALL pass
And test coverage SHALL be at least 95% for negative index handling
And all previously ignored tests SHALL be enabled and passing

#### Scenario: Neo4j Compatibility Verification

Given string and array functions with negative index support
When compatibility tests are executed against Neo4j
Then all function results SHALL match Neo4j results exactly
And edge cases SHALL be handled identically to Neo4j
And documentation SHALL be updated with negative index behavior

## Implementation Notes

### Negative Index Calculation Formula

For a string or array with length `len` and negative index `-n`:
- Calculated index = `len + (-n)`
- Clamp to valid range: `max(0, min(len, calculated_index))`

### Edge Cases to Handle

1. Very large negative values (should clamp to 0)
2. Empty strings/arrays (should return empty result)
3. Negative start + length exceeding bounds (should clamp to end)
4. Single element strings/arrays with negative indices

### Testing Requirements

- Unit tests for each negative index scenario
- Integration tests comparing with Neo4j behavior
- Edge case tests for boundary conditions
- Performance tests to ensure no regression

