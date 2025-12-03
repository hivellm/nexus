---
title: API Documentation
module: api
id: api-index
order: 0
description: REST API, MCP, and integration documentation
tags: [api, rest, mcp, endpoints, integration]
---

# API Documentation

Complete REST API, MCP, UMICP, and integration guides.

## API Reference

### [REST API Reference](./API_REFERENCE.md)

Complete reference for all API endpoints:

**REST API:**

- System endpoints (health, stats)
- Cypher query execution
- Database management
- Node and relationship operations
- Vector search endpoints
- Schema management
- Authentication endpoints

### [MCP Protocol](./MCP.md)

Model Context Protocol integration:

- StreamableHTTP connection
- JSON-RPC 2.0 protocol
- 19+ MCP tools (cypher, graph, vector, etc.)
- Complete tool reference with examples

### [UMICP Protocol](./UMICP.md)

Universal Multi-Agent Communication Protocol:

- UMICP v0.2.1 support
- Envelope-based communication
- Tool discovery endpoint (`/umicp/discover`)
- All MCP tools available via UMICP

## Advanced APIs

### [Authentication](./AUTHENTICATION.md)

Authentication and authorization:

- API keys
- JWT tokens
- RBAC permissions
- Rate limiting
- Root user setup

### [Replication API](./REPLICATION.md)

Master-replica replication:

- High availability setup
- Read scaling
- Replication monitoring
- Failover procedures

### [Admin API](./ADMIN.md)

Administrative endpoints:

- Server status and monitoring
- Configuration management
- Log access
- Server management

### [Integration Guide](./INTEGRATION.md)

Integrating Nexus with other systems:

- Web frameworks (FastAPI, Express, Axum)
- Databases (PostgreSQL, MongoDB)
- LLMs (OpenAI, LangChain)
- ETL pipelines (Airflow, Kafka)
- Monitoring (Prometheus, Grafana)
- CI/CD (GitHub Actions, GitLab CI)

## Quick Reference

### Base URL

```
http://localhost:15474
```

### Common Endpoints

- `GET /health` - Health check
- `POST /cypher` - Execute Cypher query
- `GET /databases` - List databases
- `POST /databases` - Create database
- `GET /stats` - Server statistics

## Authentication

Nexus supports multiple authentication methods:

- **API Keys**: Long-lived credentials (`X-API-Key` header or `Authorization: Bearer nx_...`)
- **JWT Tokens**: Short-lived tokens (`Authorization: Bearer <token>`)
- **Root User**: Initial superuser account

See [Authentication Guide](./AUTHENTICATION.md) for details.

## Response Format

**Success:**

```json
{
  "columns": ["n.name", "n.age"],
  "rows": [["Alice", 30]],
  "execution_time_ms": 2
}
```

**Error:**

```json
{
  "error": {
    "type": "SyntaxError",
    "message": "Invalid Cypher syntax",
    "status_code": 400
  }
}
```

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Using Cypher via API
- [Vector Search](../vector-search/) - Vector operations
- [SDKs Guide](../sdks/README.md) - Client SDKs that wrap the API

