# Query Executor Specification - Query Execution Fixes

## Purpose

This specification defines the requirements for fixing query execution bugs including DELETE with count(*), directed relationship matching, and multiple relationship types with RETURN clause. These fixes ensure correct query results and full Neo4j compatibility.

## MODIFIED Requirements

### Requirement: DELETE with RETURN count(*)

The system SHALL correctly return the actual count of deleted nodes and relationships when using DELETE with RETURN count(*), not returning 0.

#### Scenario: DELETE Nodes with count(*)

Given a graph with 5 nodes labeled Person
When executing query "MATCH (n:Person) DELETE n RETURN count(*)"
Then the system SHALL return count(*) = 5
And all 5 nodes SHALL be deleted
And the result SHALL match Neo4j behavior exactly

#### Scenario: DELETE Relationships with count(*)

Given a graph with 10 relationships of type KNOWS
When executing query "MATCH ()-[r:KNOWS]->() DELETE r RETURN count(*)"
Then the system SHALL return count(*) = 10
And all 10 relationships SHALL be deleted
And the result SHALL match Neo4j behavior exactly

#### Scenario: DELETE with WHERE and count(*)

Given a graph with 3 nodes where age > 30
When executing query "MATCH (n:Person) WHERE n.age > 30 DELETE n RETURN count(*)"
Then the system SHALL return count(*) = 3
And only matching nodes SHALL be deleted
And the result SHALL match Neo4j behavior exactly

#### Scenario: DELETE Multiple Entities with count(*)

Given a graph with 2 nodes and 3 relationships
When executing query "MATCH (n)-[r]->(m) DELETE n, r RETURN count(*)"
Then the system SHALL return count(*) = 5 (2 nodes + 3 relationships)
And all entities SHALL be deleted
And the result SHALL match Neo4j behavior exactly

### Requirement: Directed Relationship Matching with Labels

The system SHALL correctly match directed relationships with labels, returning accurate counts and results.

#### Scenario: Single Directed Relationship with Label

Given a graph with node A labeled Person and node B labeled Company
And a relationship A-[:WORKS_AT]->B
When executing query "MATCH (p:Person)-[r:WORKS_AT]->(c:Company) RETURN count(*)"
Then the system SHALL return count(*) = 1
And the result SHALL match Neo4j behavior exactly

#### Scenario: Directed Relationship Pattern Matching

Given a graph with directed relationship A-[:KNOWS]->B
When executing query "MATCH (a)-[r:KNOWS]->(b) RETURN count(*)"
Then the system SHALL return count(*) = 1
And the relationship SHALL be matched correctly
And the result SHALL match Neo4j behavior exactly

#### Scenario: Bidirectional Relationship Matching

Given a graph with relationship A-[:KNOWS]-B (undirected)
When executing query "MATCH (a)-[r:KNOWS]-(b) RETURN count(*)"
Then the system SHALL return count(*) = 1
And the relationship SHALL be matched correctly
And the result SHALL match Neo4j behavior exactly

### Requirement: Multiple Relationship Types with RETURN

The system SHALL correctly handle queries with multiple relationship types in pattern matching and RETURN clauses.

#### Scenario: Multiple Relationship Types Pattern

Given a graph with relationships of types KNOWS and LIKES
And a node connected via both types
When executing query "MATCH (a)-[r:KNOWS|LIKES]->(b) RETURN r"
Then the system SHALL return all matching relationships
And both KNOWS and LIKES relationships SHALL be included
And the result SHALL match Neo4j behavior exactly

#### Scenario: Multiple Relationship Types with RETURN Clause

Given a graph with relationships of types KNOWS and LIKES
When executing query "MATCH (a)-[r:KNOWS|LIKES]->(b) RETURN a, r, b"
Then the system SHALL return rows for each matching relationship
And variable binding SHALL work correctly for both types
And the result SHALL match Neo4j behavior exactly

#### Scenario: Multiple Relationship Types with WHERE

Given a graph with relationships of types KNOWS and LIKES
When executing query "MATCH (a)-[r:KNOWS|LIKES]->(b) WHERE r.since > 2020 RETURN r"
Then the system SHALL filter relationships by property
And only matching relationships SHALL be returned
And the result SHALL match Neo4j behavior exactly

#### Scenario: Three or More Relationship Types

Given a graph with relationships of types KNOWS, LIKES, and FOLLOWS
When executing query "MATCH (a)-[r:KNOWS|LIKES|FOLLOWS]->(b) RETURN count(*)"
Then the system SHALL return count of all matching relationships
And all three types SHALL be included in count
And the result SHALL match Neo4j behavior exactly

## ADDED Requirements

### Requirement: Query Result Accuracy

The system SHALL ensure all query results match Neo4j behavior exactly, with correct counts, variable bindings, and result sets.

#### Scenario: Count Accuracy Verification

Given various DELETE and MATCH queries
When executed against Nexus and Neo4j
Then all count(*) results SHALL match exactly
And all result set sizes SHALL match exactly
And no discrepancies SHALL exist

#### Scenario: Variable Binding Accuracy

Given queries with multiple relationship types
When executed with RETURN clause
Then all variable bindings SHALL be correct
And relationship properties SHALL be accessible
And node properties SHALL be accessible

### Requirement: Comprehensive Query Testing

The system SHALL have comprehensive test coverage for all query execution scenarios.

#### Scenario: Enable Previously Ignored DELETE Tests

Given DELETE tests marked with #[ignore]
When DELETE functionality is fixed
Then #[ignore] attributes SHALL be removed
And all DELETE tests SHALL pass
And test coverage SHALL be at least 95%

#### Scenario: Enable Previously Ignored Relationship Tests

Given relationship matching tests marked with #[ignore]
When relationship matching is fixed
Then #[ignore] attributes SHALL be removed
And all relationship tests SHALL pass
And test coverage SHALL be at least 95%

#### Scenario: Enable Previously Ignored Multiple Type Tests

Given multiple relationship type tests marked with #[ignore]
When multiple type handling is fixed
Then #[ignore] attributes SHALL be removed
And all multiple type tests SHALL pass
And test coverage SHALL be at least 95%

## Implementation Notes

### DELETE Count Tracking

During DELETE execution:
1. Track deleted node count
2. Track deleted relationship count
3. Sum counts for RETURN count(*)
4. Return accurate total count

### Relationship Pattern Matching

For directed relationships:
- `(a)-[r:TYPE]->(b)` matches only outgoing relationships
- `(a)<-[r:TYPE]-(b)` matches only incoming relationships
- `(a)-[r:TYPE]-(b)` matches both directions

### Multiple Relationship Type Handling

For pattern `[:TYPE1|TYPE2|TYPE3]`:
1. Match relationships of any specified type
2. Bind relationship variable correctly
3. Include all matching relationships in result set
4. Handle WHERE clause filtering correctly

### Testing Requirements

- Unit tests for each query type
- Integration tests comparing with Neo4j
- Edge case tests for boundary conditions
- Performance tests to ensure no regression

