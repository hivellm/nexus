---
title: Quick Start Guide
module: getting-started
id: quick-start
order: 2
description: Get up and running in minutes
tags: [quick-start, tutorial, getting-started]
---

# Quick Start Guide

Get up and running with Nexus in minutes.

## Prerequisites

- Nexus server running (see [Installation Guide](./INSTALLATION.md))
- `curl` or similar HTTP client
- Optional: Python, TypeScript, or Rust SDK

## First Query

### Using REST API

```bash
# Execute a simple Cypher query
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n) RETURN n LIMIT 10"
  }'
```

**Response:**
```json
{
  "columns": ["n"],
  "rows": [],
  "execution_time_ms": 1
}
```

### Create Your First Node

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "CREATE (n:Person {name: \"Alice\", age: 30}) RETURN n"
  }'
```

**Response:**
```json
{
  "columns": ["n"],
  "rows": [
    [
      {
        "id": 1,
        "labels": ["Person"],
        "properties": {
          "name": "Alice",
          "age": 30
        }
      }
    ]
  ],
  "execution_time_ms": 2
}
```

### Query the Node

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) WHERE n.name = \"Alice\" RETURN n.name, n.age"
  }'
```

**Response:**
```json
{
  "columns": ["n.name", "n.age"],
  "rows": [["Alice", 30]],
  "execution_time_ms": 1
}
```

## Using SDKs

### Python SDK

```python
from nexus_sdk import NexusClient

# Connect to server
client = NexusClient("http://localhost:15474")

# Execute query
result = client.execute_cypher(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    params={"name": "Bob", "age": 28}
)

print(result.rows)
```

### TypeScript SDK

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474'
});

const result = await client.executeCypher(
  'CREATE (n:Person {name: $name, age: $age}) RETURN n',
  { name: 'Bob', age: 28 }
);

console.log(result.rows);
```

### Rust SDK

```rust
use nexus_sdk::NexusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NexusClient::new("http://localhost:15474")?;
    
    let result = client.execute_cypher(
        "CREATE (n:Person {name: $name, age: $age}) RETURN n",
        &[("name", "Bob"), ("age", "28")]
    ).await?;
    
    println!("{:?}", result.rows);
    Ok(())
}
```

## Common Patterns

### Create Multiple Nodes

```cypher
CREATE 
  (alice:Person {name: "Alice", age: 30}),
  (bob:Person {name: "Bob", age: 28}),
  (charlie:Person {name: "Charlie", age: 35})
RETURN alice, bob, charlie
```

### Create Relationships

```cypher
MATCH (a:Person {name: "Alice"}), (b:Person {name: "Bob"})
CREATE (a)-[:KNOWS {since: "2020"}]->(b)
RETURN a, b
```

### Query Relationships

```cypher
MATCH (a:Person)-[r:KNOWS]->(b:Person)
RETURN a.name, b.name, r.since
```

### Filter and Sort

```cypher
MATCH (n:Person)
WHERE n.age > 25
RETURN n.name, n.age
ORDER BY n.age DESC
LIMIT 10
```

### Aggregation

```cypher
MATCH (n:Person)
RETURN COUNT(n) AS total_people, AVG(n.age) AS avg_age
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

- [Cypher Query Language](../cypher/BASIC.md) - Learn Cypher syntax
- [API Reference](../api/API_REFERENCE.md) - Complete API documentation
- [Examples](../use-cases/EXAMPLES.md) - Real-world examples
- [Performance Tips](../guides/PERFORMANCE.md) - Optimize your queries

## Related Topics

- [First Steps](./FIRST_STEPS.md) - Complete guide after installation
- [Installation Guide](./INSTALLATION.md) - Installation instructions
- [Docker Installation](./DOCKER.md) - Docker deployment

