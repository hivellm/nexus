# Cypher Subset Specification

This document defines the Cypher query language subset supported by Nexus (MVP and V1).

## Design Philosophy

**20% syntax covering 80% use cases**:
- Focus on read-heavy workloads (MATCH/RETURN)
- Pattern matching with property filters
- KNN-seeded traversal (native procedure)
- Simple aggregations
- No complex subqueries initially

## Supported Syntax (MVP)

### MATCH Clause

```cypher
-- Single node
MATCH (n:Label)

-- Node with multiple labels
MATCH (n:Person:Employee)

-- Relationship pattern
MATCH (n:Person)-[r:KNOWS]->(m:Person)

-- Undirected relationship
MATCH (n:Person)-[r:KNOWS]-(m:Person)

-- Variable-length path ✅ IMPLEMENTED
MATCH (n:Person)-[:KNOWS*1..3]->(m:Person)
MATCH (n:Person)-[:KNOWS*5]->(m:Person)  -- Fixed length
MATCH (n:Person)-[:KNOWS*]->(m:Person)   -- Unbounded
MATCH (n:Person)-[:KNOWS+]->(m:Person)   -- One or more
MATCH (n:Person)-[:KNOWS?]->(m:Person)   -- Zero or one

-- Multiple patterns
MATCH (a:Person)-[:KNOWS]->(b:Person)-[:WORKS_AT]->(c:Company)
```

**Pattern Syntax**:
```
Pattern ::= Node | Relationship | Pattern ',' Pattern

Node ::= '(' Variable? Labels? Properties? ')'

Labels ::= ':' Identifier ( ':' Identifier )*

Relationship ::= 
    '-[' Variable? ':' Type Properties? ']->' |  // directed out
    '<-[' Variable? ':' Type Properties? ']-'  |  // directed in
    '-[' Variable? ':' Type Properties? ']-'      // undirected

Variable ::= Identifier
Type ::= Identifier
```

### WHERE Clause

```cypher
-- Property equality
WHERE n.name = 'Alice'

-- Comparison operators
WHERE n.age > 25
WHERE n.age >= 18
WHERE n.age < 65
WHERE n.age <= 100
WHERE n.age != 0

-- Boolean operators
WHERE n.age > 25 AND n.city = 'NYC'
WHERE n.active = true OR n.premium = true
WHERE NOT n.deleted

-- IN operator
WHERE n.status IN ['active', 'pending']
WHERE n.age IN [18, 21, 65]

-- IS NULL / IS NOT NULL
WHERE n.email IS NOT NULL
WHERE n.deleted_at IS NULL

-- String operations ✅ IMPLEMENTED
WHERE n.name STARTS WITH 'Al'
WHERE n.email ENDS WITH '@example.com'
WHERE n.bio CONTAINS 'engineer'

-- Pattern matching (regex) ✅ IMPLEMENTED
WHERE n.email =~ '.*@example\.com'
```

**Expression Syntax**:
```
Expr ::= Literal 
       | Property
       | Expr BinOp Expr
       | UnOp Expr
       | Expr 'IN' List
       | Expr 'IS' 'NULL'
       | Expr 'IS' 'NOT' 'NULL'

Property ::= Variable '.' Identifier

BinOp ::= '=' | '!=' | '>' | '>=' | '<' | '<=' | 'AND' | 'OR'

UnOp ::= 'NOT' | '-'

Literal ::= Number | String | Boolean | Null

List ::= '[' (Literal (',' Literal)*)? ']'
```

### RETURN Clause

```cypher
-- Return nodes
RETURN n

-- Return properties
RETURN n.name, n.age

-- Return relationships
RETURN r

-- Aliasing
RETURN n.name AS name, n.age AS age

-- Distinct results
RETURN DISTINCT n.city

-- All properties
RETURN n.*

-- Expressions ✅ IMPLEMENTED
RETURN n.age + 1 AS next_age
RETURN n.price * 1.1 AS price_with_tax
RETURN 1 + 1 AS result  -- Literal expressions
```

**Return Syntax**:
```
Return ::= 'RETURN' ('DISTINCT')? ReturnItem (',' ReturnItem)*

ReturnItem ::= Expr ('AS' Identifier)?
```

### ORDER BY Clause

