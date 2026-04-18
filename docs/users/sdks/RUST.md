---
title: Rust SDK
module: sdks
id: rust-sdk
order: 4
description: Complete Rust SDK guide
tags: [rust, sdk, client, library]
---

# Rust SDK

Complete guide for the Nexus Rust SDK.

## Installation

Add to `Cargo.toml`:

```toml
[dependencies]
nexus-sdk = "0.12.0"
tokio = { version = "1.0", features = ["full"] }
```

## Quick Start

```rust
use nexus_sdk::NexusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let client = NexusClient::new("http://localhost:15474")?;
    
    // Execute Cypher query
    let result = client.execute_cypher(
        "MATCH (n:Person) RETURN n.name, n.age LIMIT 10",
        &[]
    ).await?;
    
    println!("{:?}", result.rows);
    Ok(())
}
```

## Authentication

### API Key

```rust
use nexus_sdk::NexusClient;

let client = NexusClient::builder()
    .base_url("http://localhost:15474")?
    .api_key("nx_abc123def456...")
    .build()?;
```

### JWT Token

```rust
use nexus_sdk::NexusClient;

// Login first
let client = NexusClient::new("http://localhost:15474")?;
let token = client.login("username", "password").await?;

// Use token
let authenticated_client = NexusClient::builder()
    .base_url("http://localhost:15474")?
    .jwt_token(&token)
    .build()?;
```

## Basic Operations

### Execute Cypher Query

```rust
// Simple query
let result = client.execute_cypher(
    "MATCH (n:Person) RETURN n LIMIT 10",
    &[]
).await?;

// Query with parameters
use nexus_sdk::Value;
let result = client.execute_cypher(
    "MATCH (n:Person {name: $name}) RETURN n",
    &[("name", Value::String("Alice".to_string()))]
).await?;
```

### Create Node

```rust
use nexus_sdk::{NexusClient, Value};

// Using Cypher
let result = client.execute_cypher(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    &[
        ("name", Value::String("Alice".to_string())),
        ("age", Value::Number(30.into()))
    ]
).await?;

// Using API
let node = client.create_node(
    &["Person"],
    &[
        ("name", Value::String("Alice".to_string())),
        ("age", Value::Number(30.into()))
    ]
).await?;
```

### Query Nodes

```rust
// Query by label
let nodes = client.query_nodes("Person", None, 10).await?;

// Query with filter
let nodes = client.query_nodes(
    "Person",
    Some("n.age > 25"),
    10
).await?;
```

## Vector Search

### Create Node with Vector

```rust
use nexus_sdk::{NexusClient, Value};

let vector: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4];
let result = client.execute_cypher(
    "CREATE (n:Person {name: $name, vector: $vector}) RETURN n",
    &[
        ("name", Value::String("Alice".to_string())),
        ("vector", Value::Array(vector.iter().map(|&v| Value::Number(v.into())).collect()))
    ]
).await?;
```

### KNN Search

```rust
// Using Cypher
let query_vector: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4];
let result = client.execute_cypher(
    "MATCH (n:Person) WHERE n.vector IS NOT NULL \
     RETURN n.name, n.vector \
     ORDER BY n.vector <-> $query_vector LIMIT 5",
    &[("query_vector", Value::Array(query_vector.iter().map(|&v| Value::Number(v.into())).collect()))]
).await?;

// Using KNN endpoint
let results = client.knn_traverse(
    "Person",
    &query_vector,
    10,
    Some("n.age > 25")
).await?;
```

## Database Management

### List Databases

```rust
let response = client.list_databases().await?;
for db in &response.databases {
    println!("Database: {}, Nodes: {}", db.name, db.node_count);
}
```

### Create Database

```rust
client.create_database("mydb").await?;
```

### Switch Database

```rust
client.switch_database("mydb").await?;
```

### Get Current Database

```rust
let current = client.get_current_database().await?;
println!("Current database: {}", current);
```

## Error Handling

```rust
use nexus_sdk::{NexusClient, NexusError};

let client = NexusClient::new("http://localhost:15474")?;

match client.execute_cypher("MATCH (n) RETURN n", &[]).await {
    Ok(result) => println!("Success: {:?}", result.rows),
    Err(NexusError::SyntaxError { message, .. }) => {
        eprintln!("Syntax error: {}", message);
    }
    Err(NexusError::AuthenticationError { message, .. }) => {
        eprintln!("Auth error: {}", message);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## Advanced Features

### Connection Pooling

```rust
use nexus_sdk::NexusClient;

let client = NexusClient::builder()
    .base_url("http://localhost:15474")?
    .max_connections(10)
    .connection_timeout(std::time::Duration::from_secs(30))
    .build()?;
```

### Retry Logic

```rust
use nexus_sdk::NexusClient;

let client = NexusClient::builder()
    .base_url("http://localhost:15474")?
    .max_retries(3)
    .retry_delay(std::time::Duration::from_secs(1))
    .build()?;
```

## Examples

### Social Network

```rust
// Create users
client.execute_cypher(
    "CREATE \
     (alice:Person {name: \"Alice\", age: 30}), \
     (bob:Person {name: \"Bob\", age: 28})",
    &[]
).await?;

// Create relationship
client.execute_cypher(
    "MATCH (a:Person {name: \"Alice\"}), (b:Person {name: \"Bob\"}) \
     CREATE (a)-[:KNOWS {since: \"2020\"}]->(b)",
    &[]
).await?;

// Query relationships
let result = client.execute_cypher(
    "MATCH (a:Person)-[r:KNOWS]->(b:Person) \
     RETURN a.name, b.name, r.since",
    &[]
).await?;
```

### Recommendation System

```rust
let result = client.execute_cypher(
    "MATCH (user:Person {id: 1})-[:LIKES]->(item:Item), \
          (similar:Person)-[:LIKES]->(item) \
     WHERE similar.vector IS NOT NULL \
       AND similar.vector <-> user.vector < 0.3 \
     RETURN DISTINCT similar.name, \
            similar.vector <-> user.vector as similarity \
     ORDER BY similarity LIMIT 10",
    &[]
).await?;
```

## Related Topics

- [SDKs Overview](./SDKS.md) - Compare all SDKs
- [API Reference](../api/API_REFERENCE.md) - REST API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language

