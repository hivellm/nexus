---
title: Integration Guide
module: api
id: integration-guide
order: 5
description: Web frameworks, databases, LLMs, monitoring
tags: [integration, frameworks, databases, llm, monitoring]
---

# Integration Guide

Complete guide for integrating Nexus with other systems.

## Web Frameworks

### FastAPI (Python)

```python
from fastapi import FastAPI
from nexus_sdk import NexusClient

app = FastAPI()
nexus = NexusClient("http://localhost:15474")

@app.get("/users")
async def get_users():
    result = await nexus.execute_cypher(
        "MATCH (n:Person) RETURN n LIMIT 10"
    )
    return {"users": result.rows}
```

### Express (Node.js)

```javascript
const express = require('express');
const { NexusClient } = require('@hivellm/nexus-sdk');

const app = express();
const nexus = new NexusClient({ baseUrl: 'http://localhost:15474' });

app.get('/users', async (req, res) => {
  const result = await nexus.executeCypher('MATCH (n:Person) RETURN n LIMIT 10');
  res.json({ users: result.rows });
});
```

### Axum (Rust)

```rust
use axum::{Router, Json};
use nexus_sdk::NexusClient;

async fn get_users() -> Json<serde_json::Value> {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let result = client.execute_cypher("MATCH (n:Person) RETURN n LIMIT 10", &[]).await.unwrap();
    Json(serde_json::json!({ "users": result.rows }))
}
```

## LLM Integration

### OpenAI

```python
from nexus_sdk import NexusClient
from openai import OpenAI

nexus = NexusClient("http://localhost:15474")
llm = OpenAI()

# Search for context
query_vector = get_embedding("What is AI?")
results = await nexus.execute_cypher(
    "MATCH (doc:Document) WHERE doc.vector IS NOT NULL "
    "RETURN doc.content ORDER BY doc.vector <-> $v LIMIT 5",
    params={"v": query_vector}
)

# Generate response
response = llm.chat.completions.create(
    model="gpt-4",
    messages=[{"role": "user", "content": f"Context: {results.rows}\n\nQuestion: What is AI?"}]
)
```

### LangChain

```python
from langchain.llms import OpenAI
from nexus_sdk import NexusClient

nexus = NexusClient("http://localhost:15474")
llm = OpenAI()

# Use Nexus as vector store
# (Integration code here)
```

## Monitoring

### Prometheus

```yaml
scrape_configs:
  - job_name: 'nexus'
    static_configs:
      - targets: ['localhost:15474']
    metrics_path: '/metrics'
```

### Grafana

Import dashboard configuration for Nexus metrics visualization.

## Related Topics

- [API Reference](./API_REFERENCE.md) - REST API documentation
- [MCP Protocol](./MCP.md) - MCP integration
- [SDKs Guide](../sdks/README.md) - Client SDKs

