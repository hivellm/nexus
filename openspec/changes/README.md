# Nexus OpenSpec Changes

## Active Changes

### 🎯 Master Plans

#### `implement-cypher-complete-clauses/` - MASTER TRACKER
**Status**: Split into 14 modular changes
- Complete Neo4j Cypher compatibility roadmap
- Tracks all 14 implementation phases
- Timeline: 32-46 weeks total
- See `proposal.md` for full modular structure

### 🔴 Phase 1: Critical Write Operations

#### `implement-cypher-write-operations/` ✅ Ready to Start
- MERGE, SET, DELETE, REMOVE clauses
- Duration: 2-3 weeks
- Priority: CRITICAL (blocks all other phases)
- Tasks: 20 focused implementation tasks

### 🟠 Phase 2-7: Core Cypher Features

#### `implement-cypher-query-composition/` ✅ Ready
- WITH, OPTIONAL MATCH, UNWIND, UNION
- Duration: 2-3 weeks
- Depends on: Phase 1

#### `implement-cypher-advanced-features/`
- FOREACH, EXISTS, CASE, comprehensions
- Duration: 3-4 weeks
- Depends on: Phase 2

#### `implement-cypher-string-ops/`
- STARTS WITH, ENDS WITH, CONTAINS, regex
- Duration: 1 week
- Depends on: Phase 3

#### `implement-cypher-paths/`
- Variable-length paths, shortest path
- Duration: 2 weeks
- Depends on: Phase 4

#### `implement-cypher-functions/`
- 50+ built-in functions
- Duration: 3-4 weeks
- Depends on: Phase 5

#### `implement-cypher-schema-admin/`
- Indexes, constraints, transactions
- Duration: 2-3 weeks
- Depends on: Phase 6

### 🟡 Phase 8-12: Production Features

#### `implement-query-analysis/`
- EXPLAIN, PROFILE, query hints
- Duration: 1-2 weeks

#### `implement-data-import-export/`
- LOAD CSV, bulk operations
- Duration: 2-3 weeks

#### `implement-advanced-db-features/`
- USE DATABASE, subqueries
- Duration: 2 weeks

#### `implement-performance-monitoring/`
- Statistics, slow query logging
- Duration: 2-3 weeks

#### `implement-udf-procedures/`
- UDF framework, plugins
- Duration: 3-4 weeks

### 🔵 Phase 13-14: Optional Features

#### `implement-graph-algorithms/`
- Pathfinding, centrality, communities
- Duration: 4-5 weeks

#### `implement-geospatial/`
- Point type, spatial indexes
- Duration: 2-3 weeks

## Critical Bug Fixes

### `fix-data-api-endpoints/` 🔴 CRITICAL
- **Priority**: CRITICAL - Blocking basic CRUD operations
- **Status**: Ready to start
- **Issues**: GET/PUT/DELETE /data/nodes endpoints broken
- **Impact**: Users cannot retrieve, update, or delete nodes via REST
- **Solution**: Refactor to use Engine directly (like POST already does)
- **Timeline**: 3.5-4.5 hours
- **Blocks**: All other REST API work

## Other Active Changes

### `implement-v1-authentication/`
- Status: Phase 3 Complete (Phases 1-3 done, 85% complete)
- API key auth, RBAC, rate limiting, JWT, audit logging
- Phase 4 (Testing & Documentation) pending

### `implement-v1-authentication-remaining-todos/`
- Status: Not Started
- Phase 4: Testing, Documentation, Security Audit, Quality Checks
- Code TODOs: AuthContext extraction, API key improvements
- ~50 tasks remaining, estimated 2-3 weeks

### `implement-graph-correlation-analysis/`
- Status: In progress (57.5% MVP complete)
- Graph comparison and correlation

### `implement-v1-replication/`
- Status: Planned
- Master-replica replication

### `implement-v1-gui/`
- Status: Planned
- Web-based admin interface

### `implement-v2-sharding/`
- Status: Future
- Horizontal scaling

## Implementation Order

### 🔴 CRITICAL PRIORITY - Fix First

1. **START HERE**: `fix-data-api-endpoints/` - **MUST FIX BEFORE ANYTHING ELSE**
   - GET/PUT/DELETE /data/nodes endpoints broken
   - Blocks all REST API CRUD operations
   - Estimated: 3.5-4.5 hours

### Then Continue with Feature Work

2. `implement-cypher-write-operations` (Phase 1)
3. `implement-cypher-query-composition` (Phase 2)
4. Continue sequentially through Phases 3-7 for core Cypher
5. Phases 8-12 can be prioritized based on need
6. Phases 13-14 are optional advanced features

## Archive

Completed changes are moved to `archive/` after deployment.
