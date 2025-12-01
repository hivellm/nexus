---
title: MCP Protocol
module: api
id: mcp-protocol
order: 3
description: Model Context Protocol integration
tags: [mcp, protocol, integration, ai]
---

# MCP Protocol

Complete guide for Model Context Protocol (MCP) integration with Nexus.

## Overview

Nexus supports the Model Context Protocol (MCP) for AI agent integrations, providing 19+ focused tools for graph database operations.

## Connection

### StreamableHTTP

Nexus uses StreamableHTTP (v0.9.0) as the transport protocol:

```
http://localhost:15474/mcp
```

### JSON-RPC 2.0

All MCP communication uses JSON-RPC 2.0 protocol.

## Available Tools

### Cypher Tools

- `execute_cypher` - Execute Cypher queries
- `explain_query` - Explain query execution plan
- `profile_query` - Profile query performance

### Graph Tools

- `create_node` - Create nodes
- `get_node` - Get node by ID
- `update_node` - Update node properties
- `delete_node` - Delete nodes
- `create_relationship` - Create relationships
- `get_relationship` - Get relationship by ID
- `delete_relationship` - Delete relationships

### Vector Tools

- `knn_search` - K-nearest neighbor search
- `vector_similarity` - Calculate vector similarity

### Schema Tools

- `list_labels` - List all labels
- `list_relationship_types` - List relationship types
- `create_index` - Create indexes

### Database Tools

- `list_databases` - List all databases
- `create_database` - Create database
- `switch_database` - Switch database

## Example Usage

### Execute Cypher Query

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "execute_cypher",
    "arguments": {
      "query": "MATCH (n:Person) RETURN n LIMIT 10"
    }
  }
}
```

### KNN Search

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "knn_search",
    "arguments": {
      "label": "Person",
      "vector": [0.1, 0.2, 0.3, 0.4],
      "k": 10
    }
  }
}
```

## Related Topics

- [UMICP Protocol](./UMICP.md) - Universal MCP Interface
- [API Reference](./API_REFERENCE.md) - REST API documentation
- [Integration Guide](./INTEGRATION.md) - Integration examples

