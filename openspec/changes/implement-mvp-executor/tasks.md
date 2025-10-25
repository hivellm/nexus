# Implementation Tasks - MVP Cypher Executor

## 1. Cypher Parser

- [x] 1.1 Define AST (Abstract Syntax Tree) structs
- [x] 1.2 Implement MATCH clause parsing (patterns, labels, variables)
- [x] 1.3 Implement WHERE clause parsing (predicates, boolean ops)
- [x] 1.4 Implement RETURN clause parsing (projections, aliases)
- [x] 1.5 Implement ORDER BY clause parsing
- [x] 1.6 Implement LIMIT/SKIP clause parsing
- [x] 1.7 Implement parameter substitution ($param)
- [x] 1.8 Implement aggregation function parsing (COUNT, SUM, AVG)
- [x] 1.9 Add syntax error reporting with line/column numbers
- [x] 1.10 Add unit tests for parser (95%+ coverage)

## 2. Query Planner

- [x] 2.1 Implement cost model (cardinality estimation)
- [x] 2.2 Implement pattern reordering (selectivity-based)
- [x] 2.3 Implement index selection (label bitmap vs full scan)
- [x] 2.4 Implement filter pushdown optimization
- [x] 2.5 Implement limit pushdown (top-K)
- [x] 2.6 Generate physical execution plan
- [x] 2.7 Add plan visualization (EXPLAIN query)
- [x] 2.8 Add unit tests for planner (95%+ coverage)

## 3. Physical Operators

- [x] 3.1 Implement NodeByLabel operator (scan label bitmap)
- [x] 3.2 Implement Filter operator (property predicates)
- [x] 3.3 Implement Expand operator (traverse relationships)
- [x] 3.4 Implement Project operator (return expressions)
- [x] 3.5 Implement OrderBy operator (heap sort)
- [x] 3.6 Implement Limit operator (top-K)
- [x] 3.7 Implement Aggregate operator (hash aggregation)
- [x] 3.8 Implement operator pipelining (iterator-based)
- [x] 3.9 Add unit tests for each operator (95%+ coverage)

## 4. Aggregation Functions

- [x] 4.1 Implement COUNT(*)
- [x] 4.2 Implement COUNT(expr)
- [x] 4.3 Implement SUM(expr)
- [x] 4.4 Implement AVG(expr)
- [x] 4.5 Implement MIN(expr)
- [x] 4.6 Implement MAX(expr)
- [x] 4.7 Implement GROUP BY logic (hash map)
- [x] 4.8 Add unit tests for aggregations (95%+ coverage)

## 5. Integration & Testing

- [x] 5.1 E2E test: Simple MATCH query
- [x] 5.2 E2E test: MATCH with WHERE clause
- [x] 5.3 E2E test: Pattern traversal (2-hop)
- [x] 5.4 E2E test: Aggregation with GROUP BY
- [x] 5.5 E2E test: ORDER BY + LIMIT
- [ ] 5.6 Performance test: Query latency (<10ms for simple queries)
- [ ] 5.7 Performance test: Throughput (1K+ queries/sec)
- [x] 5.8 Verify 95%+ test coverage

## 6. Documentation & Quality

- [x] 6.1 Update docs/ROADMAP.md (mark Phase 1.4 complete)
- [x] 6.2 Add query examples to README
- [x] 6.3 Update CHANGELOG.md with v0.3.0
- [x] 6.4 Run all quality checks

