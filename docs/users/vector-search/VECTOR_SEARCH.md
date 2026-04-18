---
title: Complete Vector Search Guide
module: vector-search
id: complete-vector-search-guide
order: 4
description: Comprehensive vector search reference
tags: [vector, search, knn, similarity, complete]
---

# Complete Vector Search Guide

Comprehensive reference for vector similarity search in Nexus.

## Overview

Nexus provides native support for vector similarity search using HNSW (Hierarchical Navigable Small World) indexes.

## Vector Properties

### Creating Vectors

```cypher
// Create node with vector
CREATE (n:Person {
  name: "Alice",
  vector: [0.1, 0.2, 0.3, 0.4]
})
```

### Vector Dimensions

- All vectors in the same label must have the same dimension
- Common dimensions: 64, 128, 256, 512, 768, 1536
- Check dimension: `size(n.vector)`

## Similarity Operators

### Cosine Distance (`<->`)

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector <-> [0.1, 0.2, 0.3, 0.4] AS distance
ORDER BY distance
LIMIT 10
```

### Euclidean Distance

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, 
       distance(n.vector, [0.1, 0.2, 0.3, 0.4]) AS euclidean_distance
ORDER BY euclidean_distance
LIMIT 10
```

## KNN Operations

### Basic KNN Search

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 10
```

### KNN with Threshold

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.vector <-> [0.1, 0.2, 0.3, 0.4] < 0.5
RETURN n.name, n.vector
```

### KNN Traverse Endpoint

```bash
POST /knn_traverse
{
  "label": "Person",
  "vector": [0.1, 0.2, 0.3, 0.4],
  "k": 10,
  "where": "n.age > 25",
  "limit": 100
}
```

## Hybrid Queries

### Graph + Vector

```cypher
MATCH (a:Person)-[:KNOWS]->(b:Person)
WHERE a.vector IS NOT NULL
  AND b.vector IS NOT NULL
RETURN a.name, b.name, 
       a.vector <-> b.vector AS similarity
ORDER BY similarity
LIMIT 10
```

### Filtered Vector Search

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.age > 25
  AND n.city = 'NYC'
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 10
```

## HNSW Indexes

### Automatic Index Creation

HNSW indexes are automatically created for vector properties.

### Index Configuration

```yaml
indexes:
  vector:
    hnsw:
      m: 16
      ef_construction: 200
      ef_search: 50
```

## Best Practices

1. **Consistent Dimensions**: All vectors must have the same dimension
2. **Normalize Vectors**: Normalize for better cosine similarity
3. **Use Indexes**: HNSW indexes are automatically created
4. **Limit Results**: Always use LIMIT
5. **Filter Early**: Combine with property filters

## Performance Tips

1. **Batch Operations**: Use bulk insert for multiple vectors
2. **Dimension Size**: Keep dimensions reasonable (64-512)
3. **Index Tuning**: Adjust HNSW parameters for your use case
4. **Query Optimization**: Use KNN endpoint for large-scale search

## Related Topics

- [Basic Vector Search](./BASIC.md) - Basic operations
- [Advanced Vector Search](./ADVANCED.md) - Advanced patterns
- [KNN Operations](./KNN.md) - K-nearest neighbor

