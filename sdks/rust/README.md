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

### Multi-Database Support

```rust
// List all databases
let databases = client.list_databases().await?;
println!("Available databases: {:?}", databases.databases);
println!("Default database: {}", databases.default_database);

// Create a new database
let create_result = client.create_database("mydb").await?;
println!("Created: {}", create_result.name);

// Switch to the new database
let switch_result = client.switch_database("mydb").await?;
println!("Switched to: mydb");

// Get current database
let current_db = client.get_current_database().await?;
println!("Current database: {}", current_db);

// Create data in the current database
let result = client.execute_cypher(
    "CREATE (n:Product {name: $name}) RETURN n",
    Some(serde_json::json!({"name": "Laptop"}))
).await?;

// Get database information
let db_info = client.get_database("mydb").await?;
println!("Nodes: {}, Relationships: {}", db_info.node_count, db_info.relationship_count);

// Drop database (must not be current database)
client.switch_database("neo4j").await?;  // Switch away first
let drop_result = client.drop_database("mydb").await?;

// Or connect directly to a specific database
let db_client = NexusClient::builder()
    .url("http://localhost:15474")
    .database("mydb")
    .build()?;
// All operations will use 'mydb'
let result = db_client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None).await?;
```

### High Availability with Replication

Nexus supports master-replica replication for high availability and read scaling.
Use the **master** for all write operations and **replicas** for read operations.

```rust
use nexus_sdk::NexusClient;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Client for Nexus cluster with master-replica topology.
pub struct NexusCluster {
    master: NexusClient,
    replicas: Vec<NexusClient>,
    replica_index: AtomicUsize,
}

impl NexusCluster {
    /// Create a new cluster client.
    ///
    /// # Arguments
    /// * `master_url` - URL of the master node (for writes)
    /// * `replica_urls` - List of replica URLs (for reads)
    pub fn new(master_url: &str, replica_urls: &[&str]) -> Result<Self, nexus_sdk::Error> {
        let master = NexusClient::new(master_url)?;
        let replicas = replica_urls
            .iter()
            .map(|url| NexusClient::new(url))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            master,
            replicas,
            replica_index: AtomicUsize::new(0),
        })
    }

    /// Get next replica using round-robin selection.
    fn get_replica(&self) -> &NexusClient {
        if self.replicas.is_empty() {
            return &self.master;
        }
        let index = self.replica_index.fetch_add(1, Ordering::Relaxed) % self.replicas.len();
        &self.replicas[index]
    }

    /// Execute write query on master.
    pub async fn write(
        &self,
        query: &str,
        params: Option<serde_json::Value>,
    ) -> Result<nexus_sdk::QueryResult, nexus_sdk::Error> {
        self.master.execute_cypher(query, params).await
    }

    /// Execute read query on replica (round-robin).
    pub async fn read(
        &self,
        query: &str,
        params: Option<serde_json::Value>,
    ) -> Result<nexus_sdk::QueryResult, nexus_sdk::Error> {
        self.get_replica().execute_cypher(query, params).await
    }

    /// Get reference to master client.
    pub fn master(&self) -> &NexusClient {
        &self.master
    }

    /// Get reference to all replicas.
    pub fn replicas(&self) -> &[NexusClient] {
        &self.replicas
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to cluster
    // Master handles all writes, replicas handle reads
    let cluster = NexusCluster::new(
        "http://master:15474",
        &["http://replica1:15474", "http://replica2:15474"],
    )?;

    // Write operations go to master
    cluster.write(
        "CREATE (n:Person {name: $name, age: $age}) RETURN n",
        Some(serde_json::json!({"name": "Alice", "age": 30})),
    ).await?;

    // Read operations are distributed across replicas
    let result = cluster.read(
        "MATCH (n:Person) WHERE n.age > $min_age RETURN n",
        Some(serde_json::json!({"min_age": 25})),
    ).await?;
    println!("Found {} persons", result.rows.len());

    // High-volume reads are load-balanced
    for _ in 0..100 {
        cluster.read("MATCH (n) RETURN count(n) as total", None).await?;
    }

    Ok(())
}
```

#### Replication Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              NexusCluster Client                     │   │
│   │   write() ──────────┐     read() ───────────────┐   │   │
│   └─────────────────────┼───────────────────────────┼───┘   │
└─────────────────────────┼───────────────────────────┼───────┘
                          │                           │
                          ▼                           ▼
              ┌───────────────────┐     ┌─────────────────────┐
              │      MASTER       │     │      REPLICAS       │
              │   (writes only)   │────▶│   (reads only)      │
              │                   │ WAL │  ┌───────────────┐  │
              │ • CREATE          │────▶│  │   Replica 1   │  │
              │ • UPDATE          │     │  └───────────────┘  │
              │ • DELETE          │     │  ┌───────────────┐  │
              │ • MERGE           │────▶│  │   Replica 2   │  │
              └───────────────────┘     │  └───────────────┘  │
                                        └─────────────────────┘
```

#### Starting a Nexus Cluster

```bash
# Start master node
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
./nexus-server

# Start replica 1
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server

# Start replica 2
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server
```

#### Monitoring Replication Status

```rust
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ReplicationStatus {
    role: String,
    running: bool,
    mode: String,
    replica_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MasterStats {
    entries_replicated: u64,
    connected_replicas: u32,
}

async fn check_replication_status(master_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    // Check master status
    let status: ReplicationStatus = client
        .get(format!("{}/replication/status", master_url))
        .send()
        .await?
        .json()
        .await?;
    println!("Role: {}", status.role);
    println!("Running: {}", status.running);
    println!("Connected replicas: {}", status.replica_count.unwrap_or(0));

    // Get master stats
    let stats: MasterStats = client
        .get(format!("{}/replication/master/stats", master_url))
        .send()
        .await?
        .json()
        .await?;
    println!("Entries replicated: {}", stats.entries_replicated);
    println!("Connected replicas: {}", stats.connected_replicas);

    Ok(())
}
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
- ✅ Multi-database support (create, list, switch, drop databases)
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
- `multi_database.rs` - Multi-database support examples

Run examples with:

```bash
cargo run --example basic_usage
cargo run --example with_auth
cargo run --example performance_monitoring
cargo run --example transactions
cargo run --example query_builder
cargo run --example multi_database
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
cargo test --test multi_database_test
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
- ✅ Multi-database support (Create, list, switch, drop databases)

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