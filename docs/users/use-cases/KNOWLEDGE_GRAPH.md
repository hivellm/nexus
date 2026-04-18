---
title: Knowledge Graph
module: use-cases
id: knowledge-graph
order: 1
description: Build a knowledge graph
tags: [knowledge-graph, examples, use-cases]
---

# Knowledge Graph

Complete guide for building a knowledge graph with Nexus.

## Overview

Knowledge graphs represent entities and their relationships, enabling powerful semantic queries and reasoning.

## Creating Entities

### Basic Entities

```cypher
// Create concepts
CREATE 
  (ai:Concept {name: "Artificial Intelligence"}),
  (ml:Concept {name: "Machine Learning"}),
  (dl:Concept {name: "Deep Learning"}),
  (nn:Concept {name: "Neural Networks"})
```

### Entities with Properties

```cypher
CREATE 
  (paper:Document {
    title: "Attention Is All You Need",
    authors: ["Vaswani", "Shazeer"],
    year: 2017,
    citations: 50000
  }),
  (concept:Concept {
    name: "Transformer",
    description: "Neural network architecture"
  })
```

## Creating Relationships

### Hierarchical Relationships

```cypher
// Create hierarchy
CREATE 
  (ai:Concept {name: "AI"})-[:INCLUDES]->(ml:Concept {name: "ML"}),
  (ml:Concept {name: "ML"})-[:INCLUDES]->(dl:Concept {name: "DL"}),
  (dl:Concept {name: "DL"})-[:USES]->(nn:Concept {name: "NN"})
```

### Document Relationships

```cypher
// Link documents to concepts
MATCH (paper:Document {title: "Attention Is All You Need"}),
      (concept:Concept {name: "Transformer"})
CREATE (paper)-[:DESCRIBES]->(concept)
```

## Querying Knowledge Graph

### Find Related Concepts

```cypher
// Find all concepts related to AI
MATCH (ai:Concept {name: "AI"})-[:INCLUDES|USES*1..2]->(related:Concept)
RETURN DISTINCT related.name
```

### Find Paths Between Concepts

```cypher
MATCH (a:Concept {name: "AI"}), 
      (b:Concept {name: "Neural Networks"}),
      p = shortestPath((a)-[:INCLUDES|USES*]-(b))
RETURN p
```

### Find Documents About Concept

```cypher
MATCH (concept:Concept {name: "Transformer"})<-[:DESCRIBES]-(doc:Document)
RETURN doc.title, doc.authors, doc.year
ORDER BY doc.citations DESC
```

## Vector-Enhanced Knowledge Graph

### Add Embeddings

```cypher
CREATE (concept:Concept {
  name: "Transformer",
  description: "Neural network architecture",
  vector: [0.1, 0.2, 0.3, 0.4]
})
```

### Semantic Search

```cypher
MATCH (concept:Concept)
WHERE concept.vector IS NOT NULL
RETURN concept.name, concept.description
ORDER BY concept.vector <-> [0.1, 0.2, 0.3, 0.4]
LIMIT 5
```

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Query language
- [Vector Search](../vector-search/) - Vector operations
- [Examples](./EXAMPLES.md) - More examples

