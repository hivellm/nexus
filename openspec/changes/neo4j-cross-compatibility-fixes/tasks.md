# Implementation Tasks: Neo4j Cross-Compatibility Fixes

**Date**: 2025-10-31  
**Owner**: Unassigned  
**Priority**: HIGH  
**Status**: 🔴 Not Started  

---

## Objective
Fix 8 failing tests in Neo4j cross-compatibility validation script.

**Current**: 9/17 tests passing (52.94%)  
**Target**: 16+/17 tests passing (>90%)  
**Estimated Time**: 9-12 days

---

## Phase 1: Investigation & Setup ✅ COMPLETE

### 1.1 Run Cross-Compatibility Test - ✅ DONE
- [x] Execute `tests/cross-compatibility/test-compatibility.ps1`
- [x] Document all failing tests
- [x] Identify root causes
- **Result**: 8 failures identified, analysis complete

### 1.2 Create OpenSpec Documentation - ✅ DONE
- [x] Create proposal.md
- [x] Create analysis.md
- [x] Create tasks.md (this file)

---

## Phase 2: High Priority Fixes (Target: 82% compatibility)

### 2.1 Fix Relationship Query Support - 🔴 NOT STARTED
**Impact**: +11.76% (2 tests)  
**Estimated Time**: 1-2 days

#### Tasks:
- [ ] Investigate relationship pattern matching in executor
- [ ] Debug `MATCH ()-[r:KNOWS]->()` returning empty results
- [ ] Fix relationship property access in projections
- [ ] Test with `MATCH (a)-[r:KNOWS]->(b) RETURN a.name, b.name, r.since`
- [ ] Add unit tests for relationship queries
- [ ] Update cross-compatibility script to re-test

**Files to Modify**:
- `nexus-core/src/executor/mod.rs` (relationship traversal)
- `nexus-core/src/executor/planner.rs` (Expand operator)

**Acceptance Criteria**:
- ✅ `count relationships` test passes
- ✅ `relationship properties` test passes
- ✅ Unit tests added and passing

---

### 2.2 Implement Aggregation Functions - 🔴 NOT STARTED
**Impact**: +11.76% (2 tests)  
**Estimated Time**: 2-3 days

#### Tasks:
- [ ] Verify current aggregation implementation in executor
- [ ] Implement `avg()` aggregation function
- [ ] Implement `min()` aggregation function
- [ ] Implement `max()` aggregation function
- [ ] Fix `sum()` if broken (should already exist)
- [ ] Handle NULL values correctly in aggregations
- [ ] Test with multiple aggregations in single query
- [ ] Add unit tests for each aggregation function
- [ ] Update cross-compatibility script to re-test

**Files to Modify**:
- `nexus-core/src/executor/mod.rs` (lines 1200-1400, aggregation logic)
- `nexus-core/src/executor/planner.rs` (aggregation operator)

**Test Queries**:
```cypher
MATCH (n:Person) RETURN avg(n.age) AS avg_age
MATCH (n:Person) RETURN min(n.age) AS min_age, max(n.age) AS max_age
```

**Acceptance Criteria**:
- ✅ `avg()` returns correct average
- ✅ `min()` and `max()` return correct values
- ✅ Both aggregation tests pass
- ✅ Unit tests cover edge cases (empty sets, NULLs, single value)

---

### 2.3 Fix WHERE Clause Filtering - 🔴 NOT STARTED
**Impact**: +5.88% (1 test)  
**Estimated Time**: 0.5 days

#### Tasks:
- [ ] Review `execute_filter` implementation
- [ ] Debug `WHERE n.age > 25` not filtering correctly
- [ ] Test comparison operators (>, <, >=, <=, =, <>)
- [ ] Verify filter is applied to correct result set
- [ ] Add unit tests for WHERE clause variations
- [ ] Update cross-compatibility script to re-test

**Files to Modify**:
- `nexus-core/src/executor/mod.rs` (execute_filter function)

**Test Query**:
```cypher
MATCH (n:Person) WHERE n.age > 25 RETURN n.name, n.age
```

**Acceptance Criteria**:
- ✅ WHERE clause test passes
- ✅ Correct number of rows returned
- ✅ All comparison operators work
- ✅ Unit tests added

---

## Phase 3: Medium Priority Fixes (Target: 94% compatibility)

### 3.1 Fix ORDER BY Implementation - 🔴 NOT STARTED
**Impact**: +5.88% (1 test)  
**Estimated Time**: 1-2 days

#### Tasks:
- [ ] Verify ORDER BY operator exists in planner
- [ ] Implement sorting logic in executor
- [ ] Support ASC and DESC ordering
- [ ] Support multiple ORDER BY columns
- [ ] Handle NULL values in sorting
- [ ] Test with different data types (string, int, float)
- [ ] Add unit tests for ORDER BY
- [ ] Update cross-compatibility script to re-test

**Files to Modify**:
- `nexus-core/src/executor/mod.rs` (sorting implementation)
- `nexus-core/src/executor/planner.rs` (ORDER BY operator)

**Test Query**:
```cypher
MATCH (n:Person) RETURN n.name, n.age ORDER BY n.age DESC LIMIT 3
```

