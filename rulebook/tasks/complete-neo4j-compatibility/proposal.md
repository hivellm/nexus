# Achieve Complete Neo4j/openCypher Compatibility

## Why

Nexus currently implements **~55% of openCypher features** with 195/195 compatibility tests passing (100% of implemented features). To become a true Neo4j alternative and serve a broader range of use cases, we need to achieve near-complete openCypher compatibility.

**Current Gaps:**
- Missing critical clauses: `OPTIONAL MATCH`, `CALL {}` subqueries, enhanced `FOREACH`
- Limited temporal support: Only 5/20+ temporal functions (25% complete)
- Missing graph algorithms: PageRank, community detection, centrality algorithms
- Incomplete list operations: No `extract()`, `filter()`, list comprehensions
- Limited pattern matching: No pattern comprehensions, EXISTS subqueries
- Missing predicate functions: Limited to 3/10 functions (30% complete)

**Impact:**
- **Time-series applications** cannot use temporal component extraction
- **Complex filtering** requires workarounds instead of EXISTS subqueries
- **Graph analytics** must export to external tools (no centrality, community detection)
- **Left outer joins** impossible without OPTIONAL MATCH
- **Data transformation** verbose without list comprehensions

## What Changes

Implement missing openCypher features across 4 major phases to achieve **90%+ compatibility**:

### Phase 1: Critical Missing Features (4-6 weeks)
- Implement `OPTIONAL MATCH` clause (left outer join semantics)
- Add temporal component extraction: `year()`, `month()`, `day()`, `hour()`, `minute()`, `second()`, `quarter()`, `week()`, `dayOfWeek()`, `dayOfYear()`
- Implement `EXISTS` subqueries for pattern existence checks
- Add list comprehensions: `[x IN list WHERE ... | ...]`
- Implement pattern comprehensions: `[(n)-[r]->(m) | m.prop]`

### Phase 2: Important Enhancements (3-4 weeks)
- Add advanced string functions: `left()`, `right()`, regex enhancements
- Implement list functions: `extract()`, `filter()`, `flatten()`, `zip()`
- Add temporal arithmetic and duration components
- Implement map projections: `n {.name, .age, ...}`
- Add `CALL {}` subqueries for batch operations
- Enhance constraint management (unique, existence constraints)

### Phase 3: Graph Analytics (4-6 weeks)
- Implement PageRank algorithm (`gds.pageRank`)
- Add community detection: `gds.louvain`, `gds.labelPropagation`
- Implement centrality algorithms: betweenness, closeness, degree, eigenvector
- Add enhanced pathfinding: A*, K shortest paths
- Implement triangle counting and clustering coefficient
- Add weakly/strongly connected components

### Phase 4: Advanced Features (2-3 weeks)
- Add remaining mathematical functions: inverse trig, logarithms
- Implement additional temporal functions and timezone handling
- Add advanced geospatial: polygon operations, area/perimeter
- Implement query management: `SHOW QUERIES`, `TERMINATE QUERY`
- Add performance hints and optimization directives
- Complete admin command coverage

**BREAKING**: None - All additions are backward compatible

## Impact

### Affected Specs
- MODIFIED: `docs/specs/cypher-subset.md` - Update supported features from 55% to 90%+
- MODIFIED: `nexus-core/src/executor/parser.rs` - Add new AST nodes for new clauses
- MODIFIED: `nexus-core/src/executor/mod.rs` - Implement 40+ new functions
- ADDED: `nexus-core/src/graph/analytics.rs` - Graph analytics algorithms
- MODIFIED: `docs/NEO4J_COMPATIBILITY_REPORT.md` - Update compatibility status

### Affected Code
- `nexus-core/src/executor/mod.rs` (~13,185 lines) - Add 40+ functions, OPTIONAL MATCH execution
- `nexus-core/src/executor/parser.rs` - Add AST nodes for new syntax
- `nexus-core/src/executor/planner.rs` - Enhance query planning for new clauses
- `nexus-core/src/graph/algorithms.rs` - Implement 15+ new graph algorithms
- `nexus-core/src/graph/procedures.rs` - Register new algorithm procedures
- `scripts/test-neo4j-nexus-compatibility-200.ps1` - Expand from 195 to 300+ tests

### Dependencies
- Requires: Current MVP complete (✅ Done)
- Optional: May require additional crates for algorithms (e.g., `petgraph` for advanced graph algorithms)

### Timeline
- **Phase 1 (Critical)**: 4-6 weeks
- **Phase 2 (Important)**: 3-4 weeks
- **Phase 3 (Analytics)**: 4-6 weeks
- **Phase 4 (Advanced)**: 2-3 weeks
- **Total Duration**: 13-19 weeks (~3-5 months)
- **Complexity**: High (architectural changes + extensive testing)

### Success Metrics
- openCypher compatibility: 55% → 90%+
- Compatibility tests passing: 195 → 300+ tests
- Function coverage: 60 → 100+ functions
- Graph algorithm procedures: 1 → 15+ algorithms
- Zero regressions on existing tests
- Performance impact: <10% overhead for new features

### Risk Assessment
- **Medium Risk**: OPTIONAL MATCH requires planner changes (may affect performance)
- **Low Risk**: Function additions are isolated and testable
- **Medium Risk**: Graph algorithms may have performance implications for large graphs
- **Low Risk**: All changes backward compatible

## Success Criteria

### Functional Requirements
- [ ] All Phase 1 features implemented and tested
- [ ] All Phase 2 features implemented and tested
- [ ] All Phase 3 graph algorithms working correctly
- [ ] All Phase 4 advanced features complete
- [ ] 300+ Neo4j compatibility tests passing
- [ ] Zero regressions on existing 195 tests
- [ ] Documentation complete for all new features

### Performance Requirements
- [ ] OPTIONAL MATCH performance within 2x of regular MATCH
- [ ] Temporal functions overhead < 5% vs current implementation
- [ ] Graph algorithms complete within reasonable time (< 1s for 10K nodes)
- [ ] Overall query performance degradation < 10%

### Quality Requirements
- [ ] Test coverage ≥ 95% for all new code
- [ ] All new functions documented with examples
- [ ] Compatibility report updated
- [ ] User guide includes examples for all new features
- [ ] No clippy warnings
- [ ] Code formatted with rustfmt

### Documentation Requirements
- [ ] `docs/specs/cypher-subset.md` updated to 90%+ coverage
- [ ] `docs/NEO4J_COMPATIBILITY_REPORT.md` updated
- [ ] `docs/USER_GUIDE.md` includes examples for all new features
- [ ] CHANGELOG.md updated with all additions
- [ ] Migration guide for users (if applicable)
- [ ] Performance tuning guide for graph algorithms

## References

- [openCypher Official Website](https://opencypher.org/)
- [openCypher Specification v9 (PDF)](https://s3.amazonaws.com/artifacts.opencypher.org/openCypher9.pdf)
- [openCypher GitHub Repository](https://github.com/opencypher/openCypher)
- [Neo4j Cypher Manual](https://neo4j.com/docs/cypher-manual/current/)
- [Graph Data Science Library](https://neo4j.com/docs/graph-data-science/current/)
- Current implementation: `nexus-core/src/executor/mod.rs` (lines 10300-11700)
- Compatibility tests: `scripts/test-neo4j-nexus-compatibility-200.ps1`
