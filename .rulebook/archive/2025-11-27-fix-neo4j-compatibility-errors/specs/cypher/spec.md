# Cypher Query Execution Specification

## MODIFIED Requirements

### Requirement: MATCH Query with Property Filters
The system SHALL correctly match nodes using inline property filters in MATCH clauses and WHERE clauses with property equality conditions, returning all matching nodes.

#### Scenario: MATCH with inline property filter
Given nodes exist with matching labels and properties
When executing a MATCH query with inline property filter like `MATCH (n:Person {name: 'Alice'}) RETURN n.name`
Then the system SHALL return all matching nodes (1 row expected)

#### Scenario: MATCH with WHERE property equality
Given nodes exist with matching labels and properties
When executing a MATCH query with WHERE clause like `MATCH (n:Person) WHERE n.name = 'Bob' RETURN n.name`
Then the system SHALL return all matching nodes (1 row expected)

#### Scenario: MATCH with WHERE numeric property
Given nodes exist with matching labels and numeric properties
When executing a MATCH query with WHERE clause like `MATCH (n:Person) WHERE n.age = 30 RETURN n.name`
Then the system SHALL return all matching nodes (1 row expected)

### Requirement: GROUP BY Aggregation
The system SHALL correctly group rows by specified expressions and apply aggregation functions to each group, returning one row per unique group value.

#### Scenario: COUNT with GROUP BY
Given multiple nodes with different property values
When executing a GROUP BY query like `MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY city`
Then the system SHALL return one row per unique city value with correct counts

#### Scenario: SUM with GROUP BY
Given multiple nodes with different property values
When executing a GROUP BY query like `MATCH (n:Person) RETURN n.city AS city, sum(n.age) AS total ORDER BY city`
Then the system SHALL return one row per unique city value with correct sums

#### Scenario: AVG with GROUP BY
Given multiple nodes with different property values
When executing a GROUP BY query like `MATCH (n:Person) RETURN n.city AS city, avg(n.age) AS avg_age ORDER BY city`
Then the system SHALL return one row per unique city value with correct averages

#### Scenario: GROUP BY with ORDER BY aggregation result
Given multiple nodes with different property values
When executing a GROUP BY query with ORDER BY on aggregation like `MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY cnt DESC`
Then the system SHALL return all groups sorted by aggregation result

#### Scenario: GROUP BY with LIMIT
Given multiple nodes with different property values
When executing a GROUP BY query with LIMIT like `MATCH (n:Person) RETURN n.city AS city, count(n) AS cnt ORDER BY cnt DESC LIMIT 2`
Then the system SHALL return limited groups sorted by aggregation result

### Requirement: DISTINCT Operation
The system SHALL remove duplicate values from result sets when DISTINCT is specified in RETURN clause.

#### Scenario: DISTINCT on property values
Given multiple nodes with some duplicate property values
When executing a DISTINCT query like `MATCH (n:Person) RETURN DISTINCT n.city AS city`
Then the system SHALL return only unique city values (one row per unique value)

### Requirement: UNION Queries
The system SHALL correctly combine results from multiple queries using UNION (removing duplicates) and UNION ALL (keeping duplicates), ensuring correct row counts.

#### Scenario: UNION two queries
Given nodes of different labels exist
When executing a UNION query like `MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name`
Then the system SHALL return union of unique results from both queries

#### Scenario: UNION ALL with duplicates
Given nodes of different labels exist
When executing a UNION ALL query like `MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name`
Then the system SHALL return all results from both queries including duplicates

#### Scenario: UNION with WHERE filter
Given nodes of different labels with various properties exist
When executing a UNION query with WHERE like `MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name`
Then the system SHALL return correct union of filtered and unfiltered results

#### Scenario: UNION with empty result set
Given a query with no matches
When executing a UNION query where first query returns no results like `MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (n:Person) RETURN n.name AS name`
Then the system SHALL return only results from the second query

### Requirement: Node Property Functions
The system SHALL correctly return node property information using functions like `properties()`, `labels()`, and `keys()` when nodes are matched.

#### Scenario: properties() function
Given nodes with properties exist
When executing a query like `MATCH (n:Person {name: 'Alice'}) RETURN properties(n) AS props`
Then the system SHALL return all properties of the matched node

#### Scenario: labels() function
Given nodes with labels exist
When executing a query like `MATCH (n:Person) WHERE n.name = 'David' RETURN labels(n) AS lbls`
Then the system SHALL return all labels of the matched node

#### Scenario: keys() function
Given nodes with properties exist
When executing a query like `MATCH (n:Person {name: 'Alice'}) RETURN keys(n) AS ks`
Then the system SHALL return all property keys of the matched node

### Requirement: Relationship Aggregation
The system SHALL correctly aggregate relationship data when grouping by node properties in relationship queries.

#### Scenario: Relationship aggregation with GROUP BY
Given relationships exist between nodes
When executing a query like `MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person`
Then the system SHALL return one row per person with correct relationship counts

#### Scenario: DISTINCT in relationship query
Given bidirectional relationships exist
When executing a query like `MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name`
Then the system SHALL return unique node names even with multiple relationships

#### Scenario: Complex relationship query with multiple projections
Given relationships with properties exist
When executing a query like `MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year`
Then the system SHALL return correct number of relationship rows matching the pattern

### Requirement: Property Access
The system SHALL correctly handle property access including non-existent properties, returning NULL for missing properties.

#### Scenario: String function with property
Given nodes with string properties exist
When executing a query like `MATCH (n:Person {name: 'Alice'}) RETURN toLower(n.name) AS result`
Then the system SHALL return the property value transformed by the function

#### Scenario: Non-existent property access
Given nodes exist without a specific property
When executing a query like `MATCH (n:Person {name: 'Alice'}) RETURN n.nonexistent AS result`
Then the system SHALL return NULL for the non-existent property (still returning 1 row)

#### Scenario: Array function with property function
Given nodes with properties exist
When executing a query like `MATCH (n:Person {name: 'Alice'}) RETURN size(keys(n)) AS prop_count`
Then the system SHALL return the size of the keys array

