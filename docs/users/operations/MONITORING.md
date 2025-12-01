---
title: Monitoring
module: operations
id: monitoring
order: 3
description: Health checks, metrics, and dashboards
tags: [monitoring, health, metrics, prometheus, grafana]
---

# Monitoring

Complete guide for monitoring Nexus.

## Health Checks

### Health Endpoint

```bash
curl http://localhost:15474/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "0.12.0",
  "uptime_seconds": 12345
}
```

### Health Check Script

```bash
#!/bin/bash
HEALTH_URL="http://localhost:15474/health"
STATUS=$(curl -s $HEALTH_URL | jq -r '.status')

if [ "$STATUS" = "healthy" ]; then
    echo "Nexus is healthy"
    exit 0
else
    echo "Nexus is unhealthy"
    exit 1
fi
```

## Statistics

### Statistics Endpoint

```bash
curl http://localhost:15474/stats
```

**Response:**
```json
{
  "node_count": 1000,
  "relationship_count": 5000,
  "database_count": 1,
  "memory_usage_mb": 256,
  "cache_hit_rate": 0.95,
  "queries_per_second": 100
}
```

### Monitor Statistics

```bash
# Watch statistics
watch -n 1 'curl -s http://localhost:15474/stats | jq'
```

## Prometheus Metrics

### Metrics Endpoint

```bash
curl http://localhost:15474/metrics
```

**Metrics Available:**
- `nexus_queries_total` - Total queries executed
- `nexus_queries_duration_seconds` - Query duration
- `nexus_nodes_total` - Total nodes
- `nexus_relationships_total` - Total relationships
- `nexus_memory_usage_bytes` - Memory usage
- `nexus_cache_hits_total` - Cache hits
- `nexus_cache_misses_total` - Cache misses

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: 'nexus'
    static_configs:
      - targets: ['localhost:15474']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

## Grafana Dashboards

### Dashboard Configuration

```json
{
  "dashboard": {
    "title": "Nexus Monitoring",
    "panels": [
      {
        "title": "Query Rate",
        "targets": [
          {
            "expr": "rate(nexus_queries_total[5m])"
          }
        ]
      },
      {
        "title": "Memory Usage",
        "targets": [
          {
            "expr": "nexus_memory_usage_bytes"
          }
        ]
      }
    ]
  }
}
```

## Log Monitoring

### View Logs

**Linux:**
```bash
# Follow logs
sudo journalctl -u nexus -f

# Filter by level
sudo journalctl -u nexus -p err

# Last 100 lines
sudo journalctl -u nexus -n 100
```

**Windows:**
```powershell
# Follow logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100 -Wait

# Filter errors
Get-Content C:\ProgramData\Nexus\logs\nexus.log | Select-String "ERROR"
```

## Alerting

### Alert Rules

```yaml
groups:
  - name: nexus_alerts
    rules:
      - alert: NexusDown
        expr: up{job="nexus"} == 0
        for: 5m
        annotations:
          summary: "Nexus is down"
      
      - alert: HighMemoryUsage
        expr: nexus_memory_usage_bytes > 8589934592
        for: 5m
        annotations:
          summary: "High memory usage"
      
      - alert: HighQueryLatency
        expr: nexus_queries_duration_seconds > 1
        for: 5m
        annotations:
          summary: "High query latency"
```

## Performance Monitoring

### Query Performance

```cypher
// Use PROFILE to see execution stats
PROFILE MATCH (n:Person) WHERE n.age > 25 RETURN n
```

### Monitor Slow Queries

```bash
# Log queries taking more than 1 second
export NEXUS_SLOW_QUERY_THRESHOLD_MS=1000
```

## Related Topics

- [Service Management](./SERVICE_MANAGEMENT.md) - Managing services
- [Troubleshooting](./TROUBLESHOOTING.md) - Common problems
- [Performance Guide](../guides/PERFORMANCE.md) - Performance optimization

