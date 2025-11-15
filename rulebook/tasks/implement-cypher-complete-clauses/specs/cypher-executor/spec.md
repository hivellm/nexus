## ADDED Requirements

### Requirement: MERGE Clause Support
The system SHALL support the MERGE clause for idempotent create-or-match operations in Cypher queries.

#### Scenario: Create node if not exists
- **WHEN** executing `MERGE (n:Person {name: 'Alice'}) RETURN n`
- **THEN** create the node if it doesn't exist, otherwise match the existing node

#### Scenario: MERGE with ON CREATE
- **WHEN** executing `MERGE (n:Person {email: $email}) ON CREATE SET n.created_at = timestamp() RETURN n`
- **THEN** the `created_at` property is only set when creating a new node

#### Scenario: MERGE with ON MATCH
- **WHEN** executing `MERGE (n:Person {email: $email}) ON MATCH SET n.last_seen = timestamp() RETURN n`
- **THEN** the `last_seen` property is only set when matching an existing node

### Requirement: SET Clause Support
The system SHALL support the SET clause for updating node and relationship properties and labels.

#### Scenario: Update node properties
- **WHEN** executing `MATCH (n:Person {name: 'Alice'}) SET n.age = 31, n.city = 'NYC' RETURN n`
- **THEN** update the node's age and city properties

#### Scenario: Add label to node
- **WHEN** executing `MATCH (n:Person {name: 'Alice'}) SET n:Employee RETURN n`
- **THEN** add the Employee label to the node

#### Scenario: Update with expression
- **WHEN** executing `MATCH (n:Person) SET n.age = n.age + 1 RETURN n`
- **THEN** increment the age property by 1 for all matched nodes

### Requirement: DELETE Clause Support
The system SHALL support the DELETE clause for removing nodes and relationships.

#### Scenario: Delete relationship
- **WHEN** executing `MATCH (a:Person)-[r:KNOWS]->(b:Person) WHERE a.name = 'Alice' DELETE r`
- **THEN** delete the KNOWS relationship between nodes

#### Scenario: Delete node
- **WHEN** executing `MATCH (n:Person {name: 'Alice'}) DELETE n`
- **THEN** delete the node if it has no relationships

#### Scenario: DETACH DELETE node
- **WHEN** executing `MATCH (n:Person {name: 'Alice'}) DETACH DELETE n`
- **THEN** delete the node and all its relationships

### Requirement: REMOVE Clause Support
The system SHALL support the REMOVE clause for removing properties and labels.

#### Scenario: Remove property
- **WHEN** executing `MATCH (n:Person {name: 'Alice'}) REMOVE n.age RETURN n`
- **THEN** remove the age property from the node

#### Scenario: Remove label
- **WHEN** executing `MATCH (n:Person:Employee {name: 'Alice'}) REMOVE n:Employee RETURN n`
- **THEN** remove the Employee label from the node

### Requirement: WITH Clause Support
The system SHALL support the WITH clause for query chaining and intermediate result transformation.

#### Scenario: WITH filtering
- **WHEN** executing `MATCH (n:Person) WITH n WHERE n.age > 18 RETURN n`
- **THEN** filter intermediate results before returning

#### Scenario: WITH aggregation
- **WHEN** executing `MATCH (p:Person)-[:LIKES]->(movie:Movie) WITH movie, COUNT(p) AS likes WHERE likes > 10 RETURN movie`
- **THEN** pre-aggregate before final filtering and return

#### Scenario: WITH projection
- **WHEN** executing `MATCH (n:Person) WITH n.name AS name, n.age AS age WHERE age > 21 RETURN name`
- **THEN** project intermediate results and filter on projected values

### Requirement: OPTIONAL MATCH Support
The system SHALL support OPTIONAL MATCH for left outer join semantics.

#### Scenario: Optional relationship
- **WHEN** executing `MATCH (n:Person) OPTIONAL MATCH (n)-[:WORKS_AT]->(c:Company) RETURN n, c`
- **THEN** return all persons with null company if no relationship exists

#### Scenario: Optional with WHERE
- **WHEN** executing `MATCH (n:Person) OPTIONAL MATCH (n)-[:KNOWS]->(friend) WHERE friend.city = 'NYC' RETURN n, friend`
- **THEN** filter optional matches with WHERE clause

### Requirement: UNWIND Clause Support
The system SHALL support the UNWIND clause for expanding lists into rows.

#### Scenario: Expand list to rows
- **WHEN** executing `UNWIND [1, 2, 3] AS x RETURN x`
- **THEN** return three rows with values 1, 2, and 3

#### Scenario: UNWIND with WHERE
- **WHEN** executing `MATCH (n:Person) UNWIND n.hobbies AS hobby WHERE hobby = 'reading' RETURN n.name, hobby`
- **THEN** expand hobbies list and filter by hobby value

