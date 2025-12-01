---
title: Recommendation System
module: use-cases
id: recommendation-system
order: 2
description: Content-based recommendations
tags: [recommendation, examples, use-cases]
---

# Recommendation System

Complete guide for building a recommendation system with Nexus.

## Overview

Build a recommendation system that combines graph relationships with vector similarity.

## User-Item Graph

### Create Users and Items

```cypher
// Create users
CREATE 
  (user1:User {id: 1, name: "Alice"}),
  (user2:User {id: 2, name: "Bob"}),
  (user3:User {id: 3, name: "Charlie"})

// Create items
CREATE 
  (item1:Item {id: 1, title: "Movie A", genre: "Action"}),
  (item2:Item {id: 2, title: "Movie B", genre: "Comedy"}),
  (item3:Item {id: 3, title: "Movie C", genre: "Action"})
```

### Create Interactions

```cypher
// User likes items
CREATE 
  (user1:User {id: 1})-[:LIKES {rating: 5}]->(item1:Item {id: 1}),
  (user1:User {id: 1})-[:LIKES {rating: 4}]->(item2:Item {id: 2}),
  (user2:User {id: 2})-[:LIKES {rating: 5}]->(item1:Item {id: 1}),
  (user2:User {id: 2})-[:LIKES {rating: 5}]->(item3:Item {id: 3})
```

## Collaborative Filtering

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

## Content-Based Recommendations

### Add Item Vectors

```cypher
CREATE (item:Item {
  id: 1,
  title: "Movie A",
  vector: [0.1, 0.2, 0.3, 0.4]
})
```

### Find Similar Items

```cypher
MATCH (user:User {id: 1})-[:LIKES]->(liked:Item),
      (similar:Item)
WHERE liked.vector IS NOT NULL
  AND similar.vector IS NOT NULL
  AND similar.id <> liked.id
RETURN similar.title, 
       liked.vector <-> similar.vector AS similarity
ORDER BY similarity
LIMIT 10
```

## Hybrid Recommendations

### Combine Graph and Vector

```cypher
MATCH (user:User {id: 1})-[:LIKES]->(liked:Item),
      (similar:User)-[:LIKES]->(liked),
      (similar)-[:LIKES]->(recommended:Item)
WHERE recommended.vector IS NOT NULL
  AND user.vector IS NOT NULL
  AND NOT (user)-[:LIKES]->(recommended)
RETURN recommended.title,
       COUNT(similar) AS graph_score,
       user.vector <-> recommended.vector AS vector_score
ORDER BY graph_score DESC, vector_score
LIMIT 10
```

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Query language
- [Vector Search](../vector-search/) - Vector operations
- [Examples](./EXAMPLES.md) - More examples