```cypher
-- Single column
ORDER BY n.age

-- Multiple columns
ORDER BY n.city, n.age DESC

-- Ascending (default)
ORDER BY n.name ASC

-- Descending
ORDER BY n.created_at DESC
```

**Order Syntax**:
```
OrderBy ::= 'ORDER' 'BY' OrderItem (',' OrderItem)*

OrderItem ::= Expr ('ASC' | 'DESC')?
```

### LIMIT Clause

```cypher
-- Limit results
LIMIT 10

-- Combined with ORDER BY
ORDER BY n.score DESC LIMIT 100
```

**Limit Syntax**:
```
Limit ::= 'LIMIT' Number
```

### SKIP Clause (V1)

```cypher
-- Skip first N results
SKIP 10

-- Pagination
SKIP 20 LIMIT 10  -- page 3, size 10
```

### Aggregations

```cypher
-- Count
RETURN COUNT(*)
RETURN COUNT(n)
RETURN COUNT(DISTINCT n.city)

-- Sum
RETURN SUM(n.age)

-- Average
RETURN AVG(n.age)

-- Min/Max
RETURN MIN(n.age), MAX(n.age)

-- Collect ✅ IMPLEMENTED
RETURN COLLECT(n.name) AS names
RETURN COLLECT(DISTINCT n.city) AS cities

-- Group by
MATCH (p:Person)-[:LIKES]->(prod:Product)
RETURN prod.category, COUNT(*) AS likes
ORDER BY likes DESC
```

**Aggregation Syntax**:
```
AggFunc ::= 'COUNT' '(' ('DISTINCT')? Expr ')'
          | 'SUM' '(' Expr ')'
          | 'AVG' '(' Expr ')'
          | 'MIN' '(' Expr ')'
          | 'MAX' '(' Expr ')'
          | 'COLLECT' '(' Expr ')'  // V1
```

## KNN Procedures (MVP)

### vector.knn

```cypher
-- Basic KNN search
CALL vector.knn('Person', $embedding, 10)
YIELD node, score
RETURN node.name, score
ORDER BY score DESC

-- KNN with filters
CALL vector.knn('Person', $embedding, 10)
YIELD node, score
WHERE node.active = true
RETURN node.name, score

-- KNN-seeded traversal
CALL vector.knn('Person', $embedding, 10)
YIELD node AS n
MATCH (n)-[:WORKS_AT]->(c:Company)
RETURN n.name, c.name, score
```

**Signature**:
```
vector.knn(
  label: String,        // Node label to search
  vector: List<Float>,  // Query embedding
  k: Integer            // Number of neighbors
) YIELDS (
  node: Node,          // Matched node
  score: Float         // Similarity score (0.0-1.0)
)
```

## Full-Text Procedures (V1)

### text.search

```cypher
-- Full-text search
CALL text.search('Person', 'bio', 'software engineer', 10)
YIELD node, score
RETURN node.name, node.bio, score
ORDER BY score DESC
```

**Signature**:
```
text.search(
  label: String,      // Node label
  key: String,        // Property key
  query: String,      // Search query
  limit: Integer      // Max results
) YIELDS (
  node: Node,        // Matched node
  score: Float       // Relevance score (BM25)
)
```

## Write Operations ✅ IMPLEMENTED

### CREATE

```cypher
-- Create node
CREATE (n:Person {name: 'Alice', age: 30})
RETURN n

-- Create relationship
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
CREATE (a)-[r:KNOWS {since: 2020}]->(b)
RETURN r

-- Create multiple
CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
CREATE (a)-[:KNOWS]->(b)
```

### SET

```cypher
-- Set property
MATCH (n:Person {name: 'Alice'})
SET n.age = 31
RETURN n

-- Set multiple properties
MATCH (n:Person {name: 'Alice'})
SET n.age = 31, n.city = 'NYC'

-- Add label
MATCH (n:Person {name: 'Alice'})
SET n:Employee
```

### DELETE

```cypher
-- Delete node (must delete relationships first)
MATCH (n:Person {name: 'Alice'})
DELETE n

-- Delete relationship
MATCH (a:Person)-[r:KNOWS]->(b:Person)
WHERE a.name = 'Alice'
DELETE r

-- Delete with DETACH (deletes relationships automatically) ✅ IMPLEMENTED
MATCH (n:Person {name: 'Alice'})
DETACH DELETE n
```

### REMOVE

