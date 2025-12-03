# Implement Cypher Write Operations (Phase 1)

## Why

Nexus currently only supports read operations (MATCH, RETURN). To be usable in production, it needs complete CRUD operations including MERGE, SET, DELETE, and REMOVE clauses.

This is **Phase 1** of complete Neo4j Cypher compatibility - the most critical phase for basic database functionality.

## What Changes

- Implement MERGE clause (create-or-match/upsert pattern)
- Implement SET clause (update properties and labels)
- Implement DELETE clause (remove nodes and relationships)
- Implement REMOVE clause (remove properties and labels)

**BREAKING**: None (extending existing functionality)

## Impact

### Affected Specs
- MODIFIED capability: \cypher-executor\ - Add MERGE, SET, DELETE, REMOVE support
- MODIFIED capability: \cypher-parser\ - Extend AST for write clauses

### Affected Code
- exus-core/src/executor/parser.rs\ - Add write clause parsing (~500 lines)
- exus-core/src/executor/planner.rs\ - Add write operation planning
- exus-core/src/graph.rs\ - Add mutation operations
- \	ests/cypher_tests/\ - Write operation tests (~400 lines)

### Dependencies
- Requires: Current MVP parser (MATCH, CREATE, RETURN)
- Requires: Transaction support for ACID writes

### Timeline
- **Duration**: 2-3 weeks
- **Complexity**: High (critical path)
- **Priority**: 🔴 CRITICAL (Phase 1 of 14)
