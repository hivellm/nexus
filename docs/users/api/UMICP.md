---
title: UMICP Protocol
module: api
id: umicp-protocol
order: 4
description: Universal Multi-Agent Communication Protocol
tags: [umicp, protocol, integration, ai]
---

# UMICP Protocol

Complete guide for Universal Multi-Agent Communication Protocol (UMICP) integration.

## Overview

UMICP v0.2.1 provides a standardized protocol interface for multi-agent communication with Nexus.

## Endpoint

```
http://localhost:15474/umicp
```

## Tool Discovery

### Discover Tools

```bash
GET /umicp/discover
```

**Response:**
```json
{
  "tools": [
    {
      "name": "execute_cypher",
      "description": "Execute Cypher query",
      "parameters": {
        "query": {"type": "string", "required": true}
      }
    }
  ]
}
```

## Communication Format

### Request Format

```json
{
  "method": "tool.call",
  "params": {
    "name": "execute_cypher",
    "arguments": {
      "query": "MATCH (n) RETURN n LIMIT 10"
    }
  }
}
```

### Response Format

```json
{
  "result": {
    "columns": ["n"],
    "rows": [[{"id": 1, "labels": ["Person"]}]]
  }
}
```

## Available Methods

All 19+ MCP tools are available via UMICP:

- `tool.call` - Call any MCP tool
- `tool.list` - List all available tools
- `tool.describe` - Describe a specific tool

## Related Topics

- [MCP Protocol](./MCP.md) - Model Context Protocol
- [API Reference](./API_REFERENCE.md) - REST API documentation
- [Integration Guide](./INTEGRATION.md) - Integration examples