#### Scenario: Batch operations with UNWIND
- **WHEN** executing `UNWIND [{name: 'Alice'}, {name: 'Bob'}] AS p MERGE (n:Person {name: p.name}) RETURN n`
- **THEN** perform batched MERGE operations from list input

### Requirement: UNION/UNION ALL Support
The system SHALL support UNION and UNION ALL for combining query results.

#### Scenario: UNION removes duplicates
- **WHEN** executing `MATCH (n:Person) RETURN n.name AS name UNION MATCH (m:Company) RETURN m.name AS name`
- **THEN** return all unique names from both Person and Company nodes

#### Scenario: UNION ALL keeps duplicates
- **WHEN** executing `MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (m:Person) RETURN m.name AS name`
- **THEN** return all names including duplicates if they exist

### Requirement: CALL Procedures Support
The system SHALL support CALL for invoking built-in and user-defined procedures beyond vector.knn.

#### Scenario: Built-in procedure
- **WHEN** executing `CALL db.labels() YIELD label RETURN label`
- **THEN** return all node labels in the database

#### Scenario: CALL with parameters
- **WHEN** executing `CALL dbms.info() YIELD version RETURN version`
- **THEN** return system information

### Requirement: FOREACH Clause Support
The system SHALL support the FOREACH clause for conditional iteration.

#### Scenario: Iterate and update
- **WHEN** executing `MATCH (n:Person) FOREACH (hobby IN n.hobbies | SET n.verified = true) RETURN n`
- **THEN** update nodes for each element in the list

#### Scenario: Conditional FOREACH
- **WHEN** executing `MATCH (n:Person) FOREACH (x IN CASE WHEN n.age > 18 THEN [1] ELSE [] END | DELETE n)`
- **THEN** perform conditional delete based on age

### Requirement: EXISTS Subquery Support
The system SHALL support EXISTS patterns for existential checks.

#### Scenario: EXISTS in WHERE
- **WHEN** executing `MATCH (n:Person) WHERE EXISTS { MATCH (n)-[:KNOWS]->(:Person {city: 'NYC'}) } RETURN n`
- **THEN** return only persons who know someone in NYC

### Requirement: CASE Expression Support
The system SHALL support CASE expressions for conditional logic.

#### Scenario: Simple CASE
- **WHEN** executing `MATCH (n:Person) RETURN n.name, CASE n.age WHEN 18 THEN 'adult' ELSE 'minor' END AS status`
- **THEN** return 'adult' if age is 18, otherwise 'minor'

#### Scenario: Generic CASE
- **WHEN** executing `MATCH (n:Person) RETURN n.name, CASE WHEN n.age < 18 THEN 'minor' WHEN n.age < 65 THEN 'adult' ELSE 'senior' END AS category`
- **THEN** return age category based on predicates

### Requirement: Map Projection Support
The system SHALL support map projections for object construction.

#### Scenario: Property selection
- **WHEN** executing `MATCH (n:Person) RETURN n {.name, .age} AS person`
- **THEN** return a map with only name and age properties

#### Scenario: Virtual keys
- **WHEN** executing `MATCH (n:Person) RETURN n {.name, fullName: n.firstName + ' ' + n.lastName} AS person`
- **THEN** return a map with existing and computed properties

### Requirement: List Comprehension Support
The system SHALL support list comprehensions for list transformations.

#### Scenario: Filter and transform
- **WHEN** executing `MATCH (n:Person) RETURN [x IN n.scores WHERE x > 80 | x * 1.1] AS bonus`
- **THEN** filter and transform list elements based on conditions

### Requirement: Pattern Comprehension Support
The system SHALL support pattern comprehensions for collecting from graph patterns.

#### Scenario: Collect related nodes
- **WHEN** executing `MATCH (n:Person) RETURN n.name, [(n)-[:KNOWS]->(f:Person) | f.name] AS friends`
- **THEN** return person name and list of friend names

### Requirement: String Operators Support
The system SHALL support STARTS WITH, ENDS WITH, and CONTAINS string operators.

#### Scenario: STARTS WITH
- **WHEN** executing `MATCH (n:Person) WHERE n.name STARTS WITH 'Al' RETURN n`
- **THEN** return persons whose name starts with 'Al'

#### Scenario: ENDS WITH
- **WHEN** executing `MATCH (n:Person) WHERE n.email ENDS WITH '@example.com' RETURN n`
- **THEN** return persons whose email ends with '@example.com'

#### Scenario: CONTAINS
- **WHEN** executing `MATCH (n:Person) WHERE n.bio CONTAINS 'engineer' RETURN n`
- **THEN** return persons whose bio contains 'engineer'

### Requirement: Regular Expression Support
The system SHALL support regular expression matching via =~ operator.

#### Scenario: Pattern matching
- **WHEN** executing `MATCH (n:Person) WHERE n.email =~ '.*@example\.com' RETURN n`
- **THEN** return persons whose email matches the regex pattern

