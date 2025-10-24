# API & Protocol Specification

This document defines the REST HTTP API and integration protocols (MCP/UMICP) for Nexus.

## REST HTTP API

### Overview

Nexus exposes a RESTful HTTP API for:
- **Cypher query execution**
- **KNN-seeded graph traversal**
- **Bulk data ingestion**
- **Database statistics and health**

**Base URL**: `http://localhost:7474` (configurable)

### Authentication (V1)

```
MVP: No authentication (trust network)

V1: Bearer token authentication
Authorization: Bearer <token>

V2: mTLS for service-to-service
```

### Content Types

```
Request:  application/json
Response: application/json
Streaming: text/event-stream (Server-Sent Events)
```

## Endpoints

### Health Check

```http
GET /health
```

**Response**:
```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600
}
```

**Status Codes**:
- `200 OK`: Service healthy
- `503 Service Unavailable`: Service degraded

---

### Database Statistics

```http
GET /stats
```

**Response**:
```json
{
  "nodes": {
    "total": 1000000,
    "by_label": {
      "Person": 800000,
      "Company": 200000
    }
  },
  "relationships": {
    "total": 2000000,
    "by_type": {
      "KNOWS": 1500000,
      "WORKS_AT": 500000
    }
  },
  "indexes": {
    "knn": [
      {
        "label": "Person",
        "dimension": 768,
        "node_count": 800000,
        "size_mb": 2048
      }
    ]
  },
  "storage": {
    "total_size_mb": 5120,
    "wal_size_mb": 256,
    "page_cache_size_mb": 800,
    "page_cache_hit_rate": 0.94
  }
}
```

---

### Execute Cypher Query

```http
POST /cypher
Content-Type: application/json
```

**Request**:
```json
{
  "query": "MATCH (n:Person) WHERE n.age > $min_age RETURN n.name, n.age LIMIT 10",
  "params": {
    "min_age": 25
  },
  "timeout_ms": 30000
}
```

**Response**:
```json
{
  "columns": ["n.name", "n.age"],
  "rows": [
    ["Alice", 30],
    ["Bob", 28],
    ["Charlie", 35]
  ],
  "execution_time_ms": 12,
  "row_count": 3
}
```

**Error Response**:
```json
{
  "error": {
    "code": "CYPHER_SYNTAX_ERROR",
    "message": "Unexpected token 'WERE' at line 1, column 20. Expected 'WHERE'.",
    "query": "MATCH (n:Person) WERE n.age > 25 RETURN n",
    "position": {
      "line": 1,
      "column": 20
    }
  }
}
```

**Status Codes**:
- `200 OK`: Query executed successfully
- `400 Bad Request`: Syntax error, invalid parameters
- `408 Request Timeout`: Query exceeded timeout
- `500 Internal Server Error`: Execution error

---

### KNN-Seeded Traversal

```http
POST /knn_traverse
Content-Type: application/json
```

**Request**:
```json
{
  "label": "Person",
  "vector": [0.1, 0.2, 0.3, ..., 0.768],
  "k": 10,
  "expand": [
    "(n)-[:WORKS_AT]->(company:Company)"
  ],
  "where": "n.active = true AND company.size > 100",
  "return": ["n.name", "company.name", "score"],
  "order_by": "score DESC",
  "limit": 20
}
```

**Parameters**:
- `label`: Node label to search (required)
- `vector`: Query embedding (required, length must match index dimension)
- `k`: Number of nearest neighbors (required, > 0)
- `expand`: Optional graph patterns to expand from KNN results
- `where`: Optional filter predicate (Cypher syntax)
- `return`: Fields to return (default: all)
- `order_by`: Sort order (default: score DESC)
- `limit`: Result limit (default: 100)

**Response**:
```json
{
  "results": [
    {
      "node": {
        "id": 42,
        "labels": ["Person"],
        "properties": {
          "name": "Alice",
          "age": 30,
          "active": true
        }
      },
      "company": {
        "id": 10,
        "labels": ["Company"],
        "properties": {
          "name": "TechCorp",
          "size": 500
        }
      },
      "score": 0.95
    }
  ],
  "execution_time_ms": 8,
  "knn_time_ms": 2,
  "expand_time_ms": 4,
  "filter_time_ms": 2,
  "result_count": 20
}
```

**Status Codes**:
- `200 OK`: Query executed successfully
- `400 Bad Request`: Invalid vector dimension, missing parameters
- `404 Not Found`: KNN index not found for label
- `500 Internal Server Error`: Execution error

---

### Bulk Ingestion

```http
POST /ingest
Content-Type: application/json
```

**Request**:
```json
{
  "nodes": [
    {
      "id": 1,
      "labels": ["Person"],
      "properties": {
        "name": "Alice",
        "age": 30
      }
    },
    {
      "id": 2,
      "labels": ["Person"],
      "properties": {
        "name": "Bob",
        "age": 28
      }
    }
  ],
  "relationships": [
    {
      "src": 1,
      "dst": 2,
      "type": "KNOWS",
      "properties": {
        "since": 2020
      }
    }
  ],
  "options": {
    "skip_duplicates": true,
    "batch_size": 10000
  }
}
```