```cypher
-- Remove property
MATCH (n:Person {name: 'Alice'})
REMOVE n.age

-- Remove label
MATCH (n:Person:Employee {name: 'Alice'})
REMOVE n:Employee
```

## Query Examples

### Example 1: Social Network

```cypher
-- Find friends of friends
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)-[:KNOWS]->(fof)
WHERE fof <> me AND NOT (me)-[:KNOWS]->(fof)
RETURN DISTINCT fof.name
LIMIT 10
```

### Example 2: Recommendation

```cypher
-- Recommend products based on what friends bought
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)
MATCH (friend)-[:BOUGHT]->(product:Product)
WHERE NOT (me)-[:BOUGHT]->(product)
RETURN product.name, COUNT(*) AS friend_count
ORDER BY friend_count DESC
LIMIT 5
```

### Example 3: KNN + Graph

```cypher
-- Find similar people and their companies
CALL vector.knn('Person', $my_embedding, 20)
YIELD node AS similar, score
WHERE similar.age > 25
MATCH (similar)-[:WORKS_AT]->(company:Company)
RETURN similar.name, company.name, score
ORDER BY score DESC
LIMIT 10
```

### Example 4: Aggregation

```cypher
-- Top product categories by sales
MATCH (person:Person)-[:BOUGHT]->(product:Product)
RETURN product.category, COUNT(*) AS purchases, AVG(product.price) AS avg_price
ORDER BY purchases DESC
LIMIT 10
```

## Advanced Query Features ✅ IMPLEMENTED

### MERGE Clause

```cypher
-- Match or create
MERGE (n:Person {email: 'alice@example.com'})
ON CREATE SET n.created = true
ON MATCH SET n.last_seen = datetime()
RETURN n
```

### WITH Clause

```cypher
-- Query piping
MATCH (p:Person)
WITH p, COUNT(*) AS connection_count
WHERE connection_count > 10
RETURN p

-- Pre-aggregation
MATCH (n:Person)
WITH n.city AS city, COUNT(*) AS count
WHERE count > 5
RETURN city, count
```

### OPTIONAL MATCH

```cypher
-- Left outer join
MATCH (n:Person)
OPTIONAL MATCH (n)-[:KNOWS]->(friend)
RETURN n, friend
```

### UNWIND Clause

```cypher
-- List expansion
UNWIND [1, 2, 3] AS num
RETURN num * 2 AS doubled

-- With WHERE filtering
UNWIND $names AS name
WHERE name STARTS WITH 'A'
RETURN name
```

### UNION / UNION ALL

```cypher
-- Union (distinct)
MATCH (n:Person) RETURN n
UNION
MATCH (n:Company) RETURN n

-- Union all (keep duplicates)
MATCH (n:Person) RETURN n
UNION ALL
MATCH (n:Company) RETURN n
```

### FOREACH Clause

```cypher
-- Iterate and update
MATCH (n:Person)
FOREACH (x IN [1, 2, 3] |
  CREATE (n)-[:TAG {value: x}]->(:Tag)
)
```

### EXISTS Subqueries

```cypher
-- Existential pattern check
MATCH (n:Person)
WHERE EXISTS {
  MATCH (n)-[:KNOWS]->(:Person {city: 'NYC'})
}
RETURN n
```

### CASE Expressions

```cypher
-- Simple CASE
RETURN CASE n.status
  WHEN 'active' THEN 'Online'
  WHEN 'inactive' THEN 'Offline'
  ELSE 'Unknown'
END AS status_text

-- Generic CASE
RETURN CASE
  WHEN n.age < 18 THEN 'Minor'
  WHEN n.age < 65 THEN 'Adult'
  ELSE 'Senior'
END AS age_group
```

### Comprehensions

```cypher
-- List comprehension
RETURN [x IN range(1,10) WHERE x % 2 = 0 | x * 2] AS evens

-- Pattern comprehension
RETURN [(n)-[:KNOWS]->(f) | f.name] AS friends

-- Map projection
RETURN n {.name, .age, friends: [(n)-[:KNOWS]->(f) | f.name]}
```

## Schema Management ✅ IMPLEMENTED

### Index Management

```cypher
-- Create index
CREATE INDEX ON :Person(email)

-- Create index if not exists
CREATE INDEX IF NOT EXISTS ON :Person(name)

-- Create or replace index
CREATE OR REPLACE INDEX ON :Person(age)

-- Create spatial index
CREATE SPATIAL INDEX ON :Location(coords)

-- Drop index
DROP INDEX ON :Person(email)

-- Drop index if exists
DROP INDEX IF EXISTS ON :Person(name)
```

### Constraint Management

```cypher
-- Unique constraint
CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE

-- Exists constraint
CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS(n.email)

-- Drop constraint
DROP CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE

-- Drop constraint if exists
DROP CONSTRAINT IF EXISTS ON (n:Person) ASSERT EXISTS(n.email)
```

### Database Management

```cypher
-- Show databases
SHOW DATABASES

-- Create database
CREATE DATABASE mydb

-- Create database if not exists
CREATE DATABASE IF NOT EXISTS mydb

-- Use database
USE DATABASE mydb

-- Drop database
DROP DATABASE mydb

-- Drop database if exists
DROP DATABASE IF EXISTS mydb
```

### Function Management

```cypher
-- Create function signature
CREATE FUNCTION multiply(a: Integer, b: Integer) RETURNS Integer AS 'Multiply two integers'

-- Create function if not exists
CREATE FUNCTION IF NOT EXISTS add(a: Integer, b: Integer) RETURNS Integer

-- Show functions
SHOW FUNCTIONS

-- Drop function
DROP FUNCTION multiply

-- Drop function if exists
DROP FUNCTION IF EXISTS multiply
```

## Transaction Commands ✅ IMPLEMENTED

```cypher
-- Begin transaction
BEGIN

-- Commit transaction
COMMIT

-- Rollback transaction
ROLLBACK
```

## Query Analysis ✅ IMPLEMENTED

### EXPLAIN

```cypher
-- Query plan analysis
EXPLAIN MATCH (n:Person) RETURN n
```

### PROFILE

```cypher
-- Execution profiling
PROFILE MATCH (n:Person) RETURN n
```

### Query Hints

```cypher
-- Force index usage
MATCH (n:Person)
USING INDEX n:Person(email)
WHERE n.email = 'alice@example.com'
RETURN n

-- Force label scan
MATCH (n:Person)
USING SCAN n:Person
WHERE n.age > 25
RETURN n

-- Force join strategy
MATCH (a)-[r]->(b)
USING JOIN ON r
RETURN a, b
```

## Data Import/Export ✅ IMPLEMENTED

### LOAD CSV

```cypher
-- Load CSV file
LOAD CSV FROM 'file:///path/to/data.csv' AS row
CREATE (n:Person {name: row[0], age: toInteger(row[1])})

-- With headers
LOAD CSV FROM 'file:///path/to/data.csv' WITH HEADERS AS row
CREATE (n:Person {name: row.name, age: toInteger(row.age)})
```

## Advanced Features ✅ IMPLEMENTED

### Subqueries

```cypher
-- CALL subquery
CALL {
  MATCH (n:Person) RETURN n
}

-- CALL subquery in transactions
CALL {
  MATCH (n:Person) DELETE n
} IN TRANSACTIONS OF 1000 ROWS
```

### Named Paths

```cypher
-- Path variable assignment
MATCH p = (a:Person)-[*]-(b:Person)
RETURN p, nodes(p), relationships(p), length(p)
```

### Shortest Path Functions

```cypher
-- Shortest path
RETURN shortestPath((a:Person {name: 'Alice'})-[*]-(b:Person {name: 'Bob'}))

-- All shortest paths
RETURN allShortestPaths((a:Person)-[*]-(b:Person))
```

## Built-in Functions ✅ IMPLEMENTED

### String Functions

```cypher
-- Basic string operations
RETURN toLower('HELLO') AS lower
RETURN toUpper('hello') AS upper
RETURN substring('Hello World', 0, 5) AS substr
RETURN trim('  hello  ') AS trimmed
RETURN ltrim('  hello') AS left_trimmed
RETURN rtrim('hello  ') AS right_trimmed
RETURN replace('hello world', 'world', 'nexus') AS replaced
RETURN split('a,b,c', ',') AS parts
RETURN length('hello') AS len
```

### Math Functions

```cypher
-- Basic math
RETURN abs(-5) AS absolute
RETURN ceil(4.3) AS ceiling
RETURN floor(4.7) AS floor
RETURN round(4.5) AS rounded
RETURN sqrt(16) AS square_root
RETURN pow(2, 3) AS power

-- Trigonometric
RETURN sin(0) AS sine
RETURN cos(0) AS cosine
RETURN tan(0) AS tangent
```

