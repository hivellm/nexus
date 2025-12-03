---
title: Advanced Vector Search
module: vector-search
id: advanced-vector-search
order: 2
description: Hybrid queries and optimization
tags: [vector, search, advanced, hybrid, optimization]
---

# Advanced Vector Search

Advanced vector search patterns and optimization techniques.

## Hybrid Queries

### Graph Traversal + Vector Search

```cypher
// Find similar people who are connected
MATCH (a:Person)-[:KNOWS]->(b:Person)
WHERE a.vector IS NOT NULL
  AND b.vector IS NOT NULL
RETURN a.name, b.name, 
       a.vector <-> b.vector AS similarity
ORDER BY similarity
LIMIT 10
```

### Multi-Hop Vector Search

```cypher
MATCH (user:Person {id: 1})-[:LIKES]->(item:Item),
      (similar:Person)-[:LIKES]->(item),
      (similar)-[:LIKES]->(recommended:Item)
WHERE recommended.vector IS NOT NULL
  AND user.vector IS NOT NULL
RETURN recommended.title,
       COUNT(similar) AS graph_score,
       user.vector <-> recommended.vector AS vector_score
ORDER BY graph_score DESC, vector_score
LIMIT 10
```

## Vector Filtering

### Multiple Vector Spaces

```cypher
// Search in specific vector space
MATCH (n:Person)
WHERE n.embedding_vector IS NOT NULL
RETURN n.name
ORDER BY n.embedding_vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 10
```

### Vector + Property Filters

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.vector <-> [0.1, 0.2, 0.3, 0.4] < 0.5
  AND n.age > 25
  AND n.city = 'NYC'
RETURN n.name, n.vector
```

## Performance Optimization

### Batch Vector Operations

```cypher
// Create multiple nodes with vectors
UNWIND $vectors AS vec
CREATE (n:Person {
  name: vec.name,
  vector: vec.vector
})
```

### Index Optimization

```yaml
# Optimize HNSW index
indexes:
  vector:
    hnsw:
      m: 32  # Increase for better recall
      ef_construction: 400  # Increase for better quality
      ef_search: 100  # Increase for better accuracy
```

## Related Topics

- [Basic Vector Search](./BASIC.md) - Basic operations
- [KNN Operations](./KNN.md) - K-nearest neighbor
- [Complete Vector Search Guide](./VECTOR_SEARCH.md) - Comprehensive reference