### Requirement: Variable-Length Path Support
The system SHALL support variable-length path quantifiers.

#### Scenario: Fixed length
- **WHEN** executing `MATCH (n:Person)-[:KNOWS*5]->(m:Person) RETURN n, m`
- **THEN** find persons exactly 5 degrees of separation apart

#### Scenario: Range length
- **WHEN** executing `MATCH (n:Person)-[:KNOWS*1..3]->(m:Person) RETURN n, m`
- **THEN** find persons 1 to 3 degrees of separation apart

#### Scenario: Unbounded length
- **WHEN** executing `MATCH (n:Person)-[:KNOWS*]->(m:Person {name: 'Target'}) RETURN n`
- **THEN** find any person who can reach 'Target' through KNOWS relationships

### Requirement: Shortest Path Functions
The system SHALL support shortest path and all shortest paths functions.

#### Scenario: Shortest path
- **WHEN** executing `MATCH p = shortestPath((n:Person)-[*]-(m:Person)) WHERE n.name = 'Alice' AND m.name = 'Bob' RETURN p`
- **THEN** return the shortest path between Alice and Bob

#### Scenario: All shortest paths
- **WHEN** executing `MATCH p = allShortestPaths((n:Person)-[*]-(m:Person)) WHERE n.name = 'Alice' AND m.name = 'Bob' RETURN p`
- **THEN** return all shortest paths between Alice and Bob

### Requirement: Built-in Scalar Functions
The system SHALL support common scalar functions for string, math, temporal, and type conversion operations.

#### Scenario: String functions
- **WHEN** executing `MATCH (n:Person) RETURN n.name, toUpper(n.name), substring(n.name, 0, 3)`
- **THEN** return uppercase name and first 3 characters

#### Scenario: Math functions
- **WHEN** executing `MATCH (n:Person) RETURN n.name, abs(n.salary), round(n.salary)`
- **THEN** return absolute and rounded salary values

#### Scenario: Type conversion
- **WHEN** executing `MATCH (n:Person) RETURN toInteger(n.age), toString(n.age), toFloat(n.weight)`
- **THEN** return converted type values

### Requirement: Additional Aggregation Functions
The system SHALL support COLLECT, percentile, and statistical aggregation functions.

#### Scenario: COLLECT
- **WHEN** executing `MATCH (p:Person)-[:LIKES]->(m:Movie) RETURN m, COLLECT(p.name) AS fans`
- **THEN** return each movie with a list of fan names

#### Scenario: Percentiles
- **WHEN** executing `MATCH (p:Person) RETURN percentileCont(p.age, 0.5) AS median`
- **THEN** return the median age

### Requirement: List and Path Functions
The system SHALL support functions for manipulating lists and paths.

#### Scenario: List functions
- **WHEN** executing `MATCH (n:Person) RETURN size(n.hobbies), head(n.hobbies), tail(n.hobbies)`
- **THEN** return list size, first element, and all but first element

#### Scenario: Path functions
- **WHEN** executing `MATCH path = (n:Person)-[*]->(m:Person) RETURN nodes(path), relationships(path), length(path)`
- **THEN** return nodes in path, relationships in path, and path length

### Requirement: Index Management Support
The system SHALL support CREATE INDEX and DROP INDEX commands.

#### Scenario: Create index
- **WHEN** executing `CREATE INDEX ON :Person(email)`
- **THEN** create an index on Person email property

#### Scenario: Drop index
- **WHEN** executing `DROP INDEX ON :Person(email)`
- **THEN** drop the index on Person email property

### Requirement: Constraint Management Support
The system SHALL support CREATE CONSTRAINT and DROP CONSTRAINT commands.

#### Scenario: Create unique constraint
- **WHEN** executing `CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE`
- **THEN** enforce email uniqueness for Person nodes

#### Scenario: Drop constraint
- **WHEN** executing `DROP CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE`
- **THEN** remove the uniqueness constraint

### Requirement: Transaction Commands
The system SHALL support BEGIN, COMMIT, and ROLLBACK transaction management commands.

#### Scenario: Explicit transaction
- **WHEN** executing `BEGIN MATCH (n:Person) SET n.age = 31 DELETE n COMMIT`
- **THEN** execute operations within an explicit transaction

### Requirement: Database Management Support
The system SHALL support SHOW, CREATE, and DROP DATABASE commands.

#### Scenario: Show databases
- **WHEN** executing `SHOW DATABASES`
- **THEN** return list of all databases

#### Scenario: Create database
- **WHEN** executing `CREATE DATABASE mydb`
- **THEN** create a new database named mydb

### Requirement: User Management Support
The system SHALL support SHOW USERS, CREATE USER, and permission management.

#### Scenario: Show users
- **WHEN** executing `SHOW USERS`
- **THEN** return list of all users

#### Scenario: Create user with permissions
- **WHEN** executing `CREATE USER alice SET PASSWORD 'secret'`
- **THEN** create a new user account