**Response**:
```json
{
  "nodes_ingested": 2,
  "relationships_ingested": 1,
  "ingestion_time_ms": 15,
  "throughput": {
    "nodes_per_sec": 133,
    "relationships_per_sec": 66
  },
  "errors": []
}
```

**Error Response** (partial failure):
```json
{
  "nodes_ingested": 1,
  "relationships_ingested": 0,
  "ingestion_time_ms": 10,
  "errors": [
    {
      "type": "node",
      "index": 1,
      "id": 2,
      "error": "Duplicate node ID: 2"
    },
    {
      "type": "relationship",
      "index": 0,
      "error": "Source node not found: 1"
    }
  ]
}
```

**Status Codes**:
- `200 OK`: Ingestion completed (may have partial errors, check errors array)
- `400 Bad Request`: Invalid input format
- `500 Internal Server Error`: Ingestion failed completely

---

### Streaming Results (V1)

```http
POST /cypher/stream
Content-Type: application/json
Accept: text/event-stream
```

**Request**:
```json
{
  "query": "MATCH (n:Person) RETURN n LIMIT 100000"
}
```

**Response** (Server-Sent Events):
```
event: metadata
data: {"columns": ["n"], "total_estimate": 100000}

event: row
data: {"n": {"id": 1, "labels": ["Person"], "properties": {"name": "Alice"}}}

event: row
data: {"n": {"id": 2, "labels": ["Person"], "properties": {"name": "Bob"}}}

...

event: complete
data: {"row_count": 100000, "execution_time_ms": 5000}
```

**Status Codes**:
- `200 OK`: Stream started
- `400 Bad Request`: Invalid query
- `408 Request Timeout`: Stream timeout

---

## Error Codes

### Standard Error Response

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error message",
    "details": {}
  }
}
```

### Error Code Table

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `CYPHER_SYNTAX_ERROR` | 400 | Invalid Cypher syntax |
| `CYPHER_SEMANTIC_ERROR` | 400 | Valid syntax, semantic error (undefined variable) |
| `INVALID_PARAMETER` | 400 | Invalid request parameter |
| `VECTOR_DIMENSION_MISMATCH` | 400 | Vector length doesn't match index |
| `INDEX_NOT_FOUND` | 404 | KNN index not found for label |
| `NODE_NOT_FOUND` | 404 | Node ID not found |
| `QUERY_TIMEOUT` | 408 | Query exceeded timeout |
| `TRANSACTION_CONFLICT` | 409 | Write-write conflict |
| `INTERNAL_ERROR` | 500 | Unexpected server error |
| `STORAGE_ERROR` | 500 | Storage layer failure |

---

## MCP Integration

### Model Context Protocol (MCP)

Nexus can be queried via MCP for AI/LLM integrations.

#### Configuration

```json
{
  "mcpServers": {
    "nexus": {
      "command": "nexus-mcp-server",
      "args": ["--db-url", "http://localhost:7474"],
      "env": {
        "NEXUS_API_KEY": "your-api-key"
      }
    }
  }
}
```

#### Available Tools

**1. nexus/query**

Execute Cypher query via MCP:

```json
{
  "name": "nexus/query",
  "description": "Execute a Cypher query on Nexus graph database",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Cypher query string"
      },
      "params": {
        "type": "object",
        "description": "Query parameters"
      }
    },
    "required": ["query"]
  }
}
```

**Example Call**:
```json
{
  "method": "tools/call",
  "params": {
    "name": "nexus/query",
    "arguments": {
      "query": "MATCH (n:Person {name: $name}) RETURN n",
      "params": {"name": "Alice"}
    }
  }
}
```

**2. nexus/knn_search**

Vector search via MCP:

```json
{
  "name": "nexus/knn_search",
  "description": "Perform KNN vector search",
  "inputSchema": {
    "type": "object",
    "properties": {
      "label": {"type": "string"},
      "vector": {"type": "array", "items": {"type": "number"}},
      "k": {"type": "integer"},
      "filters": {"type": "object"}
    },
    "required": ["label", "vector", "k"]
  }
}
```

#### MCP Resources

Expose graph schema as MCP resource:

```json
{
  "method": "resources/list",
  "result": {
    "resources": [
      {
        "uri": "nexus://schema",
        "name": "Graph Schema",
        "description": "Node labels, relationship types, property keys"
      }
    ]
  }
}
```

**Fetch Resource**:
```json
{
  "method": "resources/read",
  "params": {
    "uri": "nexus://schema"
  },
  "result": {
    "contents": {
      "labels": ["Person", "Company", "Product"],
      "types": ["KNOWS", "WORKS_AT", "BOUGHT"],
      "keys": ["name", "age", "city", "founded"]
    }
  }
}
```

---

## UMICP Integration

### Universal Model Interoperability Protocol (UMICP)

Cross-service communication protocol for distributed AI systems.

#### Endpoint Registration

```json
{
  "service": "nexus-graph",
  "version": "0.1.0",
  "capabilities": [
    "graph.query",
    "graph.knn_search",
    "graph.ingest"
  ],
  "endpoints": {
    "query": "http://localhost:7474/cypher",
    "knn": "http://localhost:7474/knn_traverse",
    "ingest": "http://localhost:7474/ingest"
  }
}
```

#### UMICP Call Example

```json
{
  "method": "graph.knn_search",
  "params": {
    "label": "Document",
    "query_text": "machine learning algorithms",
    "k": 10
  },
  "context": {
    "trace_id": "abc-123",
    "caller": "vectorizer",
    "timestamp": 1704067200
  }
}
```

**Response**:
```json
{
  "result": {
    "documents": [
      {"id": 42, "title": "Introduction to ML", "score": 0.92}
    ]
  },
  "metadata": {
    "execution_time_ms": 15,
    "service": "nexus-graph",
    "version": "0.1.0"
  }
}
```

---

## Client Libraries

### Rust Client (nexus-protocol)

```rust
use nexus_protocol::RestClient;

