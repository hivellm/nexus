---
title: Python SDK
module: sdks
id: python-sdk
order: 2
description: Complete Python SDK guide
tags: [python, sdk, client, library]
---

# Python SDK

Complete guide for the Nexus Python SDK.

## Installation

```bash
pip install nexus-sdk
```

## Quick Start

```python
from nexus_sdk import NexusClient

# Create client
client = NexusClient("http://localhost:15474")

# Execute Cypher query
result = client.execute_cypher(
    "MATCH (n:Person) RETURN n.name, n.age LIMIT 10"
)

print(result.rows)
```

## Authentication

### API Key

```python
from nexus_sdk import NexusClient

client = NexusClient(
    "http://localhost:15474",
    api_key="nx_abc123def456..."
)
```

### JWT Token

```python
from nexus_sdk import NexusClient

# Login first
client = NexusClient("http://localhost:15474")
token = client.login("username", "password")

# Use token
client = NexusClient(
    "http://localhost:15474",
    jwt_token=token
)
```

## Basic Operations

### Execute Cypher Query

```python
# Simple query
result = client.execute_cypher(
    "MATCH (n:Person) RETURN n LIMIT 10"
)

# Query with parameters
result = client.execute_cypher(
    "MATCH (n:Person {name: $name}) RETURN n",
    params={"name": "Alice"}
)
```

### Create Node

```python
# Using Cypher
result = client.execute_cypher(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    params={"name": "Alice", "age": 30}
)

# Using API
node = client.create_node(
    labels=["Person"],
    properties={"name": "Alice", "age": 30}
)
```

### Query Nodes

```python
# Query by label
nodes = client.query_nodes(label="Person", limit=10)

# Query with filter
nodes = client.query_nodes(
    label="Person",
    where="n.age > 25",
    limit=10
)
```

## Vector Search

### Create Node with Vector

```python
result = client.execute_cypher(
    """
    CREATE (n:Person {
        name: $name,
        vector: $vector
    }) RETURN n
    """,
    params={
        "name": "Alice",
        "vector": [0.1, 0.2, 0.3, 0.4]
    }
)
```

### KNN Search

```python
# Using Cypher
result = client.execute_cypher(
    """
    MATCH (n:Person)
    WHERE n.vector IS NOT NULL
    RETURN n.name, n.vector
    ORDER BY n.vector <-> $query_vector
    LIMIT 5
    """,
    params={"query_vector": [0.1, 0.2, 0.3, 0.4]}
)

# Using KNN endpoint
results = client.knn_traverse(
    label="Person",
    vector=[0.1, 0.2, 0.3, 0.4],
    k=10,
    where="n.age > 25"
)
```

## Database Management

### List Databases

```python
databases = client.list_databases()
for db in databases.databases:
    print(f"Database: {db.name}, Nodes: {db.node_count}")
```

### Create Database

```python
client.create_database("mydb")
```

### Switch Database

```python
client.switch_database("mydb")
```

### Get Current Database

```python
current = client.get_current_database()
print(f"Current database: {current}")
```

## Error Handling

```python
from nexus_sdk import NexusError, NexusClient

client = NexusClient("http://localhost:15474")

try:
    result = client.execute_cypher("MATCH (n) RETURN n")
except NexusError as e:
    print(f"Error: {e.message}")
    print(f"Type: {e.error_type}")
    print(f"Status: {e.status_code}")
```

## Advanced Features

### Connection Pooling

```python
from nexus_sdk import NexusClient

client = NexusClient(
    "http://localhost:15474",
    max_connections=10,
    connection_timeout=30
)
```

### Async Operations

```python
import asyncio
from nexus_sdk import AsyncNexusClient

async def main():
    client = AsyncNexusClient("http://localhost:15474")
    result = await client.execute_cypher("MATCH (n) RETURN n LIMIT 10")
    print(result.rows)

asyncio.run(main())
```

### Batch Operations

```python
# Batch create nodes
nodes = [
    {"labels": ["Person"], "properties": {"name": "Alice", "age": 30}},
    {"labels": ["Person"], "properties": {"name": "Bob", "age": 28}}
]

result = client.bulk_create_nodes(nodes)
```

## Examples

### Social Network

```python
# Create users
client.execute_cypher(
    """
    CREATE 
      (alice:Person {name: "Alice", age: 30}),
      (bob:Person {name: "Bob", age: 28})
    """
)

# Create relationship
client.execute_cypher(
    """
    MATCH (a:Person {name: "Alice"}), (b:Person {name: "Bob"})
    CREATE (a)-[:KNOWS {since: "2020"}]->(b)
    """
)

# Query relationships
result = client.execute_cypher(
    """
    MATCH (a:Person)-[r:KNOWS]->(b:Person)
    RETURN a.name, b.name, r.since
    """
)
```

### Recommendation System

```python
# Find similar users
result = client.execute_cypher(
    """
    MATCH (user:Person {id: 1})-[:LIKES]->(item:Item),
          (similar:Person)-[:LIKES]->(item)
    WHERE similar.vector IS NOT NULL
      AND similar.vector <-> user.vector < 0.3
    RETURN DISTINCT similar.name, 
           similar.vector <-> user.vector as similarity
    ORDER BY similarity
    LIMIT 10
    """
)
```

## Related Topics

- [SDKs Overview](./SDKS.md) - Compare all SDKs
- [API Reference](../api/API_REFERENCE.md) - REST API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language