### Type Conversion

```cypher
-- Type conversions
RETURN toInteger('123') AS int_val
RETURN toFloat('3.14') AS float_val
RETURN toString(123) AS str_val
RETURN toBoolean('true') AS bool_val
RETURN toDate('2024-01-01') AS date_val
```

### Temporal Functions

```cypher
-- Date/time functions
RETURN date() AS today
RETURN datetime() AS now
RETURN time() AS current_time
RETURN timestamp() AS unix_timestamp
RETURN duration({days: 7}) AS week
```

### List Functions

```cypher
-- List operations
RETURN size([1, 2, 3]) AS list_size
RETURN head([1, 2, 3]) AS first
RETURN tail([1, 2, 3]) AS rest
RETURN last([1, 2, 3]) AS last_item
RETURN range(1, 10) AS numbers
RETURN reverse([1, 2, 3]) AS reversed
RETURN reduce(acc = 0, x IN [1, 2, 3] | acc + x) AS sum
RETURN [x IN [1, 2, 3] | x * 2] AS doubled
```

### Path Functions

```cypher
-- Path operations
MATCH p = (a)-[*]-(b)
RETURN nodes(p) AS path_nodes
RETURN relationships(p) AS path_rels
RETURN length(p) AS path_length
```

### Predicate Functions

```cypher
-- Predicates
RETURN all(x IN [1, 2, 3] WHERE x > 0) AS all_positive
RETURN any(x IN [1, 2, 3] WHERE x > 2) AS any_greater
RETURN none(x IN [1, 2, 3] WHERE x < 0) AS none_negative
RETURN single(x IN [1, 2, 3] WHERE x = 2) AS single_match
```

### Additional Aggregations

```cypher
-- Advanced aggregations
RETURN percentileDisc(n.age, 0.5) AS median
RETURN percentileCont(n.age, 0.5) AS median_cont
RETURN stDev(n.age) AS std_dev
RETURN stDevP(n.age) AS std_dev_pop
```

## Geospatial Features ✅ IMPLEMENTED

### Point Data Type

```cypher
-- Create point
RETURN point({x: 1, y: 2}) AS p

-- 3D point
RETURN point({x: 1, y: 2, z: 3}) AS p3d

-- WGS84 point
RETURN point({x: -122.4194, y: 37.7749, crs: 'wgs-84'}) AS location
```

### Distance Functions

```cypher
-- Calculate distance
MATCH (l:Location)
RETURN distance(l.coords, point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})) AS dist
ORDER BY dist LIMIT 5
```

### Geospatial Procedures

```cypher
-- Within bounding box
CALL spatial.withinBBox('Location', 'coords', point({x: -122.5, y: 37.7}), point({x: -122.3, y: 37.8}))
YIELD node
RETURN node

-- Within distance
CALL spatial.withinDistance('Location', 'coords', point({x: -122.4194, y: 37.7749, crs: 'wgs-84'}), 1000.0)
YIELD node
RETURN node
```

## Graph Algorithms ✅ IMPLEMENTED

### Pathfinding

```cypher
-- Shortest path
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
RETURN shortestPath((a)-[*]-(b)) AS path

-- All shortest paths
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
RETURN allShortestPaths((a)-[*]-(b)) AS paths
```

### Centrality

```cypher
-- PageRank
CALL algorithms.pageRank('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC LIMIT 10
```

### Community Detection

```cypher
-- Label propagation
CALL algorithms.labelPropagation('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members
```

## Unsupported Features (Out of Scope)

### Not Currently Implemented

```cypher
-- Complex subqueries with multiple statements (future)
-- CALL {
--   MATCH (n:Person) RETURN n
--   UNION
--   MATCH (n:Company) RETURN n
-- }

-- Full-text search syntax (use procedures instead)
-- MATCH (n:Person) WHERE n.bio CONTAINS TEXT 'engineer'
```

## Query Optimization Hints

### Planner Hints (V1, optional)

```cypher
-- Force index usage
MATCH (n:Person)
USING INDEX n:Person(email)
WHERE n.email = 'alice@example.com'
RETURN n

-- Force label scan
MATCH (n:Person)
USING SCAN n:Person
WHERE n.age > 25
RETURN n
```

## Error Handling

