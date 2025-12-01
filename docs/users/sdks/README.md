---
title: SDKs
module: sdks
id: sdks-index
order: 0
description: Client libraries and SDKs
tags: [sdks, clients, libraries, python, typescript, rust]
---

# SDKs

Official client libraries and SDKs for Nexus.

## Official SDKs

### [Python SDK](./PYTHON.md)

Complete Python SDK guide:
- Installation
- Basic usage
- Advanced features
- Examples

### [TypeScript SDK](./TYPESCRIPT.md)

TypeScript/JavaScript SDK guide:
- Installation
- Basic usage
- TypeScript types
- Examples

### [Rust SDK](./RUST.md)

Complete Rust SDK guide:
- Installation
- Basic usage
- Async/await patterns
- Examples

### [Go SDK](./GO.md)

Go SDK guide:
- Installation
- Basic usage
- Examples

### [C# SDK](./CSHARP.md)

.NET SDK guide:
- Installation
- Basic usage
- Examples

### [SDKs Overview](./SDKS.md)

Quick comparison and overview of all SDKs.

## Quick Start

### Python

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")
result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10")
print(result.rows)
```

### TypeScript

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474'
});

const result = await client.executeCypher('MATCH (n) RETURN n LIMIT 10');
console.log(result.rows);
```

### Rust

```rust
use nexus_sdk::NexusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NexusClient::new("http://localhost:15474")?;
    let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", &[]).await?;
    println!("{:?}", result.rows);
    Ok(())
}
```

## Common Features

All SDKs support:

- ✅ Cypher query execution
- ✅ Database management
- ✅ Node and relationship operations
- ✅ Vector search
- ✅ Authentication (API keys, JWT)
- ✅ Error handling
- ✅ Type safety

## Related Topics

- [API Reference](../api/API_REFERENCE.md) - REST API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language
- [Use Cases](../use-cases/) - Real-world examples

