---
title: Complete Cypher Guide
module: cypher
id: complete-cypher-guide
order: 4
description: Comprehensive Cypher reference
tags: [cypher, reference, complete, guide]
---

# Complete Cypher Guide

Comprehensive reference for the Cypher query language in Nexus.

## Table of Contents

1. [Clauses](#clauses)
2. [Operators](#operators)
3. [Functions](#functions)
4. [Data Types](#data-types)
5. [Patterns](#patterns)
6. [Best Practices](#best-practices)

## Clauses

### MATCH

Match patterns in the graph:

```cypher
// Match all nodes
MATCH (n) RETURN n

// Match with label
MATCH (n:Person) RETURN n

// Match relationships
MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a, b
```

### OPTIONAL MATCH

Left outer join semantics:

```cypher
MATCH (a:Person)
OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
RETURN a.name, b.name
```

### WHERE

Filter results:

```cypher
MATCH (n:Person)
WHERE n.age > 25 AND n.city = 'NYC'
RETURN n
```

### RETURN

Return data:

```cypher
MATCH (n:Person)
RETURN n.name, n.age
```

### CREATE

Create nodes and relationships:

```cypher
CREATE (n:Person {name: "Alice", age: 30})
```

### MERGE

Create or match:

```cypher
MERGE (n:Person {name: "Alice"})
ON CREATE SET n.created_at = timestamp()
ON MATCH SET n.last_seen = timestamp()
```

### SET

Update properties:

```cypher
MATCH (n:Person {name: "Alice"})
SET n.age = 31
```

### DELETE

Delete nodes and relationships:

```cypher
MATCH (n:Person {name: "Alice"})
DELETE n
```

### REMOVE

Remove properties and labels:

```cypher
MATCH (n:Person {name: "Alice"})
REMOVE n.temp_property
```

### WITH

Chain query parts:

```cypher
MATCH (n:Person)
WITH n, n.age * 2 AS double_age
WHERE double_age > 50
RETURN n.name, double_age
```

### UNWIND

Expand lists:

```cypher
UNWIND [1, 2, 3] AS number
RETURN number
```

### UNION

Combine results:

```cypher
MATCH (n:Person) RETURN n.name AS name
UNION
MATCH (n:Company) RETURN n.name AS name
```

### FOREACH

Iterate and modify:

```cypher
MATCH (n:Person)
FOREACH (x IN [1, 2, 3] |
  CREATE (n)-[:HAS_TAG]->(:Tag {id: x})
)
```

### CALL

Call procedures and subqueries:

```cypher
CALL {
  MATCH (n:Person) RETURN n LIMIT 10
}
RETURN n
```

### ORDER BY

Sort results:

```cypher
MATCH (n:Person)
RETURN n
ORDER BY n.age DESC
```

### LIMIT

Limit results:

```cypher
MATCH (n:Person)
RETURN n
LIMIT 10
```

### SKIP

Skip results:

```cypher
MATCH (n:Person)
RETURN n
SKIP 20 LIMIT 10
```

## Operators

### Comparison Operators

- `=` - Equality
- `!=` - Inequality
- `>` - Greater than
- `>=` - Greater than or equal
- `<` - Less than
- `<=` - Less than or equal

### Boolean Operators

- `AND` - Logical AND
- `OR` - Logical OR
- `NOT` - Logical NOT
- `XOR` - Exclusive OR

### String Operators

- `STARTS WITH` - String starts with
- `ENDS WITH` - String ends with
- `CONTAINS` - String contains
- `=~` - Regex match

### List Operators

- `IN` - Membership test
- `[]` - List indexing
- `+` - List concatenation

### Vector Operators

- `<->` - Cosine distance (lower is more similar)

### Null Operators

- `IS NULL` - Check for null
- `IS NOT NULL` - Check for not null

## Functions

See [Functions Reference](./FUNCTIONS.md) for complete function list.

## Data Types

### Primitives

- **Integer**: `42`
- **Float**: `3.14`
- **String**: `"hello"`
- **Boolean**: `true`, `false`
- **Null**: `null`

### Collections

- **List**: `[1, 2, 3]`
- **Map**: `{name: "Alice", age: 30}`

### Temporal

- **Date**: `date()`
- **DateTime**: `datetime()`
- **Time**: `time()`
- **Duration**: `duration({days: 1})`

### Geospatial

- **Point**: `point({x: 1, y: 2, crs: 'cartesian'})`

## Patterns

### Node Patterns

```cypher
// Basic node
(n)

// With label
(n:Person)

// With properties
(n:Person {name: "Alice", age: 30})
```

### Relationship Patterns

```cypher
// Directed
(a)-[:KNOWS]->(b)

// Undirected
(a)-[:KNOWS]-(b)

// Variable length
(a)-[:KNOWS*2..5]->(b)
```

## Best Practices

1. **Use Indexes**: Create indexes on frequently queried properties
2. **Use LIMIT**: Always limit result sets
3. **Filter Early**: Use WHERE as early as possible
4. **Avoid Cartesian Products**: Be careful with multiple MATCH clauses
5. **Use EXPLAIN**: Analyze query plans
6. **Normalize Vectors**: For better similarity results

## Related Topics

- [Cypher Basics](./BASIC.md) - Basic syntax
- [Advanced Cypher](./ADVANCED.md) - Advanced patterns
- [Functions Reference](./FUNCTIONS.md) - All functions

