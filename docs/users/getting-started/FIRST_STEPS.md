---
title: First Steps
module: getting-started
id: first-steps
order: 5
description: Complete guide after installation
tags: [first-steps, tutorial, getting-started]
---

# First Steps

Complete guide to get started with Nexus after installation.

## Verify Installation

### Check Service Status

**Linux:**
```bash
sudo systemctl status nexus
```

**Windows:**
```powershell
Get-Service Nexus
```

### Health Check

```bash
curl http://localhost:15474/health
```

Expected response:
```json
{
  "status": "healthy",
  "version": "0.12.0",
  "uptime_seconds": 123
}
```

### Check Statistics

```bash
curl http://localhost:15474/stats
```

## Create Your First Database

### Using Cypher

```cypher
CREATE DATABASE mydb
```

### Using REST API

```bash
curl -X POST http://localhost:15474/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "mydb"}'
```

### Using SDK

**Python:**
```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")
client.create_database("mydb")
```

## Create Your First Node

### Using Cypher

```cypher
CREATE (n:Person {name: "Alice", age: 30})
RETURN n
```

### Using REST API

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "CREATE (n:Person {name: \"Alice\", age: 30}) RETURN n"
  }'
```

## Query Your Data

### Basic Query

```cypher
MATCH (n:Person)
RETURN n.name, n.age
```

### Filter Query

```cypher
MATCH (n:Person)
WHERE n.age > 25
RETURN n.name, n.age
ORDER BY n.age DESC
```

## Create Relationships

```cypher
MATCH (a:Person {name: "Alice"}), (b:Person {name: "Bob"})
CREATE (a)-[:KNOWS {since: "2020"}]->(b)
RETURN a, b
```

## Vector Search

### Create Node with Vector

```cypher
CREATE (n:Person {
  name: "Alice",
  vector: [0.1, 0.2, 0.3, 0.4]
})
RETURN n
```

### Find Similar Vectors

```cypher
MATCH (n:Person)
WHERE n.vector IS NOT NULL
RETURN n.name, n.vector
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 5
```

## Next Steps

1. **[Cypher Basics](../cypher/BASIC.md)** - Learn Cypher syntax
2. **[Vector Search](../vector-search/BASIC.md)** - Start vector search
3. **[API Reference](../api/API_REFERENCE.md)** - Explore the API
4. **[Use Cases](../use-cases/)** - See real-world examples

## Related Topics

- [Quick Start Guide](./QUICK_START.md) - Quick start tutorial
- [Installation Guide](./INSTALLATION.md) - Installation instructions
- [Configuration Guide](../configuration/CONFIGURATION.md) - Configure Nexus

