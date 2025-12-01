---
title: Cypher Clauses Reference
module: cypher
id: cypher-clauses
order: 5
description: All Cypher clauses reference
tags: [cypher, clauses, reference]
---

# Cypher Clauses Reference

Complete reference for all Cypher clauses.

## Reading Clauses

### MATCH

Match patterns in the graph:

```cypher
MATCH (n:Person) RETURN n
```

### OPTIONAL MATCH

Left outer join:

```cypher
MATCH (a:Person)
OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
RETURN a, b
```

## Writing Clauses

### CREATE

Create nodes and relationships:

```cypher
CREATE (n:Person {name: "Alice"})
```

### MERGE

Create or match:

```cypher
MERGE (n:Person {name: "Alice"})
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

## Projection Clauses

### RETURN

Return data:

```cypher
MATCH (n:Person)
RETURN n.name, n.age
```

### WITH

Chain query parts:

```cypher
MATCH (n:Person)
WITH n, n.age * 2 AS double_age
WHERE double_age > 50
RETURN n.name, double_age
```

## Subquery Clauses

### CALL

Call procedures and subqueries:

```cypher
CALL {
  MATCH (n:Person) RETURN n LIMIT 10
}
RETURN n
```

## Uniqueness Clauses

### DISTINCT

Return distinct values:

```cypher
MATCH (n:Person)
RETURN DISTINCT n.city
```

## Reading Hints

### USING INDEX

Use specific index:

```cypher
MATCH (n:Person)
USING INDEX n:Person(name)
WHERE n.name = "Alice"
RETURN n
```

### USING SCAN

Force scan:

```cypher
MATCH (n:Person)
USING SCAN n:Person
RETURN n
```

## Clauses Order

Clauses must appear in this order:

1. MATCH / OPTIONAL MATCH
2. WHERE
3. WITH
4. UNWIND
5. RETURN
6. ORDER BY
7. SKIP
8. LIMIT

## Related Topics

- [Cypher Basics](./BASIC.md) - Basic syntax
- [Advanced Cypher](./ADVANCED.md) - Advanced patterns
- [Complete Cypher Guide](./CYPHER.md) - Comprehensive reference

