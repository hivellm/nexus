---
title: Vector Search
module: vector-search
id: vector-search-index
order: 0
description: Vector similarity search guides and reference
tags: [vector, search, knn, similarity]
---

# Vector Search

Complete guides for vector similarity search in Nexus.

## Guides

### [Basic Vector Search](./BASIC.md)

Simple vector queries:
- Creating nodes with vectors
- Basic KNN search
- Similarity operators
- Vector properties

### [Advanced Vector Search](./ADVANCED.md)

Advanced vector operations:
- Hybrid queries (graph + vector)
- Vector filtering
- Multiple vector spaces
- Performance optimization

### [KNN Operations](./KNN.md)

K-nearest neighbor operations:
- KNN traversal
- Similarity metrics
- HNSW indexes
- Performance tuning

### [Complete Vector Search Guide](./VECTOR_SEARCH.md)

Comprehensive vector search reference:
- All vector operations
- Index management
- Best practices
- Performance tips

## Quick Reference

### Create Node with Vector

```cypher
CREATE (n:Person {
  name: "Alice",
  vector: [0.1, 0.2, 0.3, 0.4]
})
RETURN n
```

### Find Similar Vectors

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 5
```

### Hybrid Query

```cypher
MATCH (a:Person)-[:KNOWS]->(b:Person)
WHERE a.vector IS NOT NULL
  AND b.vector IS NOT NULL
RETURN a.name, b.name, 
       a.vector <-> b.vector as similarity
ORDER BY similarity
LIMIT 10
```

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language
- [API Reference](../api/API_REFERENCE.md) - REST API for vector search
- [Use Cases](../use-cases/) - Real-world examples

