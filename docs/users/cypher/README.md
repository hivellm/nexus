---
title: Cypher Query Language
module: cypher
id: cypher-index
order: 0
description: Cypher query language guides and reference
tags: [cypher, query, language, reference]
---

# Cypher Query Language

Complete guides for the Cypher query language in Nexus.

## Guides

### [Cypher Basics](./BASIC.md)

Basic Cypher syntax and patterns:
- Node patterns
- Relationship patterns
- Basic queries
- Filtering and sorting
- Aggregations

### [Advanced Cypher](./ADVANCED.md)

Advanced queries and patterns:
- Variable-length paths
- OPTIONAL MATCH
- Complex patterns
- Subqueries
- List and pattern comprehensions
- Map projections

### [Functions Reference](./FUNCTIONS.md)

All Cypher functions:
- String functions
- Mathematical functions
- Temporal functions
- List functions
- Aggregation functions

### [Complete Cypher Guide](./CYPHER.md)

Comprehensive Cypher reference:
- All clauses
- All functions
- All operators
- Best practices
- Performance tips

## Quick Reference

### Basic Patterns

```cypher
// Match nodes
MATCH (n:Person) RETURN n

// Match relationships
MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a, b

// Filter
MATCH (n:Person) WHERE n.age > 25 RETURN n

// Create
CREATE (n:Person {name: "Alice", age: 30})
```

### Common Operations

```cypher
// Count
MATCH (n:Person) RETURN COUNT(n)

// Group by
MATCH (n:Person) RETURN n.city, COUNT(n) AS count

// Order by
MATCH (n:Person) RETURN n ORDER BY n.age DESC

// Limit
MATCH (n:Person) RETURN n LIMIT 10
```

## Related Topics

- [API Reference](../api/API_REFERENCE.md) - REST API for Cypher
- [Vector Search](../vector-search/) - Vector similarity search
- [Use Cases](../use-cases/) - Real-world examples

