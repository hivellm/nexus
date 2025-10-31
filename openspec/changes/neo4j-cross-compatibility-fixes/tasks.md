# Implementation Tasks - Neo4j Cross-Compatibility Fixes

**Status**: 🟡 In Progress  
**Priority**: High  
**Started**: 2025-10-31  
**Current Compatibility**: 64.71% (11/17 tests passing)  
**Target**: >90% (16+/17 tests passing)

---

## 1. Investigation & Setup

- [x] 1.1 Run cross-compatibility test script
- [x] 1.2 Document failing tests and root causes
- [x] 1.3 Create OpenSpec documentation (proposal, analysis, tasks)
- [x] 1.4 Fix PowerShell script response parsing for Nexus arrays
- [x] 1.5 Verify which features already work (relationships, WHERE, UNION)

## 2. Aggregation Functions Support

- [x] 2.1 Analyze current aggregation implementation in planner
- [x] 2.2 Identify PropertyAccess vs Variable issue in aggregations
- [ ] 2.3 Add PropertyAccess support to avg() in planner
- [ ] 2.4 Add PropertyAccess support to min() in planner
- [ ] 2.5 Add PropertyAccess support to max() in planner
- [ ] 2.6 Add PropertyAccess support to sum() in planner
- [ ] 2.7 Update required_columns tracking for PropertyAccess
- [ ] 2.8 Update projection item generation for property expressions
- [ ] 2.9 Test avg(n.age) returns correct average
- [ ] 2.10 Test min(n.age) returns minimum value
- [ ] 2.11 Test max(n.age) returns maximum value
- [ ] 2.12 Fix min() NULL/0 value handling
- [ ] 2.13 Add unit tests for PropertyAccess aggregations
- [ ] 2.14 Verify cross-compatibility test passes

## 3. UNION Deduplication

- [x] 3.1 Verify UNION combines results from both sides
- [ ] 3.2 Analyze execute_union deduplication logic
- [ ] 3.3 Implement HashSet-based deduplication for UnionType::Distinct
- [ ] 3.4 Ensure UNION ALL preserves duplicates
- [ ] 3.5 Test UNION removes duplicates correctly
- [ ] 3.6 Add unit tests for UNION vs UNION ALL
- [ ] 3.7 Verify cross-compatibility test passes

## 4. COUNT DISTINCT Support

- [ ] 4.1 Add DISTINCT keyword parsing in function calls
- [ ] 4.2 Update Aggregation enum to include distinct flag
- [ ] 4.3 Implement distinct collection in execute_aggregate
- [ ] 4.4 Test COUNT(DISTINCT n.age) returns unique count
- [ ] 4.5 Handle NULL values in DISTINCT aggregation
- [ ] 4.6 Add unit tests for COUNT DISTINCT
- [ ] 4.7 Verify cross-compatibility test passes

## 5. ORDER BY Implementation

- [ ] 5.1 Verify ORDER BY operator in planner
- [ ] 5.2 Implement sorting logic in executor
- [ ] 5.3 Support ASC and DESC ordering
- [ ] 5.4 Handle NULL values in sorting
- [ ] 5.5 Test ORDER BY DESC with age values
- [ ] 5.6 Add unit tests for ORDER BY
- [ ] 5.7 Verify cross-compatibility test passes

## 6. Response Structure Fixes

- [ ] 6.1 Fix count query response structure issues
- [ ] 6.2 Ensure aggregation queries return proper row structure
- [ ] 6.3 Verify all queries return consistent format

## 7. Testing & Validation

- [ ] 7.1 Run cargo test --workspace
- [ ] 7.2 Fix any test regressions
- [ ] 7.3 Run cross-compatibility script
- [ ] 7.4 Verify >90% compatibility achieved
- [ ] 7.5 Add regression tests for all fixes
- [ ] 7.6 Manual validation against real Neo4j instance

## 8. Documentation

- [ ] 8.1 Update CHANGELOG.md with v0.9.8
- [ ] 8.2 Update README.md compatibility percentage
- [ ] 8.3 Update docs/neo4j-compatibility-report.md
- [ ] 8.4 Document any intentional differences
- [ ] 8.5 Add usage examples for new features
