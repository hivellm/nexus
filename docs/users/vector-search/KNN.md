---
title: KNN Operations
module: vector-search
id: knn-operations
order: 3
description: K-nearest neighbor operations
tags: [knn, vector, search, similarity]
---

# KNN Operations

Complete guide for K-nearest neighbor operations in Nexus.

## Overview

KNN (K-Nearest Neighbor) search finds the K most similar vectors to a query vector.

## Basic KNN

### Using Cypher

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 10
```

### Using REST API

```bash
POST /knn_traverse
Content-Type: application/json

{
  "label": "Person",
  "vector": [0.1, 0.2, 0.3, 0.4],
  "k": 10
}
```

## KNN Parameters

### K Value

```cypher
// Find top 5 similar
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 5
```

### With Filters

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.age > 25
RETURN n.name
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 10
```

## Similarity Metrics

### Cosine Similarity

```cypher
// Cosine distance (lower is more similar)
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector <-> [0.1, 0.2, 0.3, 0.4] AS cosine_distance
ORDER BY cosine_distance
LIMIT 10
```

### Euclidean Distance

```cypher
// Euclidean distance
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, 
       distance(n.vector, [0.1, 0.2, 0.3, 0.4]) AS euclidean_distance
ORDER BY euclidean_distance
LIMIT 10
```

## HNSW Index

### Automatic Indexing

HNSW indexes are automatically created for vector properties.

### Index Parameters

- **m**: Number of connections (default: 16)
- **ef_construction**: Construction parameter (default: 200)
- **ef_search**: Search parameter (default: 50)

## Performance

### Optimization Tips

1. **Normalize Vectors**: For better cosine similarity
2. **Use Appropriate K**: Don't request more than needed
3. **Filter Early**: Combine with property filters
4. **Batch Queries**: Use bulk operations when possible

## Related Topics

- [Basic Vector Search](./BASIC.md) - Basic operations
- [Advanced Vector Search](./ADVANCED.md) - Advanced patterns
- [Complete Vector Search Guide](./VECTOR_SEARCH.md) - Comprehensive reference

