# Nexus Rust SDK

[![Crates.io](https://img.shields.io/crates/v/nexus-sdk?style=flat-square)](https://crates.io/crates/nexus-sdk)
[![docs.rs](https://img.shields.io/docsrs/nexus-sdk?style=flat-square)](https://docs.rs/nexus-sdk)
[![License](https://img.shields.io/crates/l/nexus-sdk?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange?style=flat-square)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/hivellm/nexus/ci.yml?style=flat-square)](https://github.com/hivellm/nexus/actions)

Official Rust SDK for Nexus graph database.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
nexus-sdk = "0.1.0"
```

## Usage

### Basic Example

```rust
use nexus_sdk::NexusClient;
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
    properties.insert("name".to_string(), nexus_sdk::Value::String("Alice".to_string()));
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
use nexus_sdk::NexusClient;

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
- ✅ Relationship CRUD operations (Create, Update, Delete)
- ✅ Batch operations (sequential implementation)
- ✅ Schema management (Labels, Relationship Types)
- ✅ Performance monitoring (Query statistics, slow queries, plan cache)
- ✅ Transaction support (BEGIN, COMMIT, ROLLBACK)
- ✅ Query builder for type-safe query construction
- ✅ Retry logic with exponential backoff (basic implementation)
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

## Examples

See the `examples/` directory for complete examples:

- `basic_usage.rs` - Basic operations with nodes, relationships, and schema
- `with_auth.rs` - Authentication examples
- `performance_monitoring.rs` - Performance monitoring and statistics
- `transactions.rs` - Transaction management examples
- `query_builder.rs` - Query builder usage examples

Run examples with:

```bash
cargo run --example basic_usage
cargo run --example with_auth
cargo run --example performance_monitoring
cargo run --example transactions
cargo run --example query_builder
```

## Testing

Run tests with:

```bash
# Run unit tests
cargo test

# Run integration tests (requires running Nexus server)
NEXUS_TEST_SERVER=http://localhost:15474 cargo test -- --ignored

# Run specific test suites
cargo test --test integration_test
cargo test --test transaction_test
cargo test --test query_builder_test
cargo test --test relationship_test
```

### Test Coverage

The SDK includes comprehensive tests for:

- ✅ Client initialization and configuration
- ✅ Cypher query execution
- ✅ Node CRUD operations
- ✅ Relationship CRUD operations (Create, Update, Delete)
- ✅ Schema management (Labels, Relationship Types)
- ✅ Transaction support (Begin, Commit, Rollback)
- ✅ Query builder functionality
- ✅ Performance monitoring
- ✅ Batch operations

## Roadmap

- [x] Node CRUD operations
- [x] Relationship CRUD operations (Create, Update, Delete)
- [x] Schema management
- [x] Integration tests
- [x] Examples
- [x] Transaction support
- [x] Query builder
- [x] Retry logic with exponential backoff (basic implementation)
- [x] Neo4j compatibility comparison tests
- [ ] Connection pooling

## License

Licensed under the Apache License, Version 2.0.

See [LICENSE](LICENSE) for details.