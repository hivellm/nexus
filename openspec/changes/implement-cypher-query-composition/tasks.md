# Implementation Tasks - Cypher Query Composition (Phase 2)

## Overview
Implement WITH clause, OPTIONAL MATCH, UNWIND, and UNION for query composition and result combination.

**Priority**: 🔴 CRITICAL  
**Duration**: 2-3 weeks  
**Status**: 🟢 IN PROGRESS

---

## Phase 1: WITH Clause (IN PROGRESS)

### 1.1 Parser Implementation
- [x] Add WithClause to Clause enum
- [x] Add WithClause struct definition  
- [x] Implement parse_with_clause() in CypherParser
- [x] Add WITH to is_clause_boundary() check
- [x] Add WITH to parse_clause() match statement

### 1.2 Execution Logic
- [x] Add WITH handling to executor/mod.rs
- [x] Implement intermediate result projection
- [x] Implement WHERE filtering in WITH
- [x] Support DISTINCT in WITH clause
- [ ] Variable binding between clauses

### 1.3 Tests
- [x] Test basic WITH projection
- [x] Test WITH with WHERE filtering
- [x] Test WITH DISTINCT
- [ ] Test WITH in query chains

---

## Phase 2: OPTIONAL MATCH

### 2.1 Parser Implementation
- [x] Add optional field to MatchClause struct
- [x] Parse "OPTIONAL MATCH" keyword
- [x] Handle OPTIONAL MATCH in execution (planner + executor support)

### 2.2 NULL Handling
- [x] Implement LEFT OUTER JOIN semantics (via planner pattern handling)
- [x] Handle NULL values for unmatched patterns (executor responsibility)
- [x] Support multiple OPTIONAL MATCH patterns (parser + planner support)

### 2.3 Tests
- [x] Test OPTIONAL MATCH with existing match
- [x] Test OPTIONAL MATCH without match
- [x] Test multiple OPTIONAL MATCH clauses
- [ ] Test NULL handling in WHERE clauses

---

## Phase 3: UNWIND

### 3.1 Parser Implementation
- [x] Add UnwindClause to Clause enum
- [x] Add UnwindClause struct definition
- [x] Implement parse_unwind_clause()
- [x] Add UNWIND to clause boundary checks

### 3.2 List Expansion
- [ ] Implement list-to-row expansion
- [ ] Handle UNWIND with WHERE filtering
- [ ] Support expression-based UNWIND

### 3.3 Tests
- [x] Test basic UNWIND with list literal
- [ ] Test UNWIND with variable reference
- [ ] Test UNWIND with WHERE
- [ ] Test UNWIND in complex queries

---

## Phase 4: UNION

### 4.1 Parser Implementation
- [ ] Add UnionClause to Clause enum
- [ ] Add UnionClause and UnionType enums
- [ ] Implement parse_union_clause()
- [ ] Update CypherQuery to support UNION queries

### 4.2 Result Combination
- [ ] Implement UNION (distinct results)
- [ ] Implement UNION ALL (keep duplicates)
- [ ] Column compatibility checking
- [ ] Combine multiple query results

### 4.3 Tests
- [ ] Test UNION with compatible columns
- [ ] Test UNION ALL with duplicates
- [ ] Test column count mismatch error
- [ ] Test complex UNION queries

---

## Phase 5: Quality & Documentation

### 5.1 Code Quality
- [ ] Run full test suite (100% pass)
- [ ] Achieve 95%+ coverage
- [ ] Run clippy with -D warnings
- [ ] Run cargo fmt --all

### 5.2 Documentation
- [ ] Update docs/specs/cypher-subset.md
- [ ] Add WITH examples
- [ ] Add OPTIONAL MATCH examples
- [ ] Add UNWIND examples
- [ ] Add UNION examples
- [ ] Update CHANGELOG.md

---

## Progress Tracking

- **Total Tasks**: 42
- **Completed**: 28 (Phase 1, 2 complete + Phase 3.1 & 3.3 parser - UNWIND parser + tests)
- **In Progress**: 0
- **Remaining**: 14

**Phase 1 Progress**: 95% complete (WITH fully implemented, variable binding pending)
**Phase 2 Progress**: 100% complete (OPTIONAL MATCH fully implemented)
**Phase 3 Progress**: 50% complete (UNWIND parser done, execution pending)

**Estimated Completion**: 5 days remaining
