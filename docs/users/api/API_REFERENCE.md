---
title: REST API Reference
module: api
id: api-reference
order: 1
description: Complete REST API endpoint reference
tags: [api, rest, endpoints, reference]
---

# REST API Reference

Complete reference for all Nexus REST API endpoints.

## Base URL

```
http://localhost:15474
```

## Authentication

Most endpoints require authentication. Use one of:

- **API Key**: `X-API-Key: nx_...` or `Authorization: Bearer nx_...`
- **JWT Token**: `Authorization: Bearer <token>`

See [Authentication Guide](./AUTHENTICATION.md) for details.

## System Endpoints

### Health Check

```http
GET /health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "0.12.0",
  "uptime_seconds": 123
}
```

### Statistics

```http
GET /stats
```

**Response:**
```json
{
  "node_count": 1000,
  "relationship_count": 5000,
  "database_count": 1,
  "memory_usage_mb": 256
}
```

## Cypher Query

### Execute Query

```http
POST /cypher
Content-Type: application/json

{
  "query": "MATCH (n:Person) RETURN n LIMIT 10",
  "params": {},
  "timeout_ms": 5000
}
```

**Response:**
```json
{
  "columns": ["n"],
  "rows": [
    [{"id": 1, "labels": ["Person"], "properties": {"name": "Alice"}}]
  ],
  "execution_time_ms": 2
}
```

## Database Management

### List Databases

```http
GET /databases
```

**Response:**
```json
{
  "databases": [
    {
      "name": "neo4j",
      "path": "data/neo4j",
      "created_at": 1700000000,
      "node_count": 1000,
      "relationship_count": 5000
    }
  ],
  "default_database": "neo4j"
}
```

### Create Database

```http
POST /databases
Content-Type: application/json

{
  "name": "mydb"
}
```

### Get Database Info

```http
GET /databases/{name}
```

### Drop Database

```http
DELETE /databases/{name}
```

### Switch Database

```http
PUT /session/database
Content-Type: application/json

{
  "name": "mydb"
}
```

## Vector Search

### KNN Traverse

```http
POST /knn_traverse
Content-Type: application/json

{
  "label": "Person",
  "vector": [0.1, 0.2, 0.3, 0.4],
  "k": 10,
  "where": "n.age > 25",
  "limit": 100
}
```

## Schema Management

### List Labels

```http
GET /schema/labels
```

### Create Label

```http
POST /schema/labels
Content-Type: application/json

{
  "name": "Person"
}
```

### List Relationship Types

```http
GET /schema/rel_types
```

## Data Management

### Create Node

```http
POST /data/nodes
Content-Type: application/json

{
  "labels": ["Person"],
  "properties": {
    "name": "Alice",
    "age": 30
  }
}
```

### Update Node

```http
PUT /data/nodes
Content-Type: application/json

{
  "id": 1,
  "properties": {
    "age": 31
  }
}
```

### Delete Node

```http
DELETE /data/nodes
Content-Type: application/json

{
  "id": 1
}
```

## Bulk Operations

### Bulk Ingest

```http
POST /ingest
Content-Type: application/json

{
  "nodes": [
    {
      "labels": ["Person"],
      "properties": {"name": "Alice", "age": 30}
    }
  ],
  "relationships": [
    {
      "src": 1,
      "dst": 2,
      "type": "KNOWS",
      "properties": {"since": "2020"}
    }
  ]
}
```

## Error Responses

All errors follow this format:

```json
{
  "error": {
    "type": "ErrorType",
    "message": "Error description",
    "status_code": 400
  }
}
```

Common error types:
- `SyntaxError` - Invalid Cypher syntax
- `AuthenticationError` - Authentication failed
- `PermissionError` - Insufficient permissions
- `NotFoundError` - Resource not found
- `ValidationError` - Invalid input

## Rate Limiting

Rate limits are applied per API key:
- Default: 1000 requests/minute
- Default: 10000 requests/hour

Rate limit headers:
```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1704067200
```

## Related Topics

- [Authentication](./AUTHENTICATION.md) - Authentication setup
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language
- [SDKs Guide](../sdks/README.md) - Client SDKs

