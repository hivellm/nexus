---
title: Cypher Basics
module: cypher
id: cypher-basics
order: 1
description: Basic Cypher syntax and patterns
tags: [cypher, basics, query, tutorial]
---

# Cypher Basics

Learn the fundamentals of the Cypher query language.

## Node Patterns

### Match Nodes

```cypher
// Match all nodes
MATCH (n) RETURN n LIMIT 10

// Match nodes with label
MATCH (n:Person) RETURN n

// Match nodes with multiple labels
MATCH (n:Person:Employee) RETURN n
```

### Match with Properties

```cypher
// Match by property
MATCH (n:Person {name: "Alice"}) RETURN n

// Match with multiple properties
MATCH (n:Person {name: "Alice", age: 30}) RETURN n
```

## Relationship Patterns

### Direct Relationships

```cypher
// Directed relationship
MATCH (a:Person)-[:KNOWS]->(b:Person) 
RETURN a.name, b.name

// Undirected relationship
MATCH (a:Person)-[:KNOWS]-(b:Person) 
RETURN a.name, b.name

// Relationship with variable
MATCH (a:Person)-[r:KNOWS]->(b:Person) 
RETURN a.name, b.name, r.since
```

### Multiple Relationships

```cypher
MATCH (a:Person)-[:KNOWS]->(b:Person)-[:WORKS_AT]->(c:Company)
RETURN a.name, b.name, c.name
```

## Filtering

### WHERE Clause

```cypher
// Property comparison
MATCH (n:Person) WHERE n.age > 25 RETURN n

// Multiple conditions
MATCH (n:Person) 
WHERE n.age > 25 AND n.city = 'NYC' 
RETURN n

// IN operator
MATCH (n:Person) 
WHERE n.status IN ['active', 'pending'] 
RETURN n

// IS NULL / IS NOT NULL
MATCH (n:Person) 
WHERE n.email IS NOT NULL 
RETURN n
```

### String Operations

```cypher
// STARTS WITH
MATCH (n:Person) WHERE n.name STARTS WITH 'Al' RETURN n

// ENDS WITH
MATCH (n:Person) WHERE n.email ENDS WITH '@example.com' RETURN n

// CONTAINS
MATCH (n:Person) WHERE n.bio CONTAINS 'engineer' RETURN n

// Regex
MATCH (n:Person) WHERE n.email =~ '.*@example\.com' RETURN n
```

## Returning Data

### RETURN Clause

```cypher
// Return nodes
RETURN n

// Return properties
RETURN n.name, n.age

// Return with aliases
RETURN n.name AS name, n.age AS age

// Return distinct
RETURN DISTINCT n.city

// Return all properties
RETURN n.*
```

## Sorting and Limiting

### ORDER BY

```cypher
// Single column
MATCH (n:Person) RETURN n ORDER BY n.age

// Multiple columns
MATCH (n:Person) RETURN n ORDER BY n.city, n.age DESC

// Ascending (default)
MATCH (n:Person) RETURN n ORDER BY n.name ASC

// Descending
MATCH (n:Person) RETURN n ORDER BY n.age DESC
```

### LIMIT and SKIP

```cypher
// Limit results
MATCH (n:Person) RETURN n LIMIT 10

// Skip results
MATCH (n:Person) RETURN n SKIP 20 LIMIT 10

// Pagination
MATCH (n:Person) RETURN n SKIP 0 LIMIT 10  // Page 1
MATCH (n:Person) RETURN n SKIP 10 LIMIT 10 // Page 2
```

## Aggregations

### Basic Aggregations

```cypher
// Count
MATCH (n:Person) RETURN COUNT(n) AS total

// Average
MATCH (n:Person) RETURN AVG(n.age) AS avg_age

// Sum
MATCH (n:Person) RETURN SUM(n.age) AS total_age

// Min/Max
MATCH (n:Person) RETURN MIN(n.age) AS min_age, MAX(n.age) AS max_age
```

### Group By

```cypher
MATCH (n:Person) 
RETURN n.city, COUNT(n) AS count 
ORDER BY count DESC
```

### Collect

```cypher
MATCH (n:Person)-[:KNOWS]->(m:Person)
RETURN n.name, COLLECT(m.name) AS friends
```

## Creating Data

### CREATE

```cypher
// Create node
CREATE (n:Person {name: "Alice", age: 30})

// Create multiple nodes
CREATE 
  (alice:Person {name: "Alice", age: 30}),
  (bob:Person {name: "Bob", age: 28})

// Create relationship
MATCH (a:Person {name: "Alice"}), (b:Person {name: "Bob"})
CREATE (a)-[:KNOWS {since: "2020"}]->(b)
```

### MERGE

```cypher
// Create or match
MERGE (n:Person {name: "Alice"})
ON CREATE SET n.created_at = timestamp()
ON MATCH SET n.last_seen = timestamp()
RETURN n
```

## Updating Data

### SET

```cypher
// Set properties
MATCH (n:Person {name: "Alice"})
SET n.age = 31, n.updated_at = timestamp()
RETURN n

// Set labels
MATCH (n:Person {name: "Alice"})
SET n:Employee:Manager
RETURN n
```

## Deleting Data

### DELETE

```cypher
// Delete node (requires no relationships)
MATCH (n:Person {name: "Alice"})
DELETE n

// Delete with relationships
MATCH (n:Person {name: "Alice"})
DETACH DELETE n

// Delete relationship
MATCH (a:Person)-[r:KNOWS]->(b:Person)
WHERE a.name = "Alice" AND b.name = "Bob"
DELETE r
```

### REMOVE

```cypher
// Remove property
MATCH (n:Person {name: "Alice"})
REMOVE n.temp_property

// Remove label
MATCH (n:Person {name: "Alice"})
REMOVE n:Employee
```

## Next Steps

- [Advanced Cypher](./ADVANCED.md) - Advanced patterns and queries
- [Functions Reference](./FUNCTIONS.md) - All Cypher functions
- [Complete Cypher Guide](./CYPHER.md) - Comprehensive reference

## Related Topics

- [API Reference](../api/API_REFERENCE.md) - Execute Cypher via API
- [Use Cases](../use-cases/) - Real-world examples

