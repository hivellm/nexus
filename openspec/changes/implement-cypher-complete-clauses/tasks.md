# Implementation Tasks - Complete Neo4j Cypher Support (MASTER TRACKER)

**NOTE**: Tasks have been split into 14 focused change proposals for better management.

## Modular Change Structure

### ✅ Change Proposals Created:
*(Status review: 2025-11-11 – no implementation work has started beyond basic parser stubs; all runtime behavior, tests, and documentation remain pending.)*

1. **implement-cypher-write-operations** - Phase 1: MERGE, SET, DELETE, REMOVE
   - Priority: 🔴 CRITICAL
   - Duration: 2-3 weeks
   - Status: ✅ COMPLETED (2025-11-11) - Full implementation with tests, all 390 tests passing

2. **implement-cypher-query-composition** - Phase 2: WITH, OPTIONAL MATCH, UNWIND, UNION
   - Priority: 🟠 HIGH
   - Duration: 2-3 weeks
   - Status: ✅ COMPLETED (2025-11-01) - MVP features complete, archived

3. **implement-cypher-advanced-features** - Phase 3: FOREACH, EXISTS, CASE, comprehensions
   - Priority: 🟠 HIGH
   - Duration: 3-4 weeks
   - Status: ✅ COMPLETED (2025-11-11) - Full implementation with 75 comprehensive tests, all passing

4. **implement-cypher-string-ops** - Phase 4: String operators and regex
   - Priority: 🟡 MEDIUM
   - Duration: 1 week
   - Status: ✅ COMPLETED (2025-11-11) - Parsing and evaluation implemented for STARTS WITH, ENDS WITH, CONTAINS, and regex (=~)

5. **implement-cypher-paths** - Phase 5: Variable-length paths, shortest path
   - Priority: 🟡 MEDIUM
   - Duration: 2 weeks
  - Status: ⚪ Not started (quantifier parsing exists; traversal/algorithms missing)

6. **implement-cypher-functions** - Phase 6: 50+ built-in functions
   - Priority: 🟡 MEDIUM
   - Duration: 3-4 weeks
   - Status: ⚪ Not started

7. **implement-cypher-schema-admin** - Phase 7: Indexes, constraints, transactions
   - Priority: 🟠 HIGH
   - Duration: 2-3 weeks
   - Status: ⚪ Not started

8. **implement-query-analysis** - Phase 8: EXPLAIN, PROFILE, hints
   - Priority: 🟠 HIGH
   - Duration: 1-2 weeks
   - Status: ⚪ Not started

9. **implement-data-import-export** - Phase 9: LOAD CSV, bulk operations
   - Priority: 🟠 HIGH
   - Duration: 2-3 weeks
   - Status: ⚪ Not started

10. **implement-advanced-db-features** - Phase 10: USE DATABASE, subqueries
    - Priority: 🟡 MEDIUM
    - Duration: 2 weeks
    - Status: ⚪ Not started

11. **implement-performance-monitoring** - Phase 11: Statistics, slow query logging
    - Priority: 🟡 MEDIUM
    - Duration: 2-3 weeks
    - Status: ⚪ Not started

12. **implement-udf-procedures** - Phase 12: UDF framework, plugins
    - Priority: 🟡 MEDIUM
    - Duration: 3-4 weeks
    - Status: ⚪ Not started

13. **implement-graph-algorithms** - Phase 13: Pathfinding, centrality, communities
    - Priority: 🔵 OPTIONAL
    - Duration: 4-5 weeks
    - Status: ⚪ Not started

14. **implement-geospatial** - Phase 14: Point type, spatial indexes
    - Priority: 🔵 OPTIONAL
    - Duration: 2-3 weeks
    - Status: ⚪ Not started

## Implementation Order

### Critical Path (Phases 1-3)
Must be implemented first for basic Cypher compatibility:
- Phase 1: Write operations (MERGE, SET, DELETE, REMOVE)
- Phase 2: Query composition (WITH, OPTIONAL MATCH, UNWIND, UNION)
- Phase 3: Advanced features (FOREACH, EXISTS, CASE)

### High Priority (Phases 4-9)
Important for production use:
- Phase 4-6: String ops, paths, functions
- Phase 7: Schema & administration
- Phase 8-9: Query analysis & data import/export

### Medium Priority (Phases 10-12)
Enhances functionality:
- Phase 10: Advanced database features
- Phase 11: Performance monitoring
- Phase 12: UDF & procedures

### Optional (Phases 13-14)
Specialized features:
- Phase 13: Graph algorithms
- Phase 14: Geospatial support

## Original Detailed Tasks

