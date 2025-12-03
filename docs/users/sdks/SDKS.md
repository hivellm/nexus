---
title: SDKs Overview
module: sdks
id: sdks-overview
order: 1
description: Quick comparison and overview of all SDKs
tags: [sdks, comparison, overview]
---

# SDKs Overview

Quick comparison and overview of all official Nexus SDKs.

## SDK Comparison

| SDK | Language | Status | Async | Type Safety | Package Manager |
|-----|----------|--------|-------|-------------|-----------------|
| Python | Python 3.8+ | ✅ Stable | ✅ Yes | ✅ Type hints | pip |
| TypeScript | TypeScript/JavaScript | ✅ Stable | ✅ Yes | ✅ Full | npm |
| Rust | Rust 1.85+ | ✅ Stable | ✅ Yes | ✅ Full | cargo |
| Go | Go 1.21+ | ✅ Stable | ✅ Yes | ✅ Full | go mod |
| C# | .NET 6+ | ✅ Stable | ✅ Yes | ✅ Full | NuGet |

## Installation

### Python

```bash
pip install nexus-sdk
```

### TypeScript

```bash
npm install @hivellm/nexus-sdk
```

### Rust

```toml
[dependencies]
nexus-sdk = "0.12.0"
```

### Go

```bash
go get github.com/hivellm/nexus-sdk-go
```

### C#

```bash
dotnet add package Nexus.Sdk
```

## Basic Usage Comparison

### Python

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")
result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10")
```

### TypeScript

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({ baseUrl: 'http://localhost:15474' });
const result = await client.executeCypher('MATCH (n) RETURN n LIMIT 10');
```

### Rust

```rust
use nexus_sdk::NexusClient;

let client = NexusClient::new("http://localhost:15474")?;
let result = client.execute_cypher("MATCH (n) RETURN n LIMIT 10", &[]).await?;
```

### Go

```go
import "github.com/hivellm/nexus-sdk-go"

client := nexus.NewClient("http://localhost:15474")
result, err := client.ExecuteCypher("MATCH (n) RETURN n LIMIT 10", nil)
```

### C#

```csharp
using Nexus.Sdk;

var client = new NexusClient("http://localhost:15474");
var result = await client.ExecuteCypherAsync("MATCH (n) RETURN n LIMIT 10");
```

## Feature Support

All SDKs support:

- ✅ Cypher query execution
- ✅ Database management
- ✅ Node operations
- ✅ Relationship operations
- ✅ Vector search
- ✅ Authentication
- ✅ Error handling
- ✅ Connection pooling

## Choosing an SDK

**Use Python if:**
- You're building data science applications
- You need integration with ML libraries
- You prefer rapid prototyping

**Use TypeScript if:**
- You're building web applications
- You need browser support
- You prefer JavaScript ecosystem

**Use Rust if:**
- You need maximum performance
- You're building system services
- You prefer memory safety

**Use Go if:**
- You're building microservices
- You need simple concurrency
- You prefer compiled binaries

**Use C# if:**
- You're building .NET applications
- You need enterprise features
- You prefer Microsoft ecosystem

## Related Topics

- [Python SDK](./PYTHON.md) - Complete Python guide
- [TypeScript SDK](./TYPESCRIPT.md) - Complete TypeScript guide
- [Rust SDK](./RUST.md) - Complete Rust guide
- [API Reference](../api/API_REFERENCE.md) - REST API documentation