### Syntax Errors

```cypher
-- Missing RETURN
MATCH (n:Person)
-- Error: Expected RETURN clause

-- Invalid pattern
MATCH (n:Person)-->(m)
-- Error: Relationship must have type

-- Type mismatch
WHERE n.age = 'thirty'
-- Error: Type mismatch: expected Integer, got String
```

### Runtime Errors

```cypher
-- Node not found (returns empty result, not error)
MATCH (n:Person {name: 'NonExistent'})
RETURN n
-- Result: (empty)

-- Division by zero (V1)
-- RETURN 1 / 0
-- Error: Division by zero

-- Property access on null
RETURN null.name
-- Error: Cannot access property on null
```

## Performance Characteristics

| Query Pattern | Complexity | Notes |
|---------------|------------|-------|
| `MATCH (n:Label)` | O(\|V_label\|) | Label bitmap scan |
| `MATCH (n:Label {id: $id})` | O(1) | With index on `id` |
| `MATCH (n)-[r]->(m)` | O(\|V\| × avg_degree) | Expand neighbors |
| `MATCH (n)-[:TYPE*1..3]->(m)` | O(\|V\| × avg_degree^3) | Variable-length path |
| `ORDER BY ... LIMIT k` | O(n log k) | Top-K heap |
| `COUNT(*)` | O(n) | Full scan or index count |
| `vector.knn(...)` | O(log n) | HNSW logarithmic |

## Parser Grammar (EBNF)

```ebnf
Query ::= MatchClause WhereClause? ReturnClause OrderByClause? LimitClause? SkipClause?
        | CallClause YieldClause? WhereClause? ReturnClause?

MatchClause ::= 'MATCH' Pattern (',' Pattern)*

Pattern ::= NodePattern (RelationshipPattern NodePattern)*

NodePattern ::= '(' Variable? LabelExpression? PropertyMap? ')'

LabelExpression ::= (':' Identifier)+

RelationshipPattern ::= 
    '-[' Variable? ':' Type PropertyMap? ']->' |
    '<-[' Variable? ':' Type PropertyMap? ']-' |
    '-[' Variable? ':' Type PropertyMap? ']-'

PropertyMap ::= '{' (Property (',' Property)*)? '}'

Property ::= Identifier ':' Expression

WhereClause ::= 'WHERE' Expression

ReturnClause ::= 'RETURN' ('DISTINCT')? ReturnItem (',' ReturnItem)*

ReturnItem ::= Expression ('AS' Identifier)?

OrderByClause ::= 'ORDER' 'BY' OrderItem (',' OrderItem)*

OrderItem ::= Expression ('ASC' | 'DESC')?

LimitClause ::= 'LIMIT' Integer

SkipClause ::= 'SKIP' Integer

CallClause ::= 'CALL' ProcedureName '(' (Expression (',' Expression)*)? ')'

YieldClause ::= 'YIELD' YieldItem (',' YieldItem)*

YieldItem ::= Identifier ('AS' Identifier)?

Expression ::= /* standard expression grammar */
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_parse_simple_match() {
    let query = "MATCH (n:Person) RETURN n";
    let ast = parse(query).unwrap();
    assert_eq!(ast.match_patterns.len(), 1);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_execute_match_return() {
    let engine = Engine::new().unwrap();
    // Insert test data
    let result = engine.execute("MATCH (n:Person) RETURN n LIMIT 10").await.unwrap();
    assert_eq!(result.rows.len(), 10);
}
```

### Compliance Tests

TPC-like graph query suite (future V1):
- Social network queries (friends, recommendations)
- E-commerce queries (products, orders)
- Knowledge graph queries (entities, relationships)

## Future Extensions

### V2: Advanced Cypher

- WITH clause (query piping)
- OPTIONAL MATCH (left join semantics)
- UNION / UNION ALL
- Subqueries in WHERE (EXISTS, ANY, ALL)
- Map and list operators

### V3: Graph Algorithms

- Shortest path: `shortestPath((n)-[*]-(m))`
- All paths: `allShortestPaths((n)-[*]-(m))`
- Graph algorithms: PageRank, community detection, centrality

## References

- OpenCypher: https://opencypher.org/
- Neo4j Cypher Manual: https://neo4j.com/docs/cypher-manual/current/
- Cypher Style Guide: https://neo4j.com/developer/cypher-style-guide/

