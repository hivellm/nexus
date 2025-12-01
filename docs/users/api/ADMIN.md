---
title: Admin API
module: api
id: admin-api
order: 7
description: Administrative endpoints
tags: [admin, administration, system, api]
---

# Admin API

Complete guide for administrative endpoints in Nexus.

## System Status

### Health Check

```bash
GET /health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "0.12.0",
  "uptime_seconds": 12345
}
```

### Statistics

```bash
GET /stats
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

## Configuration Management

### Get Configuration

```bash
GET /admin/config
```

### Update Configuration

```bash
PUT /admin/config
Content-Type: application/json

{
  "cache": {
    "max_size_mb": 2048
  }
}
```

## Server Management

### Restart Server

```bash
POST /admin/restart
```

⚠️ **Warning**: This will restart the server.

### Shutdown Server

```bash
POST /admin/shutdown
```

⚠️ **Warning**: This will shutdown the server.

## Log Access

### Get Logs

```bash
GET /admin/logs?level=error&limit=100
```

### Clear Logs

```bash
DELETE /admin/logs
```

## Metrics

### Prometheus Metrics

```bash
GET /metrics
```

Returns Prometheus-formatted metrics.

## Related Topics

- [API Reference](./API_REFERENCE.md) - REST API documentation
- [Monitoring Guide](../operations/MONITORING.md) - Health checks and metrics
- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration

