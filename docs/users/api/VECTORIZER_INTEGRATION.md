---
title: Vectorizer Integration
module: api
id: vectorizer-integration
order: 9
description: Vectorizer integration guide
tags: [vectorizer, integration, vector-search, hybrid]
---

# Vectorizer Integration

Complete guide for integrating Nexus with Vectorizer.

## Overview

Nexus integrates with Vectorizer for enhanced vector search capabilities, enabling hybrid search combining graph traversal with semantic similarity.

## Configuration

### Enable Integration

```yaml
vectorizer_integration:
  enabled: true
  vectorizer_url: "http://localhost:15002"
  api_key: "${VECTORIZER_API_KEY}"
```

### Connection Settings

```yaml
vectorizer_integration:
  connection:
    timeout_seconds: 30
    pool_size: 10
```

## Sync Settings

### Auto-Create Vectors

```yaml
vectorizer_integration:
  sync:
    mode: "async"
    worker_threads: 2
    batch_size: 50
    auto_create_vectors: true
    auto_update_vectors: true
```

## Collection Mappings

```yaml
vectorizer_integration:
  collections:
    Document: "documents"
    LegalDocument: "legal_documents"
    FinancialDocument: "financial_documents"
```

## Hybrid Search

### Using Cypher

```cypher
// Hybrid search with Vectorizer
CALL vectorizer.hybrid_search(
  'documents',
  'What is machine learning?',
  {k: 10, graph_traversal: true}
)
YIELD node, score
RETURN node.title, score
```

## Related Topics

- [Vector Search](../vector-search/) - Vector operations
- [API Reference](./API_REFERENCE.md) - REST API
- [Integration Guide](./INTEGRATION.md) - General integration

