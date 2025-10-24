# Implementation Tasks - MVP Cypher Executor

## 1. Cypher Parser

- [ ] 1.1 Define AST (Abstract Syntax Tree) structs
- [ ] 1.2 Implement MATCH clause parsing (patterns, labels, variables)
- [ ] 1.3 Implement WHERE clause parsing (predicates, boolean ops)
- [ ] 1.4 Implement RETURN clause parsing (projections, aliases)
- [ ] 1.5 Implement ORDER BY clause parsing
- [ ] 1.6 Implement LIMIT/SKIP clause parsing
- [ ] 1.7 Implement parameter substitution ($param)
- [ ] 1.8 Implement aggregation function parsing (COUNT, SUM, AVG)
- [ ] 1.9 Add syntax error reporting with line/column numbers
- [ ] 1.10 Add unit tests for parser (95%+ coverage)

## 2. Query Planner

- [ ] 2.1 Implement cost model (cardinality estimation)
- [ ] 2.2 Implement pattern reordering (selectivity-based)
- [ ] 2.3 Implement index selection (label bitmap vs full scan)
- [ ] 2.4 Implement filter pushdown optimization
- [ ] 2.5 Implement limit pushdown (top-K)
- [ ] 2.6 Generate physical execution plan
- [ ] 2.7 Add plan visualization (EXPLAIN query)
- [ ] 2.8 Add unit tests for planner (95%+ coverage)

## 3. Physical Operators

- [ ] 3.1 Implement NodeByLabel operator (scan label bitmap)
- [ ] 3.2 Implement Filter operator (property predicates)
- [ ] 3.3 Implement Expand operator (traverse relationships)
- [ ] 3.4 Implement Project operator (return expressions)
- [ ] 3.5 Implement OrderBy operator (heap sort)
- [ ] 3.6 Implement Limit operator (top-K)
- [ ] 3.7 Implement Aggregate operator (hash aggregation)
- [ ] 3.8 Implement operator pipelining (iterator-based)
- [ ] 3.9 Add unit tests for each operator (95%+ coverage)

## 4. Aggregation Functions

- [ ] 4.1 Implement COUNT(*)
- [ ] 4.2 Implement COUNT(expr)
- [ ] 4.3 Implement SUM(expr)
- [ ] 4.4 Implement AVG(expr)
- [ ] 4.5 Implement MIN(expr)
- [ ] 4.6 Implement MAX(expr)
- [ ] 4.7 Implement GROUP BY logic (hash map)
- [ ] 4.8 Add unit tests for aggregations (95%+ coverage)

## 5. Integration & Testing

- [ ] 5.1 E2E test: Simple MATCH query
- [ ] 5.2 E2E test: MATCH with WHERE clause
- [ ] 5.3 E2E test: Pattern traversal (2-hop)
- [ ] 5.4 E2E test: Aggregation with GROUP BY
- [ ] 5.5 E2E test: ORDER BY + LIMIT
- [ ] 5.6 Performance test: Query latency (<10ms for simple queries)
- [ ] 5.7 Performance test: Throughput (1K+ queries/sec)
- [ ] 5.8 Verify 95%+ test coverage

## 6. Documentation & Quality

- [ ] 6.1 Update docs/ROADMAP.md (mark Phase 1.4 complete)
- [ ] 6.2 Add query examples to README
- [ ] 6.3 Update CHANGELOG.md with v0.3.0
- [ ] 6.4 Run all quality checks

