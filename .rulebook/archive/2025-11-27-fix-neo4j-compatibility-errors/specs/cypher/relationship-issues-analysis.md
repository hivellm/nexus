# Relationship Query Issues Analysis

## Overview

Three relationship query tests are failing in Section 7 of the compatibility test suite. All three tests involve queries with multiple source nodes and relationship patterns.

## Failing Tests

### 7.19 Relationship with aggregation

**Query**: `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person`

**Expected**: 2 rows
- Alice with 2 jobs (Acme 2020, TechCorp 2021)
- Bob with 1 job (Acme 2019)

**Actual**: 1 row

**Issue**: Only 1 row returned, suggesting aggregation GROUP BY may not be processing all source nodes correctly when multiple nodes have relationships.

### 7.25 MATCH all connected nodes

**Query**: `MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name`

**Expected**: 2 rows
- Alice
- Bob

**Actual**: 1 row

**Issue**: DISTINCT may be removing rows incorrectly, or Expand not processing all source nodes when direction is Both.

### 7.30 Complex relationship query

**Query**: `MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year`

**Expected**: 3 rows
- Bob → Acme (2019)
- Alice → Acme (2020)
- Alice → TechCorp (2021)

**Actual**: 2 rows

**Issue**: One relationship not being found or processed by Expand. Possibly missing one of Alice's relationships.

## Code Analysis

### Expand Operator (`execute_expand`)

**Location**: `nexus-core/src/executor/mod.rs:2351-2587`

**Analysis**:
- Code iterates over all rows from `result_set` or `materialize_rows_from_variables`
- For each row, extracts source node ID and finds relationships using `find_relationships`
- Creates new row for each relationship found
- Logic appears correct for handling multiple source nodes

**Potential Issues**:
- May not be processing all source nodes if `result_set_as_rows` is not returning all rows
- May be filtering out rows incorrectly when `allowed_target_ids` is set

### Find Relationships (`find_relationships`)

**Location**: `nexus-core/src/executor/mod.rs:6031-6200`

**Analysis**:
- Uses multiple strategies: relationship storage, adjacency lists, relationship index
- Handles all directions (Outgoing, Incoming, Both)
- Filters by type_ids correctly
- Logic appears correct

**Potential Issues**:
- May not be finding all relationships for nodes with multiple relationships
- Adjacency list lookup may be incomplete

### Materialize Rows from Variables (`materialize_rows_from_variables`)

**Location**: `nexus-core/src/executor/mod.rs:7728-7772`

**Analysis**:
- Creates one row per index in the largest array
- Handles arrays and single values correctly
- Logic appears correct

**Potential Issues**:
- May not be creating rows correctly when multiple variables exist

### Aggregate Operator (`execute_aggregate_with_projections`)

**Location**: `nexus-core/src/executor/mod.rs:2911-3300`

**Analysis**:
- Groups rows by GROUP BY columns
- Processes aggregations per group
- Logic appears correct

**Potential Issues**:
- May not be grouping correctly when multiple source nodes have relationships
- May be losing rows during grouping process

### Distinct Operator (`execute_distinct`)

**Location**: `nexus-core/src/executor/mod.rs:5505-5563`

**Analysis**:
- Creates canonical keys from column values
- Uses HashSet to track seen keys
- Logic appears correct

**Potential Issues**:
- May be removing rows incorrectly when processing bidirectional relationships
- Key generation may not handle node objects correctly

## Root Cause Hypothesis

Based on code analysis, the most likely issues are:

1. **Expand not processing all source nodes**: When multiple nodes are matched initially, Expand may not be iterating over all of them correctly.

2. **Relationship lookup incomplete**: `find_relationships` may not be returning all relationships for nodes with multiple relationships.

3. **Row filtering**: Some rows may be filtered out incorrectly during Expand or subsequent operators.

4. **Bidirectional relationship handling**: For test 7.25 with `-[r]-` (Both direction), Expand may not be creating rows for both directions correctly.

## Test Data

From `scripts/test-neo4j-nexus-compatibility-200.ps1`:

```cypher
CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), 
       (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})
CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)
CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)
CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)
CREATE (p1)-[:KNOWS]->(p2)
```

**Expected relationships**:
- Alice → Acme (WORKS_AT, 2020)
- Alice → TechCorp (WORKS_AT, 2021)
- Bob → Acme (WORKS_AT, 2019)
- Alice → Bob (KNOWS)

## Next Steps

1. Add debug logging to Expand operator to verify all source nodes are processed
2. Add debug logging to find_relationships to verify all relationships are found
3. Verify materialize_rows_from_variables creates correct number of rows
4. Check if rows are being filtered incorrectly in subsequent operators
5. Test bidirectional relationship expansion specifically for test 7.25

