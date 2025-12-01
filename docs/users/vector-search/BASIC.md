---
title: Basic Vector Search
module: vector-search
id: vector-search-basics
order: 1
description: Simple vector queries and operations
tags: [vector, search, basics, knn, tutorial]
---

# Basic Vector Search

Learn the fundamentals of vector similarity search in Nexus.

## Creating Nodes with Vectors

### Basic Vector Property

```cypher
CREATE (n:Person {
  name: "Alice",
  vector: [0.1, 0.2, 0.3, 0.4]
})
RETURN n
```

### Multiple Vectors

```cypher
CREATE 
  (alice:Person {name: "Alice", vector: [0.1, 0.2, 0.3, 0.4]}),
  (bob:Person {name: "Bob", vector: [0.2, 0.3, 0.4, 0.5]}),
  (charlie:Person {name: "Charlie", vector: [0.3, 0.4, 0.5, 0.6]})
RETURN alice, bob, charlie
```

## Basic Similarity Search

### Find Similar Vectors

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 5
```

### Similarity with Threshold

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.vector <-> [0.1, 0.2, 0.3, 0.4] < 0.5
RETURN n.name, n.vector
```

## Similarity Operators

### Distance Operator (`<->`)

The `<->` operator calculates cosine distance (lower is more similar):

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector <-> [0.1, 0.2, 0.3, 0.4] AS distance
ORDER BY distance
LIMIT 10
```

### Euclidean Distance

For Euclidean distance, use the `distance()` function:

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, 
       distance(n.vector, [0.1, 0.2, 0.3, 0.4]) AS euclidean_distance
ORDER BY euclidean_distance
LIMIT 10
```

## Filtering with Vectors

### Vector + Property Filter

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.age > 25
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 10
```

### Multiple Conditions

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
  AND n.vector <-> [0.1, 0.2, 0.3, 0.4] < 0.5
  AND n.city = 'NYC'
RETURN n.name, n.vector
```

## Using REST API

### KNN Traverse Endpoint

```bash
curl -X POST http://localhost:15474/knn_traverse \
  -H "Content-Type: application/json" \
  -d '{
    "label": "Person",
    "vector": [0.1, 0.2, 0.3, 0.4],
    "k": 10,
    "where": "n.age > 25",
    "limit": 100
  }'
```

## Vector Dimensions

### Check Vector Dimension

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, size(n.vector) AS dimension
LIMIT 10
```

### Common Dimensions

- **64 dimensions**: Small embeddings
- **128 dimensions**: Medium embeddings
- **256 dimensions**: Large embeddings
- **512+ dimensions**: Very large embeddings

## Best Practices

1. **Consistent Dimensions**: All vectors in the same label should have the same dimension
2. **Normalize Vectors**: Normalize vectors for better cosine similarity results
3. **Use Indexes**: HNSW indexes are automatically created for vector properties
4. **Limit Results**: Always use LIMIT to avoid large result sets
5. **Filter Early**: Combine vector search with property filters for better performance

## Next Steps

- [Advanced Vector Search](./ADVANCED.md) - Advanced patterns and optimization
- [KNN Operations](./KNN.md) - K-nearest neighbor operations
- [Complete Vector Search Guide](./VECTOR_SEARCH.md) - Comprehensive reference

## Related Topics

- [Cypher Basics](../cypher/BASIC.md) - Basic Cypher syntax
- [API Reference](../api/API_REFERENCE.md) - REST API for vector search
- [Use Cases](../use-cases/) - Real-world examples

