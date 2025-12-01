---
title: Use Cases
module: use-cases
id: use-cases-index
order: 0
description: Real-world examples and tutorials
tags: [use-cases, examples, tutorials]
---

# Use Cases

Real-world examples and tutorials for Nexus.

## Examples

### [Knowledge Graph](./KNOWLEDGE_GRAPH.md)

Build a knowledge graph:
- Entity relationships
- Property modeling
- Query patterns
- Best practices

### [Recommendation System](./RECOMMENDATION_SYSTEM.md)

Content-based recommendations:
- User-item relationships
- Vector similarity
- Hybrid queries
- Ranking algorithms

### [RAG Application](./RAG_APPLICATION.md)

Retrieval-Augmented Generation:
- Document indexing
- Vector embeddings
- Semantic search
- Context retrieval

### [Social Network](./SOCIAL_NETWORK.md)

Social network analysis:
- User relationships
- Community detection
- Path finding
- Graph algorithms

### [Practical Examples](./EXAMPLES.md)

Real-world code examples:
- Complete applications
- Integration patterns
- Best practices
- Performance tips

## Quick Examples

### Social Network

```cypher
// Create users
CREATE (alice:Person {name: "Alice"}),
       (bob:Person {name: "Bob"}),
       (charlie:Person {name: "Charlie"})

// Create relationships
CREATE (alice)-[:FOLLOWS]->(bob),
       (bob)-[:FOLLOWS]->(charlie)

// Find mutual connections
MATCH (a:Person)-[:FOLLOWS]->(b:Person),
      (b:Person)-[:FOLLOWS]->(c:Person)
RETURN a.name, b.name, c.name
```

### Recommendation System

```cypher
// Find similar users
MATCH (user:Person {id: 1})-[:LIKES]->(item:Item),
      (similar:Person)-[:LIKES]->(item)
WHERE similar.vector IS NOT NULL
  AND similar.vector <-> user.vector < 0.3
RETURN DISTINCT similar.name, 
       similar.vector <-> user.vector as similarity
ORDER BY similarity
LIMIT 10
```

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Query language
- [Vector Search](../vector-search/) - Vector operations
- [API Reference](../api/API_REFERENCE.md) - REST API

