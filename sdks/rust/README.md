# Nexus Rust SDK

Official Rust SDK for Nexus graph database.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
nexus-sdk-rust = "0.1.0"
```

## Usage

```rust
use nexus_sdk_rust::NexusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = NexusClient::new("http://localhost:15474")?;

    // Execute a Cypher query
    let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None).await?;

    println!("Found {} rows", result.rows.len());
    Ok(())
}
```

## Features

- ✅ Cypher query execution
- ✅ Database statistics
- ✅ Health check
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

- [ ] Transaction support
- [ ] Node CRUD operations
- [ ] Relationship CRUD operations
- [ ] Schema management
- [ ] Query builder
- [ ] Retry logic with exponential backoff
- [ ] Connection pooling
- [ ] Comprehensive tests

