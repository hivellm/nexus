# Implement MVP Cypher Executor

## Why

With storage and indexes complete, we need query execution to make the graph database usable. The executor parses Cypher queries and executes them using physical operators.

This is the core functionality that enables users to query the graph database.

## What Changes

- Implement basic Cypher parser (MATCH, WHERE, RETURN, ORDER BY, LIMIT)
- Implement query planner with heuristic cost-based optimization
- Implement physical operators (NodeByLabel, Filter, Expand, Project, OrderBy, Limit)
- Implement aggregation operators (COUNT, SUM, AVG, MIN, MAX)
- Add comprehensive tests (95%+ coverage)

**BREAKING**: None (new functionality)

## Impact

### Affected Specs
- NEW capability: `cypher-executor`
- NEW capability: `query-planner`

### Affected Code
- `nexus-core/src/executor/parser.rs` - Cypher parser (~600 lines)
- `nexus-core/src/executor/planner.rs` - Query planner (~400 lines)
- `nexus-core/src/executor/operators.rs` - Physical operators (~800 lines)
- `tests/executor_tests.rs` - Query tests (~500 lines)

### Dependencies
- Requires: `implement-mvp-storage` AND `implement-mvp-indexes`

### Timeline
- **Duration**: 2-3 weeks
- **Complexity**: High (parser + planner + operators)