**NOTE**: The detailed tasks below are preserved for reference.
**Active work should use the modular change proposals listed above.**

---

## Phase 1: Critical Write Operations

### 1.1 MERGE Clause Implementation
- [ ] 1.1.1 Add MergeClause to Clause enum in parser
- [ ] 1.1.2 Implement MERGE pattern parsing in parser.rs
- [ ] 1.1.3 Implement MERGE execution logic (match-or-create semantics)
- [ ] 1.1.4 Add ON CREATE/ON MATCH SET support for MERGE
- [ ] 1.1.5 Add MERGE testing in cypher tests
- [ ] 1.1.6 Update executor to handle MERGE in UTXC transactions

### 1.2 SET Clause Implementation
- [ ] 1.2.1 Add SetClause to Clause enum in parser
- [ ] 1.2.2 Implement SET property updates (node and relationship)
- [ ] 1.2.3 Implement SET label addition
- [ ] 1.2.4 Implement SET with expressions (n.age = n.age + 1)
- [ ] 1.2.5 Add SET testing in cypher tests
- [ ] 1.2.6 Update graph.rs to support in-place property updates

### 1.3 DELETE Clause Implementation
- [ ] 1.3.1 Add DeleteClause to Clause enum in parser
- [ ] 1.3.2 Implement DELETE node (with relationship check)
- [ ] 1.3.3 Implement DELETE relationship
- [ ] 1.3.4 Implement DETACH DELETE (auto-delete relationships)
- [ ] 1.3.5 Add DELETE testing in cypher tests
- [ ] 1.3.6 Update graph.rs to support deletion with referential integrity

### 1.4 REMOVE Clause Implementation
- [ ] 1.4.1 Add RemoveClause to Clause enum in parser
- [ ] 1.4.2 Implement REMOVE property
- [ ] 1.4.3 Implement REMOVE label
- [ ] 1.4.4 Add REMOVE testing in cypher tests
- [ ] 1.4.5 Update graph.rs to support property/label removal

**Phase 1 Testing & Quality**:
- [ ] Run full test suite for Phase 1
- [ ] Achieve 95%+ coverage for Phase 1
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 1

## Phase 2: Query Composition

### 2.1 WITH Clause Implementation
- [ ] 2.1.1 Add WithClause to Clause enum in parser
- [ ] 2.1.2 Implement WITH projection and filtering
- [ ] 2.1.3 Implement WITH aggregation (pre-aggregate before next clause)
- [ ] 2.1.4 Implement query piping semantics
- [ ] 2.1.5 Add WITH testing in cypher tests
- [ ] 2.1.6 Update executor to handle intermediate result sets

### 2.2 OPTIONAL MATCH Implementation
- [ ] 2.2.1 Add OptionalMatch to MatchClause variants
- [ ] 2.2.2 Implement LEFT OUTER JOIN semantics
- [ ] 2.2.3 Implement NULL handling for unmatched patterns
- [ ] 2.2.4 Add OPTIONAL MATCH testing in cypher tests
- [ ] 2.2.5 Update planner to generate LEFT JOIN plans

### 2.3 UNWIND Clause Implementation
- [ ] 2.3.1 Add UnwindClause to Clause enum in parser
- [ ] 2.3.2 Implement list to row expansion
- [ ] 2.3.3 Implement UNWIND with WHERE filtering
- [ ] 2.3.4 Add UNWIND testing in cypher tests
- [ ] 2.3.5 Update executor to handle row expansion

### 2.4 UNION/UNION ALL Implementation
- [ ] 2.4.1 Add Union/UnionAll to top-level query structure
- [ ] 2.4.2 Implement UNION (with duplicates removed)
- [ ] 2.4.3 Implement UNION ALL (keep duplicates)
- [ ] 2.4.4 Implement column compatibility checking
- [ ] 2.4.5 Add UNION testing in cypher tests
- [ ] 2.4.6 Update executor to combine multiple query results

### 2.5 CALL Procedures (Complete Support)
- [ ] 2.5.1 Extend existing CALL implementation beyond vector.knn
- [ ] 2.5.2 Add procedure registry for built-in procedures
- [ ] 2.5.3 Implement YIELD clause filtering
- [ ] 2.5.4 Add procedure testing in cypher tests
- [ ] 2.5.5 Document procedure API for extension

**Phase 2 Testing & Quality**:
- [ ] Run full test suite for Phase 2
- [ ] Achieve 95%+ coverage for Phase 2
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 2

## Phase 3: Advanced Query Features

### 3.1 FOREACH Clause Implementation
- [ ] 3.1.1 Add ForeachClause to Clause enum in parser
- [ ] 3.1.2 Implement iteration over lists
- [ ] 3.1.3 Implement FOREACH with SET/DELETE operations
- [ ] 3.1.4 Add FOREACH testing in cypher tests

