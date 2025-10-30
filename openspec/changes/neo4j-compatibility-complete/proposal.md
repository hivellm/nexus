# Neo4j Full Compatibility - Proposal

## Problem Statement

During comparison testing between Nexus and Neo4j using the same classify cache data, we identified compatibility gaps:

- **Match Rate**: 60% (3/5 tests passing)
- **Issues**: Class nodes and MENTIONS relationships return different results
- **Impact**: Nexus cannot fully replace Neo4j for classify data queries

## Current State

### Working Features ✅
- Document, Module, and Function aggregate queries return matching counts
- Basic Cypher queries (MATCH, RETURN, count) work
- Data import from classify cache (213 entries imported)

### Issues ⚠️
- Class nodes: previously returned 0 (fixed), but returned rows still lack properties
- Relationships: aggregate counts match, but `RETURN d, e` only yields bare node IDs in Nexus
- Query result format differs: Neo4j includes node/edge properties and multiple columns; Nexus currently collapses to a single column with empty properties
- Some Cypher projection features (aliases, scalar columns) not fully supported yet
- Need parity on property serialization before marking compatibility complete

## Root Cause Analysis

Potential causes:

1. **Import Issues**
   - Classes may not be created during import
   - Relationships may not be created properly
   - MERGE logic may differ from Neo4j

2. **Cypher Parser**
   - Some query patterns may not be recognized
   - Label matching may have edge cases
   - Relationship patterns may not work correctly

3. **Data Storage**
   - Label mappings may be incorrect
   - Relationship types may not be stored correctly
   - Property access may differ

## Proposed Solution

### Phase 1: Investigation (Week 1)
1. Compare actual data in both systems
2. Verify import logs and errors
3. Test individual queries to identify exact differences
4. Document all discrepancies

### Phase 2: Fixes (Weeks 2-3)
1. Fix import logic for missing node/relationship types
2. Enhance Cypher parser for missing features
3. Ensure label and relationship type handling matches Neo4j
4. Fix any property handling differences

### Phase 3: Validation (Week 4)
1. Create comprehensive test suite
2. Run full compatibility tests
3. Verify 100% match rate
4. Document any intentional differences

## Implementation Tasks

See `tasks.md` for detailed task list.

## Success Metrics

- **Target**: 100% query result match rate
- **Current**: 60% match rate
- **Tests**: All comparison queries must pass

## Risks

- Some Neo4j features may be difficult to replicate exactly
- Performance may differ due to different storage engines
- May need to make some intentional differences for architectural reasons

## Timeline

- **Week 1**: Investigation and root cause analysis
- **Week 2**: Fix import and basic query issues
- **Week 3**: Fix advanced query and relationship issues
- **Week 4**: Testing, validation, and documentation

## Dependencies

- Classify cache data import working
- Cypher parser implementation
- Storage engine for nodes and relationships
- REST API endpoints for Cypher execution