let client = RestClient::new("http://localhost:7474");

// Execute Cypher
let result = client.query("MATCH (n:Person) RETURN n LIMIT 10", None).await?;

// KNN search
let results = client.knn_traverse(
    "Person",
    &embedding,
    10,
    None,  // expand
    None,  // where
).await?;
```

### Python Client (V1)

```python
from nexus import NexusClient

client = NexusClient("http://localhost:7474")

# Execute Cypher
result = client.query(
    "MATCH (n:Person) WHERE n.age > $min_age RETURN n",
    params={"min_age": 25}
)

# KNN search
results = client.knn_search(
    label="Person",
    vector=embedding,
    k=10,
    filters={"active": True}
)
```

### TypeScript Client (V1)

```typescript
import { NexusClient } from '@nexus/client';

const client = new NexusClient('http://localhost:7474');

// Execute Cypher
const result = await client.query(
  'MATCH (n:Person) RETURN n LIMIT 10'
);

// KNN search
const results = await client.knnSearch({
  label: 'Person',
  vector: embedding,
  k: 10,
  filters: { active: true }
});
```

---

## Performance & Limits

### Rate Limiting (V1)

```
Default Limits:
- 1000 requests/sec per client IP
- 100 concurrent requests per client

Headers:
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 995
X-RateLimit-Reset: 1704067260
```

### Request Size Limits

```
Max request body size: 10 MB
Max query result size: 100 MB (paginate for larger)
Max vector dimension: 4096
Max k (KNN): 10000
```

### Timeouts

```
Default query timeout: 30 seconds
Max query timeout: 300 seconds (5 minutes)
Connection timeout: 10 seconds
Idle timeout: 60 seconds
```

---

## Monitoring & Observability

### Prometheus Metrics

```
# Query metrics
nexus_query_total{type="cypher|knn", status="success|error"}
nexus_query_duration_seconds{type="cypher|knn", quantile="0.5|0.95|0.99"}

# Ingestion metrics
nexus_ingest_nodes_total
nexus_ingest_relationships_total
nexus_ingest_duration_seconds

# System metrics
nexus_page_cache_hit_rate
nexus_wal_size_bytes
nexus_storage_size_bytes
```

### OpenTelemetry Traces

```
Trace hierarchy:
- /cypher (HTTP span)
  - parse_query
  - plan_query
  - execute_query
    - scan_nodes
    - expand_relationships
    - filter
    - project
```

---

## OpenAPI Specification

Full OpenAPI 3.0 spec available at:
```
GET /openapi.json
```

**Example**:
```yaml
openapi: 3.0.0
info:
  title: Nexus Graph Database API
  version: 0.1.0
paths:
  /cypher:
    post:
      summary: Execute Cypher query
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                query:
                  type: string
                params:
                  type: object
      responses:
        '200':
          description: Query results
          content:
            application/json:
              schema:
                type: object
                properties:
                  columns:
                    type: array
                    items:
                      type: string
                  rows:
                    type: array
```

---

## Security Considerations

### MVP (Development)

- No authentication
- No encryption
- Trust local network

### V1 (Production)

- **Authentication**: Bearer tokens (JWT)
- **Authorization**: Role-based access control (RBAC)
- **Encryption**: TLS 1.3 for transport
- **Audit logging**: All write operations

### V2 (Enterprise)

- **mTLS**: Mutual TLS for service-to-service
- **Encryption at rest**: AES-256-GCM
- **Key management**: Integration with Vault/KMS
- **Compliance**: SOC 2, GDPR, HIPAA

---

## Versioning

### API Versioning

```
Version in URL path:
GET /v1/cypher
POST /v1/knn_traverse

Version in header (alternative):
Accept: application/vnd.nexus.v1+json
```

### Compatibility

- **Minor versions**: Backwards compatible
- **Major versions**: Breaking changes allowed
- **Deprecation**: 6 months notice before removal

---

## References

- REST API Best Practices: https://restfulapi.net/
- MCP Specification: https://modelcontextprotocol.io/
- OpenAPI 3.0: https://swagger.io/specification/
- Server-Sent Events: https://html.spec.whatwg.org/multipage/server-sent-events.html