### 3.2 EXISTS Subqueries Implementation
- [ ] 3.2.1 Add EXISTS to WHERE expression parsing
- [ ] 3.2.2 Implement existential pattern checking
- [ ] 3.2.3 Add EXISTS testing in cypher tests
- [ ] 3.2.4 Update optimizer to handle EXISTS efficiently

### 3.3 CASE Expressions Implementation
- [ ] 3.3.1 Add CaseExpression to expression AST
- [ ] 3.3.2 Implement simple CASE (value-based)
- [ ] 3.3.3 Implement generic CASE (predicate-based)
- [ ] 3.3.4 Add CASE testing in cypher tests

### 3.4 Map Projections Implementation
- [ ] 3.4.1 Add MapProjection to RETURN expression AST
- [ ] 3.4.2 Implement property selection (n {.name, .age})
- [ ] 3.4.3 Implement virtual keys in projections
- [ ] 3.4.4 Add map projection testing in cypher tests

### 3.5 List Comprehensions Implementation
- [ ] 3.5.1 Add ListComprehension to expression AST
- [ ] 3.5.2 Implement list comprehension with filtering
- [ ] 3.5.3 Implement list comprehension with transformation
- [ ] 3.5.4 Add list comprehension testing in cypher tests

### 3.6 Pattern Comprehensions Implementation
- [ ] 3.6.1 Add PatternComprehension to expression AST
- [ ] 3.6.2 Implement pattern-based list collection
- [ ] 3.6.3 Add pattern comprehension testing in cypher tests

**Phase 3 Testing & Quality**:
- [ ] Run full test suite for Phase 3
- [ ] Achieve 95%+ coverage for Phase 3
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 3

## Phase 4: String Operations

### 4.1 String Predicate Operators
- [ ] 4.1.1 Add STARTS WITH operator to expression parser
- [ ] 4.1.2 Add ENDS WITH operator to expression parser
- [ ] 4.1.3 Add CONTAINS operator to expression parser
- [ ] 4.1.4 Implement string matching evaluation
- [ ] 4.1.5 Add string operator testing in cypher tests

### 4.2 Regular Expression Support
- [ ] 4.2.1 Add regex operator (=~) to expression parser
- [ ] 4.2.2 Integrate regex library (regex crate)
- [ ] 4.2.3 Implement PCRE-compatible regex matching
- [ ] 4.2.4 Add regex testing in cypher tests

**Phase 4 Testing & Quality**:
- [ ] Run full test suite for Phase 4
- [ ] Achieve 95%+ coverage for Phase 4
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 4

## Phase 5: Variable-Length Paths

### 5.1 Path Quantifiers
- [ ] 5.1.1 Implement fixed-length paths (*5)
- [ ] 5.1.2 Implement range paths (*1..3)
- [ ] 5.1.3 Implement unbounded paths (*)
- [ ] 5.1.4 Add path quantifier testing in cypher tests
- [ ] 5.1.5 Update graph traversal to handle variable-length

### 5.2 Shortest Path Functions
- [ ] 5.2.1 Add shortestPath() function to expression AST
- [ ] 5.2.2 Implement BFS-based shortest path algorithm
- [ ] 5.2.3 Implement allShortestPaths() function
- [ ] 5.2.4 Add shortest path testing in cypher tests
- [ ] 5.2.5 Update planner to optimize path queries

**Phase 5 Testing & Quality**:
- [ ] Run full test suite for Phase 5
- [ ] Achieve 95%+ coverage for Phase 5
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 5

## Phase 6: Built-in Functions

### 6.1 Scalar Functions - String
- [ ] 6.1.1 Implement substring(), toLower(), toUpper()
- [ ] 6.1.2 Implement trim(), split(), replace()
- [ ] 6.1.3 Add string function testing

### 6.2 Scalar Functions - Math
- [ ] 6.2.1 Implement abs(), ceil(), floor(), round()
- [ ] 6.2.2 Implement sqrt(), sin(), cos(), tan()
- [ ] 6.2.3 Add math function testing

### 6.3 Scalar Functions - Temporal
- [ ] 6.3.1 Implement date(), datetime(), time()
- [ ] 6.3.2 Implement timestamp(), duration()
- [ ] 6.3.3 Add temporal function testing

### 6.4 Scalar Functions - Type Conversion
- [ ] 6.4.1 Implement toInteger(), toFloat(), toString()
- [ ] 6.4.2 Implement toBoolean(), toDate()
- [ ] 6.4.3 Add type conversion testing

