# Implementation Tasks - Cypher Query Composition

**Status**: 🔄 IN PROGRESS (74% Complete)  
**Priority**: Critical  
**Estimated**: 2-3 weeks  
**Dependencies**: 
- Cypher parser implementation
- Query executor

---

## 1. WITH Clause

- [x] 1.1 Add WithClause to Clause enum and parser
- [x] 1.2 Implement WITH handling in executor
- [x] 1.3 Implement intermediate result projection
- [x] 1.4 Implement WHERE filtering in WITH
- [x] 1.5 Support DISTINCT in WITH clause
- [x] 1.6 Test basic WITH projection
- [x] 1.7 Test WITH with WHERE filtering
- [x] 1.8 Test WITH DISTINCT
- [ ] 1.9 Test WITH in query chains
- [ ] 1.10 Variable binding between clauses

## 2. OPTIONAL MATCH

- [x] 2.1 Add optional field to MatchClause struct
- [x] 2.2 Parse "OPTIONAL MATCH" keyword
- [x] 2.3 Handle OPTIONAL MATCH in execution
- [x] 2.4 Implement LEFT OUTER JOIN semantics
- [x] 2.5 Handle NULL values for unmatched patterns
- [x] 2.6 Support multiple OPTIONAL MATCH patterns
- [x] 2.7 Test OPTIONAL MATCH with existing match
- [x] 2.8 Test OPTIONAL MATCH without match
- [x] 2.9 Test multiple OPTIONAL MATCH clauses
- [ ] 2.10 Test NULL handling in WHERE clauses

## 3. UNWIND

- [x] 3.1 Add UnwindClause to Clause enum
- [x] 3.2 Implement parse_unwind_clause()
- [x] 3.3 Implement list-to-row expansion
- [x] 3.4 Handle UNWIND with WHERE filtering
- [x] 3.5 Support expression-based UNWIND
- [x] 3.6 Test basic UNWIND with list literal
- [ ] 3.7 Test UNWIND with variable reference
- [ ] 3.8 Test UNWIND with WHERE
- [ ] 3.9 Test UNWIND in complex queries

## 4. UNION

- [ ] 4.1 Add UnionClause to Clause enum
- [ ] 4.2 Implement parse_union_clause()
- [ ] 4.3 Implement UNION (distinct results)
- [ ] 4.4 Implement UNION ALL (keep duplicates)
- [ ] 4.5 Column compatibility checking
- [ ] 4.6 Combine multiple query results
- [ ] 4.7 Test UNION with compatible columns
- [ ] 4.8 Test UNION ALL with duplicates
- [ ] 4.9 Test column count mismatch error
- [ ] 4.10 Test complex UNION queries

## 5. Quality & Documentation

- [ ] 5.1 Run full test suite (100% pass)
- [ ] 5.2 Achieve 95%+ coverage
- [ ] 5.3 Run clippy with -D warnings
- [ ] 5.4 Run cargo fmt --all
- [ ] 5.5 Update docs/specs/cypher-subset.md
- [ ] 5.6 Add WITH examples
- [ ] 5.7 Add OPTIONAL MATCH examples
- [ ] 5.8 Add UNWIND examples
- [ ] 5.9 Add UNION examples
- [ ] 5.10 Update CHANGELOG.md
