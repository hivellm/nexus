# Nexus Rust SDK

Official Rust SDK for Nexus graph database.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
nexus-sdk-rust = "0.1.0"
```

## Usage

### Basic Example

```rust
use nexus_sdk_rust::NexusClient;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = NexusClient::new("http://localhost:15474")?;

    // Execute a Cypher query
    let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None).await?;
    println!("Found {} rows", result.rows.len());

    // Create a node
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), nexus_sdk_rust::Value::String("Alice".to_string()));
    let create_response = client.create_node(vec!["Person".to_string()], properties).await?;
    println!("Created node with ID: {}", create_response.node_id);

    // Get a node
    let get_response = client.get_node(create_response.node_id).await?;
    if let Some(node) = get_response.node {
        println!("Node: {:?}", node);
    }

    Ok(())
}
```

### With Authentication

```rust
use nexus_sdk_rust::NexusClient;

// Using API key
let client = NexusClient::with_api_key("http://localhost:15474", "your-api-key")?;

// Or using username/password
let client = NexusClient::with_credentials("http://localhost:15474", "user", "pass")?;
```

### Schema Management

```rust
// Create a label
let response = client.create_label("Person".to_string()).await?;

// List all labels
let labels = client.list_labels().await?;
println!("Labels: {:?}", labels.labels);

// Create a relationship type
let response = client.create_rel_type("KNOWS".to_string()).await?;

// List all relationship types
let types = client.list_rel_types().await?;
println!("Types: {:?}", types.types);
```

## Features

- ✅ Cypher query execution
- ✅ Database statistics
- ✅ Health check
- ✅ Node CRUD operations (Create, Read, Update, Delete)
- ✅ Relationship CRUD operations (Create)
- ✅ Schema management (Labels, Relationship Types)
- ✅ API key authentication
- ✅ Username/password authentication
- ✅ Proper error handling
- ✅ Type-safe models

## Dependencies Status

All dependencies are up-to-date:

- `reqwest` 0.12 - Latest stable
- `tokio` 1.0 - Latest stable
- `serde` 1.0 - Latest stable
- `serde_json` 1.0 - Latest stable
- `thiserror` 1.0 - Latest 1.x (2.0 available but breaking changes)
- `anyhow` 1.0 - Latest stable
- `futures` 0.3 - Latest stable
- `url` 2.5 - Latest stable
- `base64` 0.22 - Latest stable
- `tracing` 0.1 - Latest stable

## Roadmap

- [x] Node CRUD operations
- [x] Relationship CRUD operations (Create)
- [x] Schema management
- [ ] Transaction support
- [ ] Query builder
- [ ] Retry logic with exponential backoff
- [ ] Connection pooling
- [ ] Comprehensive tests
- [ ] Relationship update/delete operations

