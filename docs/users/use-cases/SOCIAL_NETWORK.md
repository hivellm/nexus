---
title: Social Network
module: use-cases
id: social-network
order: 4
description: Social network analysis
tags: [social-network, examples, use-cases]
---

# Social Network

Complete guide for building and analyzing social networks with Nexus.

## Overview

Social networks represent relationships between users, enabling analysis of connections, communities, and influence.

## Creating Users

### Basic Users

```cypher
CREATE 
  (alice:Person {name: "Alice", age: 30}),
  (bob:Person {name: "Bob", age: 28}),
  (charlie:Person {name: "Charlie", age: 35}),
  (diana:Person {name: "Diana", age: 32})
```

### Users with Properties

```cypher
CREATE 
  (alice:Person {
    name: "Alice",
    age: 30,
    city: "NYC",
    interests: ["tech", "music"]
  })
```

## Creating Relationships

### Friendships

```cypher
CREATE 
  (alice:Person {name: "Alice"})-[:FRIENDS {since: "2020"}]->(bob:Person {name: "Bob"}),
  (bob:Person {name: "Bob"})-[:FRIENDS {since: "2019"}]->(charlie:Person {name: "Charlie"}),
  (charlie:Person {name: "Charlie"})-[:FRIENDS {since: "2021"}]->(diana:Person {name: "Diana"})
```

### Follow Relationships

```cypher
CREATE 
  (alice:Person {name: "Alice"})-[:FOLLOWS]->(bob:Person {name: "Bob"}),
  (bob:Person {name: "Bob"})-[:FOLLOWS]->(charlie:Person {name: "Charlie"})
```

## Querying Social Network

### Find Friends

```cypher
MATCH (person:Person {name: "Alice"})-[:FRIENDS]->(friend:Person)
RETURN friend.name
```

### Find Mutual Friends

```cypher
MATCH (a:Person {name: "Alice"})-[:FRIENDS]->(mutual:Person)<-[:FRIENDS]-(b:Person {name: "Bob"})
RETURN mutual.name AS mutual_friend
```

### Find Shortest Path

```cypher
MATCH (a:Person {name: "Alice"}), 
      (b:Person {name: "Charlie"}),
      p = shortestPath((a)-[:FRIENDS*]-(b))
RETURN p
```

## Community Detection

### Find Communities

```cypher
CALL gds.community.louvain.stream()
YIELD node, communityId
RETURN communityId, COLLECT(node.name) AS members
ORDER BY SIZE(members) DESC
```

### Find Influencers

```cypher
CALL gds.centrality.pagerank.stream()
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10
```

## Related Topics

- [Graph Algorithms](../guides/GRAPH_ALGORITHMS.md) - Built-in algorithms
- [Cypher Guide](../cypher/CYPHER.md) - Query language
- [Examples](./EXAMPLES.md) - More examples