### 6.5 Additional Aggregations
- [ ] 6.5.1 Implement COLLECT() aggregation
- [ ] 6.5.2 Implement percentileDisc(), percentileCont()
- [ ] 6.5.3 Implement stDev(), stDevP()
- [ ] 6.5.4 Add aggregation function testing

### 6.6 List Functions
- [ ] 6.6.1 Implement size(), head(), tail(), last()
- [ ] 6.6.2 Implement reduce(), extract()
- [ ] 6.6.3 Add list function testing

### 6.7 Predicate Functions
- [ ] 6.7.1 Implement all(), any(), none()
- [ ] 6.7.2 Implement single() predicate
- [ ] 6.7.3 Add predicate function testing

### 6.8 Path Functions
- [ ] 6.8.1 Implement nodes() function
- [ ] 6.8.2 Implement relationships() function
- [ ] 6.8.3 Implement length() function
- [ ] 6.8.4 Add path function testing

**Phase 6 Testing & Quality**:
- [ ] Run full test suite for Phase 6
- [ ] Achieve 95%+ coverage for Phase 6
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 6

## Phase 7: Schema & Administration

### 7.1 Index Management
- [ ] 7.1.1 Implement CREATE INDEX parsing
- [ ] 7.1.2 Implement DROP INDEX parsing
- [ ] 7.1.3 Implement index creation in catalog
- [ ] 7.1.4 Add index management testing

### 7.2 Constraint Management
- [ ] 7.2.1 Implement CREATE CONSTRAINT parsing
- [ ] 7.2.2 Implement DROP CONSTRAINT parsing
- [ ] 7.2.3 Implement constraint enforcement
- [ ] 7.2.4 Add constraint management testing

### 7.3 Transaction Commands
- [ ] 7.3.1 Implement BEGIN transaction parsing
- [ ] 7.3.2 Implement COMMIT transaction
- [ ] 7.3.3 Implement ROLLBACK transaction
- [ ] 7.3.4 Add transaction command testing

### 7.4 Database Management
- [ ] 7.4.1 Implement SHOW DATABASES parsing
- [ ] 7.4.2 Implement CREATE DATABASE
- [ ] 7.4.3 Implement DROP DATABASE
- [ ] 7.4.4 Add database management testing

### 7.5 User Management
- [ ] 7.5.1 Implement SHOW USERS parsing
- [ ] 7.5.2 Implement CREATE USER
- [ ] 7.5.3 Implement GRANT/REVOKE permissions
- [ ] 7.5.4 Add user management testing

**Phase 7 Testing & Quality**:
- [ ] Run full test suite for Phase 7
- [ ] Achieve 95%+ coverage for Phase 7
- [ ] Run clippy with -D warnings
- [ ] Update CHANGELOG.md for Phase 7

## Documentation & Completion

### Documentation Updates
- [ ] Update docs/specs/cypher-subset.md with all new clauses
- [ ] Update docs/ROADMAP.md with implementation progress
- [ ] Create docs/API.md if needed for new functions
- [ ] Update README.md with Cypher compatibility status

### Final Quality Checks
- [ ] Run complete test suite (100% pass rate required)
- [ ] Verify 95%+ code coverage for entire parser/executor
- [ ] Run cargo clippy with -D warnings (no warnings allowed)
- [ ] Run cargo fmt --all (formatting check)
- [ ] Run type-check / compilation check
- [ ] Update CHANGELOG.md with complete feature list
- [ ] Create migration guide for users upgrading from MVP

### Deployment Preparation
- [ ] Tag release version (v1.0.0)
- [ ] Update version in Cargo.toml
- [ ] Create release notes
- [ ] Archive this change to openspec/changes/archive/

## Estimated Timeline

- **Phase 1**: 2-3 weeks (Critical write operations)
- **Phase 2**: 2-3 weeks (Query composition)
- **Phase 3**: 3-4 weeks (Advanced query features)
- **Phase 4**: 1 week (String operations)
- **Phase 5**: 2 weeks (Variable-length paths)
- **Phase 6**: 3-4 weeks (Built-in functions)
- **Phase 7**: 2-3 weeks (Schema & administration)
- **Documentation & Testing**: 1 week
- **Total**: 16-22 weeks (~4-5 months)

## Notes

- Each phase should be implemented and fully tested before moving to the next
- Priority: Phase 1 (write operations) is most critical for usability
- Phase 2 (WITH, OPTIONAL MATCH) is essential for complex queries
- Code quality standards must be maintained throughout (95%+ coverage, no clippy warnings)
- All changes must follow AGENTS.md Rust guidelines (Edition 2024, nightly toolchain)

