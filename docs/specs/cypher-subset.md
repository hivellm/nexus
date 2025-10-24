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

-- Variable-length path (future V1)
-- MATCH (n:Person)-[:KNOWS*1..3]->(m:Person)

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

-- String operations (V1)
-- WHERE n.name STARTS WITH 'Al'
-- WHERE n.email ENDS WITH '@example.com'
-- WHERE n.bio CONTAINS 'engineer'

-- Pattern matching (V1)
-- WHERE n.email =~ '.*@example\.com'
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

-- Expressions (V1)
-- RETURN n.age + 1 AS next_age
-- RETURN n.price * 1.1 AS price_with_tax
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

-- Collect (V1)
-- RETURN COLLECT(n.name) AS names

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

## Write Operations (V1)

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

-- Delete with DETACH (deletes relationships automatically, V1)
-- MATCH (n:Person {name: 'Alice'})
-- DETACH DELETE n
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

## Unsupported Features (Out of Scope)

### Not in MVP or V1

```cypher
-- Subqueries (future V2)
-- MATCH (n:Person)
-- WHERE EXISTS {
--   MATCH (n)-[:KNOWS]->(:Person {city: 'NYC'})
-- }
-- RETURN n

-- WITH clause (future V1+)
-- MATCH (p:Person)
-- WITH p, COUNT(*) AS connection_count
-- WHERE connection_count > 10
-- RETURN p

-- UNION (future V2)
-- MATCH (n:Person) RETURN n
-- UNION
-- MATCH (n:Company) RETURN n

-- Optional MATCH (future V1+)
-- MATCH (n:Person)
-- OPTIONAL MATCH (n)-[:KNOWS]->(friend)
-- RETURN n, friend

-- Map projections (future V1+)
-- RETURN n {.name, .age, friends: [(n)-[:KNOWS]->(f) | f.name]}

-- List comprehensions (future V2)
-- RETURN [x IN range(1,10) WHERE x % 2 = 0 | x * 2]
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

