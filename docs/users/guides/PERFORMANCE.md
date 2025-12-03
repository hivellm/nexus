---
title: Performance Optimization
module: guides
id: performance-optimization
order: 4
description: Advanced optimization tips
tags: [performance, optimization, tuning, benchmarks]
---

# Performance Optimization

Advanced optimization tips for Nexus.

## Query Optimization

### Use Indexes

```cypher
// Create indexes on frequently queried properties
CREATE INDEX ON :Person(name)
CREATE INDEX ON :Person(age)
CREATE INDEX ON :Person(email)
```

### Filter Early

```cypher
// Good: Filter early
MATCH (n:Person)
WHERE n.age > 25 AND n.city = 'NYC'
RETURN n

// Bad: Filter late
MATCH (n:Person)
RETURN n
// Then filter in application
```

### Use LIMIT

```cypher
// Always limit result sets
MATCH (n:Person)
RETURN n
LIMIT 100
```

### Avoid Cartesian Products

```cypher
// Bad: Cartesian product
MATCH (a:Person), (b:Person)
WHERE a.city = b.city
RETURN a, b

// Good: Use relationships
MATCH (a:Person)-[:LIVES_IN]->(city:City)<-[:LIVES_IN]-(b:Person)
RETURN a, b
```

## Index Strategies

### Property Indexes

```cypher
// Create single property index
CREATE INDEX ON :Person(name)

// Create composite index (if supported)
CREATE INDEX ON :Person(name, age)
```

### Vector Indexes

```cypher
// HNSW indexes are automatically created for vector properties
// Ensure consistent vector dimensions
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN DISTINCT size(n.vector) AS dimension
```

## Cache Tuning

### Cache Configuration

```yaml
# config.yml
cache:
  max_size_mb: 2048
  eviction_policy: "lru"
  warmup_enabled: true
```

### Cache Warming

```cypher
// Pre-load frequently accessed data
MATCH (n:Person)
WHERE n.popular = true
RETURN n
```

## Memory Management

### Connection Pooling

```yaml
# config.yml
server:
  max_connections: 100
  connection_timeout_seconds: 30
```

### Query Timeout

```cypher
// Set appropriate timeouts
{
  "query": "MATCH (n) RETURN n",
  "timeout_ms": 5000
}
```

## Vector Search Optimization

### Consistent Dimensions

```cypher
// Ensure all vectors have the same dimension
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN DISTINCT size(n.vector) AS dimension
```

### Normalize Vectors

Normalize vectors for better cosine similarity results:

```python
import numpy as np

vector = np.array([0.1, 0.2, 0.3, 0.4])
normalized = vector / np.linalg.norm(vector)
```

### Use KNN Endpoint

For large-scale vector search, use the dedicated KNN endpoint:

```bash
curl -X POST http://localhost:15474/knn_traverse \
  -H "Content-Type: application/json" \
  -d '{
    "label": "Person",
    "vector": [0.1, 0.2, 0.3, 0.4],
    "k": 10
  }'
```

## Benchmarking

### Query Performance

```cypher
// Use PROFILE to see execution stats
PROFILE MATCH (n:Person) WHERE n.age > 25 RETURN n
```

### Monitor Statistics

```bash
# Check server statistics
curl http://localhost:15474/stats
```

### Performance Metrics

Key metrics to monitor:
- Query execution time
- Memory usage
- Cache hit rate
- Connection count
- Throughput (queries/second)

## Best Practices

1. **Create Indexes** on frequently queried properties
2. **Use LIMIT** to avoid large result sets
3. **Filter Early** with WHERE clauses
4. **Normalize Vectors** for better similarity results
5. **Monitor Performance** regularly
6. **Use Connection Pooling** for high concurrency
7. **Set Appropriate Timeouts** for queries
8. **Cache Frequently Accessed Data**

## Related Topics

- [Configuration Guide](../configuration/PERFORMANCE_TUNING.md) - Performance tuning
- [Troubleshooting](../operations/TROUBLESHOOTING.md) - Performance problems
- [Cypher Guide](../cypher/CYPHER.md) - Query optimization

