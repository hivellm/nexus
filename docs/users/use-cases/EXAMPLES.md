---
title: Practical Examples
module: use-cases
id: practical-examples
order: 5
description: Real-world code examples
tags: [examples, code, tutorials, real-world]
---

# Practical Examples

Real-world code examples for Nexus.

## Social Network

### Create Users and Relationships

```cypher
// Create users
CREATE 
  (alice:Person {name: "Alice", age: 30}),
  (bob:Person {name: "Bob", age: 28}),
  (charlie:Person {name: "Charlie", age: 35}),
  (diana:Person {name: "Diana", age: 32})

// Create friendships
CREATE 
  (alice)-[:FRIENDS {since: "2020"}]->(bob),
  (bob)-[:FRIENDS {since: "2019"}]->(charlie),
  (charlie)-[:FRIENDS {since: "2021"}]->(diana),
  (alice)-[:FRIENDS {since: "2022"}]->(diana)
```

### Find Mutual Friends

```cypher
MATCH (a:Person {name: "Alice"})-[:FRIENDS]->(mutual:Person)<-[:FRIENDS]-(b:Person {name: "Bob"})
RETURN mutual.name AS mutual_friend
```

### Find Shortest Path

```cypher
MATCH (a:Person {name: "Alice"}), (b:Person {name: "Charlie"}),
      p = shortestPath((a)-[:FRIENDS*]-(b))
RETURN p
```

## Recommendation System

### User-Item Graph

```cypher
// Create users and items
CREATE 
  (user1:User {id: 1, name: "Alice"}),
  (user2:User {id: 2, name: "Bob"}),
  (item1:Item {id: 1, title: "Movie A"}),
  (item2:Item {id: 2, title: "Movie B"}),
  (item3:Item {id: 3, title: "Movie C"})

// Create likes
CREATE 
  (user1)-[:LIKES {rating: 5}]->(item1),
  (user1)-[:LIKES {rating: 4}]->(item2),
  (user2)-[:LIKES {rating: 5}]->(item1),
  (user2)-[:LIKES {rating: 5}]->(item3)
```

### Find Similar Users

```cypher
MATCH (user:User {id: 1})-[:LIKES]->(item:Item),
      (similar:User)-[:LIKES]->(item)
WHERE similar.id <> user.id
RETURN similar.name, COUNT(item) AS common_items
ORDER BY common_items DESC
LIMIT 10
```

### Recommend Items

```cypher
MATCH (user:User {id: 1})-[:LIKES]->(item:Item)<-[:LIKES]-(similar:User),
      (similar)-[:LIKES]->(recommended:Item)
WHERE NOT (user)-[:LIKES]->(recommended)
RETURN recommended.title, COUNT(similar) AS recommendation_score
ORDER BY recommendation_score DESC
LIMIT 10
```

## Knowledge Graph

### Create Entities and Relationships

```cypher
// Create entities
CREATE 
  (ai:Concept {name: "Artificial Intelligence"}),
  (ml:Concept {name: "Machine Learning"}),
  (dl:Concept {name: "Deep Learning"}),
  (nn:Concept {name: "Neural Networks"})

// Create relationships
CREATE 
  (ai)-[:INCLUDES]->(ml),
  (ml)-[:INCLUDES]->(dl),
  (dl)-[:USES]->(nn),
  (ml)-[:USES]->(nn)
```

### Find Related Concepts

```cypher
MATCH (concept:Concept {name: "Machine Learning"})-[:INCLUDES|USES*1..2]->(related:Concept)
RETURN DISTINCT related.name
```

## RAG Application

### Document Indexing

```cypher
// Create documents with vectors
CREATE 
  (doc1:Document {
    id: 1,
    title: "Introduction to AI",
    content: "Artificial Intelligence is...",
    vector: [0.1, 0.2, 0.3, 0.4]
  }),
  (doc2:Document {
    id: 2,
    title: "Machine Learning Basics",
    content: "Machine Learning is...",
    vector: [0.2, 0.3, 0.4, 0.5]
  })
```

### Semantic Search

```cypher
MATCH (doc:Document)
WHERE doc.vector IS NOT NULL
RETURN doc.title, doc.content
ORDER BY doc.vector <-> [0.15, 0.25, 0.35, 0.45]
LIMIT 5
```

### Hybrid Search (Graph + Vector)

```cypher
MATCH (doc:Document)-[:REFERENCES]->(related:Document)
WHERE doc.vector IS NOT NULL
  AND related.vector IS NOT NULL
RETURN doc.title, related.title,
       doc.vector <-> [0.15, 0.25, 0.35, 0.45] AS similarity
ORDER BY similarity
LIMIT 10
```

## Python SDK Examples

### Basic Usage

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")

# Execute query
result = client.execute_cypher(
    "MATCH (n:Person) RETURN n.name, n.age LIMIT 10"
)
print(result.rows)

# Create node
result = client.execute_cypher(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    params={"name": "Alice", "age": 30}
)
```

### Vector Search

```python
# Create node with vector
client.execute_cypher(
    "CREATE (n:Person {name: $name, vector: $vector}) RETURN n",
    params={
        "name": "Alice",
        "vector": [0.1, 0.2, 0.3, 0.4]
    }
)

# Search similar vectors
result = client.execute_cypher(
    """
    MATCH (n:Person)
    WHERE n.vector IS NOT NULL
    RETURN n.name, n.vector
    ORDER BY n.vector <-> $query_vector
    LIMIT 5
    """,
    params={"query_vector": [0.1, 0.2, 0.3, 0.4]}
)
```

## TypeScript SDK Examples

### Basic Usage

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474'
});

// Execute query
const result = await client.executeCypher(
  'MATCH (n:Person) RETURN n.name, n.age LIMIT 10'
);
console.log(result.rows);
```

### Vector Search

```typescript
// Create node with vector
await client.executeCypher(
  'CREATE (n:Person {name: $name, vector: $vector}) RETURN n',
  {
    name: 'Alice',
    vector: [0.1, 0.2, 0.3, 0.4]
  }
);

// Search similar vectors
const result = await client.executeCypher(
  `MATCH (n:Person)
   WHERE n.vector IS NOT NULL
   RETURN n.name, n.vector
   ORDER BY n.vector <-> $query_vector
   LIMIT 5`,
  { query_vector: [0.1, 0.2, 0.3, 0.4] }
);
```

## Related Topics

- [Knowledge Graph](./KNOWLEDGE_GRAPH.md) - Knowledge graph examples
- [Recommendation System](./RECOMMENDATION_SYSTEM.md) - Recommendation examples
- [RAG Application](./RAG_APPLICATION.md) - RAG examples
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language