**Acceptance Criteria**:
- ✅ ORDER BY test passes
- ✅ Results are correctly sorted
- ✅ ASC and DESC both work
- ✅ NULL handling correct
- ✅ Unit tests added

---

### 3.2 Fix UNION Query Execution - 🔴 NOT STARTED
**Impact**: +5.88% (1 test)  
**Estimated Time**: 0.5-1 day

#### Tasks:
- [ ] Debug `execute_union` returning empty results
- [ ] Verify UNION operator pipeline execution
- [ ] Check result set combination logic
- [ ] Test UNION vs UNION ALL
- [ ] Verify column alignment
- [ ] Add unit tests for UNION
- [ ] Update cross-compatibility script to re-test

**Files to Modify**:
- `nexus-core/src/executor/mod.rs` (execute_union function)
- `nexus-core/src/executor/planner.rs` (UNION operator planning)

**Test Query**:
```cypher
MATCH (n:Person) RETURN n.name AS name 
UNION 
MATCH (c:Company) RETURN c.name AS name
```

**Acceptance Criteria**:
- ✅ UNION query test passes
- ✅ Results from both sides combined
- ✅ Duplicate removal works (UNION)
- ✅ UNION ALL preserves duplicates
- ✅ Unit tests added

---

## Phase 4: Optional Enhancement (Target: 100% compatibility)

### 4.1 Implement COUNT(DISTINCT ...) - 🔴 NOT STARTED
**Impact**: +5.88% (1 test)  
**Estimated Time**: 2-3 days

#### Tasks:
- [ ] Add DISTINCT keyword parsing in parser
- [ ] Implement DISTINCT logic in aggregations
- [ ] Support DISTINCT for count, sum, avg
- [ ] Handle NULL values in DISTINCT
- [ ] Add unit tests for DISTINCT
- [ ] Update cross-compatibility script to re-test

**Files to Modify**:
- `nexus-core/src/executor/parser.rs` (DISTINCT parsing)
- `nexus-core/src/executor/mod.rs` (DISTINCT in aggregations)
- `nexus-core/src/executor/planner.rs` (DISTINCT operator)

**Test Query**:
```cypher
MATCH (n:Person) RETURN count(DISTINCT n.age) AS unique_ages
```

**Acceptance Criteria**:
- ✅ COUNT(DISTINCT) test passes
- ✅ Returns correct unique count
- ✅ Works with other aggregations
- ✅ Unit tests added

---

## Phase 5: Testing & Validation

### 5.1 Run Full Test Suite - 🔴 NOT STARTED
- [ ] Run `cargo test --workspace`
- [ ] Ensure all tests pass
- [ ] Fix any regressions
- [ ] Run cross-compatibility script
- [ ] Verify >90% compatibility achieved

### 5.2 Manual Validation - 🔴 NOT STARTED
- [ ] Test each fixed feature manually with Neo4j
- [ ] Compare results side-by-side
- [ ] Document any intentional differences
- [ ] Create regression tests for all fixes

---

## Phase 6: Documentation

### 6.1 Update Documentation - 🔴 NOT STARTED
- [ ] Update `CHANGELOG.md` with new version (v0.9.8)
- [ ] Update `README.md` compatibility percentage
- [ ] Update `docs/neo4j-compatibility-report.md`
- [ ] Document known limitations
- [ ] Add examples for new features

### 6.2 Create Migration Guide - 🔴 NOT STARTED
- [ ] Document changes from previous version
- [ ] List breaking changes (if any)
- [ ] Provide upgrade instructions

---

## Success Metrics

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| **Compatibility Rate** | 52.94% | >90% | 🔴 |
| **Passing Tests** | 9/17 | 16+/17 | 🔴 |
| **Aggregations** | 0/2 | 2/2 | 🔴 |
| **Relationships** | 0/2 | 2/2 | 🔴 |
| **WHERE Clause** | 0/1 | 1/1 | 🔴 |
| **ORDER BY** | 0/1 | 1/1 | 🔴 |
| **UNION** | 0/1 | 1/1 | 🔴 |
| **DISTINCT** | 0/1 | 1/1 | ⚪ Optional |

---

## Timeline

| Phase | Duration | Start | End |
|-------|----------|-------|-----|
| **Investigation** | 1 day | ✅ Done | ✅ Done |
| **High Priority** | 4-5 days | TBD | TBD |
| **Medium Priority** | 2-3 days | TBD | TBD |
| **Optional** | 2-3 days | TBD | TBD |
| **Testing** | 1-2 days | TBD | TBD |
| **Documentation** | 1 day | TBD | TBD |

**Total Estimated**: 11-15 days (without optional DISTINCT feature: 9-12 days)

---

## Notes

### Completion Strategy
1. Complete Phase 2 (High Priority) first → 82% compatibility
2. Complete Phase 3 (Medium Priority) → 94% compatibility
3. Phase 4 (DISTINCT) is optional if time allows
4. Minimum acceptable: 90% (skip 1-2 medium priority items)

### Risk Mitigation
- Focus on quick wins first (WHERE, UNION)
- Aggregations and ORDER BY may require more time
- DISTINCT can be deferred to future version if needed

### Dependencies
- Neo4j instance must be running for validation
- Cross-compatibility script must be functional
- All existing tests must continue passing

