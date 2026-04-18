---
title: RAG Application
module: use-cases
id: rag-application
order: 3
description: RAG-based applications
tags: [rag, examples, use-cases, llm]
---

# RAG Application

Complete guide for building RAG (Retrieval-Augmented Generation) applications with Nexus.

## Overview

RAG applications combine vector search with graph relationships to provide context for LLM responses.

## Document Indexing

### Create Documents with Vectors

```cypher
CREATE (doc:Document {
  id: 1,
  title: "Introduction to AI",
  content: "Artificial Intelligence is...",
  vector: [0.1, 0.2, 0.3, 0.4]
})
```

### Create Document Relationships

```cypher
CREATE 
  (doc1:Document {title: "AI Basics"})-[:REFERENCES]->(doc2:Document {title: "ML Advanced"}),
  (doc1:Document {title: "AI Basics"})-[:RELATED_TO]->(concept:Concept {name: "Machine Learning"})
```

## Semantic Search

### Basic Vector Search

```cypher
MATCH (doc:Document)
WHERE doc.vector IS NOT NULL
RETURN doc.title, doc.content
ORDER BY doc.vector <-> [0.15, 0.25, 0.35, 0.45]
LIMIT 5
```

### Search with Filters

```cypher
MATCH (doc:Document)
WHERE doc.vector IS NOT NULL
  AND doc.category = "Technical"
RETURN doc.title, doc.content
ORDER BY doc.vector <-> [0.15, 0.25, 0.35, 0.45]
LIMIT 5
```

## Hybrid Search

### Graph + Vector Search

```cypher
MATCH (doc:Document)-[:REFERENCES]->(related:Document)
WHERE doc.vector IS NOT NULL
  AND related.vector IS NOT NULL
RETURN doc.title, related.title,
       doc.vector <-> [0.15, 0.25, 0.35, 0.45] AS similarity
ORDER BY similarity
LIMIT 10
```

### Context Enrichment

```cypher
MATCH (doc:Document)-[:RELATED_TO]->(concept:Concept)
WHERE doc.vector IS NOT NULL
RETURN doc.title, 
       COLLECT(concept.name) AS related_concepts,
       doc.vector <-> [0.15, 0.25, 0.35, 0.45] AS similarity
ORDER BY similarity
LIMIT 5
```

## Integration with LLMs

### Python Example

```python
from nexus_sdk import NexusClient
from openai import OpenAI

nexus_client = NexusClient("http://localhost:15474")
llm_client = OpenAI()

# Search for relevant documents
query_vector = get_embedding("What is machine learning?")
results = nexus_client.execute_cypher(
    """
    MATCH (doc:Document)
    WHERE doc.vector IS NOT NULL
    RETURN doc.content
    ORDER BY doc.vector <-> $query_vector
    LIMIT 5
    """,
    params={"query_vector": query_vector}
)

# Build context
context = "\n\n".join([row[0] for row in results.rows])

# Generate response with context
response = llm_client.chat.completions.create(
    model="gpt-4",
    messages=[
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": f"Context: {context}\n\nQuestion: What is machine learning?"}
    ]
)
```

## Related Topics

- [Vector Search](../vector-search/) - Vector operations
- [Cypher Guide](../cypher/CYPHER.md) - Query language
- [Examples](./EXAMPLES.md) - More examples

