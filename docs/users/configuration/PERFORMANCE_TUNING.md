---
title: Performance Tuning
module: configuration
id: performance-tuning
order: 5
description: Threads, memory, optimization
tags: [performance, tuning, optimization, configuration]
---

# Performance Tuning

Complete guide for optimizing Nexus performance.

## Thread Configuration

### Thread Pool Size

```yaml
server:
  thread_pool:
    size: 4  # Number of worker threads
```

**Environment Variable:**
```bash
export NEXUS_THREAD_POOL_SIZE=4
```

### Recommended Settings

- **Small deployments**: 2-4 threads
- **Medium deployments**: 4-8 threads
- **Large deployments**: 8-16 threads

## Memory Configuration

### Cache Size

```yaml
cache:
  max_size_mb: 2048
  eviction_policy: "lru"
```

**Environment Variable:**
```bash
export NEXUS_CACHE_SIZE_MB=2048
```

### Memory Limits

```yaml
server:
  memory:
    max_heap_mb: 4096
    max_stack_mb: 1024
```

## Connection Pooling

### Max Connections

```yaml
server:
  max_connections: 1000
  connection_timeout_seconds: 30
```

### Connection Pool Settings

```yaml
server:
  connection_pool:
    min_size: 10
    max_size: 100
    idle_timeout_seconds: 300
```

## Query Optimization

### Query Timeout

```yaml
server:
  query:
    default_timeout_ms: 5000
    max_timeout_ms: 30000
```

### Query Cache

```yaml
cache:
  query_cache:
    enabled: true
    max_size_mb: 512
    ttl_seconds: 3600
```

## Index Configuration

### Index Settings

```yaml
indexes:
  bitmap:
    enabled: true
  vector:
    hnsw:
      m: 16
      ef_construction: 200
      ef_search: 50
```

## Disk I/O Optimization

### WAL Settings

```yaml
wal:
  sync_interval_ms: 1000
  max_size_mb: 100
  compression: "lz4"
```

### Checkpoint Settings

```yaml
checkpoint:
  interval_seconds: 300
  compression: "zstd"
```

## Monitoring Performance

### Enable Metrics

```yaml
monitoring:
  metrics:
    enabled: true
    endpoint: "/metrics"
```

### Performance Profiling

```cypher
// Use PROFILE to see execution stats
PROFILE MATCH (n:Person) WHERE n.age > 25 RETURN n
```

## Best Practices

1. **Monitor Memory Usage**: Keep memory usage below 80%
2. **Optimize Queries**: Use indexes and LIMIT clauses
3. **Tune Thread Pool**: Match CPU cores
4. **Configure Cache**: Allocate 50-70% of available RAM
5. **Regular Maintenance**: Compact WAL, clean checkpoints

## Related Topics

- [Performance Guide](../guides/PERFORMANCE.md) - Advanced optimization
- [Monitoring Guide](../operations/MONITORING.md) - Performance monitoring
- [Configuration Overview](./CONFIGURATION.md) - General configuration

