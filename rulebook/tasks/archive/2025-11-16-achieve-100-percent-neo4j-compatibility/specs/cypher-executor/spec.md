# Cypher Executor Specification - 100% Neo4j Compatibility Fixes

## Purpose

This specification defines requirements for fixing remaining compatibility issues to achieve 100% compatibility with Neo4j query results, ensuring that Nexus can serve as a drop-in replacement for Neo4j in existing applications.

## Requirements

### Requirement: Aggregation Function Edge Cases
The system SHALL handle aggregation function edge cases identically to Neo4j.

#### Scenario: min() without MATCH
Given a query `RETURN min(5) AS min_val`
When executed
Then it returns `5` (not null), identical to Neo4j

#### Scenario: max() without MATCH
Given a query `RETURN max(15) AS max_val`
When executed
Then it returns `15` (not null), identical to Neo4j

#### Scenario: collect() without MATCH
Given a query `RETURN collect(1) AS collected`
When executed
Then it returns `[1]` (not empty array), identical to Neo4j

#### Scenario: sum() with empty MATCH
Given a query `MATCH (n:NonExistent) RETURN sum(n.age) AS total`
When executed
Then it returns `0` (not null), identical to Neo4j

### Requirement: WHERE Clause Edge Cases
The system SHALL handle WHERE clause edge cases identically to Neo4j.

#### Scenario: WHERE with IN operator (data duplication)
Given a query `MATCH (n:Person) WHERE n.name IN ['Alice', 'Bob'] RETURN count(n)`
When executed
Then it returns the correct count without data duplication, identical to Neo4j

#### Scenario: WHERE with empty IN list
Given a query `MATCH (n:Person) WHERE n.name IN [] RETURN count(n)`
When executed
Then it returns `0` (not all rows), identical to Neo4j

#### Scenario: WHERE with list contains
Given a query `MATCH (n:Person) WHERE 'dev' IN n.tags RETURN count(n)`
When executed
Then it correctly evaluates list property membership, identical to Neo4j

### Requirement: ORDER BY Functionality
The system SHALL support ORDER BY clauses identically to Neo4j.

#### Scenario: ORDER BY DESC
Given a query `MATCH (n:Person) RETURN n.age ORDER BY n.age DESC LIMIT 3`
When executed
Then it returns rows in descending order, identical to Neo4j

#### Scenario: ORDER BY multiple columns
Given a query `MATCH (n:Person) RETURN n.name, n.age ORDER BY n.age, n.name LIMIT 3`
When executed
Then it orders by multiple columns with correct precedence, identical to Neo4j

#### Scenario: ORDER BY with WHERE
Given a query `MATCH (n:Person) WHERE n.age > 25 RETURN n.name ORDER BY n.age DESC LIMIT 2`
When executed
Then it orders filtered results correctly, identical to Neo4j

#### Scenario: ORDER BY with aggregation
Given a query `MATCH (n:Person) RETURN n.city, count(n) AS count ORDER BY count DESC LIMIT 2`
When executed
Then it orders by aggregation result correctly, identical to Neo4j

### Requirement: Property Access Features
The system SHALL support property access features identically to Neo4j.

#### Scenario: Array property indexing
Given a query `MATCH (n:Person {name: 'Alice'}) RETURN n.tags[0] AS first_tag`
When executed
Then it returns the first element of the array property, identical to Neo4j

#### Scenario: size() with array properties
Given a query `MATCH (n:Person {name: 'Alice'}) RETURN size(n.tags) AS size`
When executed
Then it returns the length of the array property, identical to Neo4j

### Requirement: Aggregation with Collect
The system SHALL handle aggregation with collect() and list functions identically to Neo4j.

#### Scenario: collect() with head()/tail()/reverse()
Given a query `MATCH (n:Person) RETURN head(collect(n.name)) AS first_name`
When executed
Then it returns a single row with the modified list, identical to Neo4j

### Requirement: Relationship Query Fixes
The system SHALL handle relationship queries identically to Neo4j.

#### Scenario: Relationship direction counting
Given a query `MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS count`
When executed
Then it returns the correct count for directed relationships, identical to Neo4j

#### Scenario: Bidirectional relationship counting
Given a query `MATCH (a)-[r:KNOWS]-(b) RETURN count(r) AS count`
When executed
Then it returns the correct count for undirected relationships (counts each relationship once), identical to Neo4j

#### Scenario: Multiple relationship types counting
Given a query `MATCH ()-[r]->() WHERE type(r) IN ['KNOWS', 'WORKS_AT'] RETURN count(r)`
When executed
Then it returns the correct count for multiple relationship types, identical to Neo4j

### Requirement: Mathematical Operator Fixes
The system SHALL handle mathematical operators identically to Neo4j.

#### Scenario: Power operator in WHERE clause
Given a query `MATCH (n:Person) WHERE n.age = 2.0 ^ 5.0 RETURN count(n)`
When executed
Then it correctly evaluates power operator in WHERE clause, identical to Neo4j

#### Scenario: Modulo operator in WHERE clause
Given a query `MATCH (n:Person) WHERE n.age % 5 = 0 RETURN count(n)`
When executed
Then it correctly evaluates modulo operator in WHERE clause, identical to Neo4j

#### Scenario: Complex arithmetic expression precedence
Given a query `RETURN (10 + 5) * 2 ^ 2 AS result`
When executed
Then it returns `60` (power before multiplication), identical to Neo4j

### Requirement: String Function Edge Cases
The system SHALL handle string function edge cases identically to Neo4j.

#### Scenario: substring() with negative start index
Given a query `RETURN substring('hello', -2, 2) AS substr`
When executed
Then it handles negative indices correctly (returns empty result or error), identical to Neo4j

### Requirement: Test Environment
The system SHALL ensure proper test environment isolation.

#### Scenario: Test data isolation
Given multiple compatibility tests running sequentially
When executed
Then each test runs with clean data without duplication, identical to Neo4j test behavior

