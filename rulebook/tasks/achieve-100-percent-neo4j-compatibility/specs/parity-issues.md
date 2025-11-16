# Parity Issues - Neo4j vs Nexus Deep Analysis

**Status**: Analysis Complete  
**Date**: 2025-11-16  
**Test Suite**: `scripts/test-neo4j-nexus-parity-issues.ps1`

## Executive Summary

Deep comparison testing revealed 26 specific parity issues across 5 categories:
- **Overall Pass Rate**: 42.31% (11/26 tests)
- **Critical Issues**: 7 tests (CREATE with RETURN)
- **High Priority**: 4 tests (String Concatenation)
- **Medium Priority**: 10 tests (Array Operations)
- **Low Priority**: 4 tests (Multiple Relationship Types)

## Test Results by Category

### 🔴 CRITICAL: CREATE with RETURN (0/7 passing)

**Status**: All 7 tests fail identically  
**Impact**: CRITICAL - Basic CREATE functionality broken with RETURN

**Symptom**: 
- Neo4j: Returns 1 row with created node data
- Nexus: Returns 0 rows (but node IS created in database)

**Root Cause**: RecordStore cloning issue - RETURN clause not processing created nodes

**Test Cases**:
- CREATE.01: Single node with property return
- CREATE.02: Create and return literal
- CREATE.03: Multiple properties return
- CREATE.04: Multiple labels return
- CREATE.05: Return node object
- CREATE.06: Return id() function
- CREATE.07: Multiple creates with RETURN

**Example**:
```cypher
Query: CREATE (n:Person {name: 'Alice', age: 30}) RETURN n.name AS name
Neo4j: 1 row with name='Alice'
Nexus: 0 rows (node created but not returned)
```

**Fix Location**: `nexus-core/src/executor/mod.rs` - execute_create functions

---

### 🟡 HIGH: String Concatenation (1/5 passing - 20%)

**Status**: 4 of 5 tests fail  
**Impact**: HIGH - Very common in real queries

**Error**: "Type mismatch: expected number, got string"

**Root Cause**: BinaryOp::Add only supports numeric addition

**Test Results**:
- ❌ STRING.01: Basic concatenation `'Hello' + ' ' + 'World'`
- ❌ STRING.02: Concatenation with property
- ❌ STRING.03: Multiple concatenations
- ✅ STRING.04: Concatenation with NULL (correct!)
- ❌ STRING.05: Concatenation in WHERE

**Example**:
```cypher
Query: RETURN 'Hello' + ' ' + 'World' AS text
Neo4j: 1 row with text='Hello World'
Nexus: Error "Type mismatch: expected number, got string"
```

**Fix Location**: `nexus-core/src/executor/mod.rs` - evaluate_expression (BinaryOp::Add)

---

### 🟡 MEDIUM: Array Slicing (0/5 passing)

**Status**: All 5 tests fail  
**Impact**: MEDIUM - Advanced feature but Neo4j compatible

**Error**: "Parse error: Expected ']'"

**Root Cause**: Range syntax `[start..end]` not implemented in parser

**Test Cases**:
- ❌ ARRAY.01: Basic slicing `[1..3]`
- ❌ ARRAY.02: Slicing from start `[..3]`
- ❌ ARRAY.03: Slicing to end `[2..]`
- ❌ ARRAY.04: Negative index slicing `[-3..-1]`
- ❌ ARRAY.05: Slicing with property `n.tags[0..1]`

**Example**:
```cypher
Query: RETURN [1, 2, 3, 4, 5][1..3] AS slice
Neo4j: 1 row with slice=[2, 3, 4]
Nexus: Parse error at ".."
```

**Fix Location**: `nexus-core/src/executor/parser.rs` - Add ArraySlice expression variant

---

### 🟡 MEDIUM: Array Concatenation (0/5 passing)

**Status**: All 5 tests fail  
**Impact**: MEDIUM - Common array manipulation

**Error**: "Type mismatch: expected number, got unknown type"

**Root Cause**: Operator `+` doesn't support array types

**Test Cases**:
- ❌ CONCAT.01: Basic array concat `[1,2] + [3,4]`
- ❌ CONCAT.02: String array concat
- ❌ CONCAT.03: Multiple concatenations
- ❌ CONCAT.04: Empty array concat
- ❌ CONCAT.05: Mixed type concat

**Fix Location**: `nexus-core/src/executor/mod.rs` - evaluate_expression (BinaryOp::Add)

---

### 🟢 LOW: Multiple Relationship Types (0/4 passing)

**Status**: All 4 tests fail  
**Impact**: LOW - Workaround available

**Error**: "Parse error: Expected ']' at column 20"

**Root Cause**: Pipe operator `|` not implemented in relationship type parser

**Workaround**: Use `WHERE type(r) IN ['KNOWS', 'WORKS_AT']`

**Test Cases**:
- ❌ RELTYPE.01: Two types `[r:KNOWS|WORKS_WITH]`
- ❌ RELTYPE.02: Three types `[r:KNOWS|WORKS_WITH|MANAGES]`
- ❌ RELTYPE.03: Return type with multiple
- ❌ RELTYPE.04: Bidirectional with multiple types

**Fix Location**: `nexus-core/src/executor/parser.rs` - parse_relationship

---

## Fix Strategies

### 1. CREATE with RETURN (URGENT)
- **Strategy**: Ensure result_set populated before RETURN processing
- **Approach**: Fix RecordStore cloning issue in executor
- **Estimated Complexity**: Medium - architectural issue

### 2. String Concatenation (HIGH)
- **Strategy**: Add string type check in BinaryOp::Add
- **Approach**: Detect string operands and concatenate instead of add
- **Estimated Complexity**: Low - straightforward type handling

### 3. Array Operations (MEDIUM)
- **Strategy**: Add ArraySlice expression variant + range parsing
- **Approach**: Parse `[start..end]` syntax and implement slice evaluation
- **Estimated Complexity**: Medium - parser + executor changes

### 4. Multiple Relationship Types (LOW)
- **Strategy**: Parse multiple types separated by `|`
- **Approach**: Modify relationship parser to accept type list
- **Estimated Complexity**: Low - parser change only

## Test Coverage

**Test Suite**: `scripts/test-neo4j-nexus-parity-issues.ps1`
- 26 targeted tests
- Deep comparison (types, values, errors)
- Detailed diagnostics
- Priority action items

**Comparison Metrics**:
- Row count matching
- Data type validation
- Value equality checking
- Error message comparison
- NULL handling verification

## Related Issues

- Session 6: RecordStore cloning issue
- Phase 1-9: Various compatibility tasks
- PowerShell Suite: 185/195 tests passing (94.87%)

