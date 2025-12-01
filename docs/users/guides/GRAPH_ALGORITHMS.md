---
title: Graph Algorithms
module: guides
id: graph-algorithms
order: 1
description: Built-in graph algorithms and GDS procedures
tags: [algorithms, graph, pagerank, centrality, community, gds]
---

# Graph Algorithms

Complete guide for built-in graph algorithms in Nexus. Nexus provides 20 Neo4j GDS-compatible procedures for graph analytics.

## Quick Reference

| Procedure | Category | Description |
|-----------|----------|-------------|
| `gds.pageRank` | Centrality | PageRank algorithm |
| `gds.centrality.betweenness` | Centrality | Betweenness centrality |
| `gds.centrality.closeness` | Centrality | Closeness centrality |
| `gds.centrality.degree` | Centrality | Degree centrality |
| `gds.centrality.eigenvector` | Centrality | Eigenvector centrality |
| `gds.shortestPath.dijkstra` | Pathfinding | Dijkstra's shortest path |
| `gds.shortestPath.astar` | Pathfinding | A* with heuristic |
| `gds.shortestPath.yens` | Pathfinding | K shortest paths (Yen's algorithm) |
| `gds.louvain` | Community | Louvain modularity optimization |
| `gds.labelPropagation` | Community | Label propagation algorithm |
| `gds.wcc` | Community | Weakly connected components |
| `gds.scc` | Community | Strongly connected components |
| `gds.triangleCount` | Structure | Count triangles per node |
| `gds.localClusteringCoefficient` | Structure | Per-node clustering coefficient |
| `gds.globalClusteringCoefficient` | Structure | Graph-wide clustering coefficient |

## Centrality Algorithms

### PageRank

Measures the importance of nodes based on the link structure of the graph.

```cypher
CALL gds.pageRank('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10
```

**Parameters:**
- `label` (String): Node label to include
- `relationship_type` (String): Relationship type to traverse

**Returns:**
- `node`: The node
- `score`: PageRank score (0.0-1.0)

### Betweenness Centrality

Measures how often a node lies on the shortest path between other nodes.

```cypher
CALL gds.centrality.betweenness('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10
```

### Closeness Centrality

Measures the average distance from a node to all other nodes.

```cypher
CALL gds.centrality.closeness('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10
```

### Degree Centrality

Measures the number of connections a node has.

```cypher
CALL gds.centrality.degree('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10
```

### Eigenvector Centrality

Measures influence based on connections to other influential nodes.

```cypher
CALL gds.centrality.eigenvector('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10
```

## Pathfinding Algorithms

### Dijkstra Shortest Path

Finds the shortest path between two nodes using Dijkstra's algorithm.

```cypher
CALL gds.shortestPath.dijkstra('Person', 'KNOWS', $sourceId, $targetId)
YIELD path, cost
RETURN path, cost
```

**Parameters:**
- `label` (String): Node label
- `relationship_type` (String): Relationship type
- `source_id` (Integer): Source node ID
- `target_id` (Integer): Target node ID

**Returns:**
- `path`: List of node IDs in the path
- `cost`: Total path cost

### A* Shortest Path

Finds the shortest path using A* algorithm with heuristic.

```cypher
CALL gds.shortestPath.astar('Person', 'KNOWS', $sourceId, $targetId)
YIELD path, cost
RETURN path, cost
```

### K Shortest Paths (Yen's Algorithm)

Finds the K shortest paths between two nodes.

```cypher
CALL gds.shortestPath.yens('Person', 'KNOWS', $sourceId, $targetId, 5)
YIELD path, cost
RETURN path, cost
ORDER BY cost
```

**Parameters:**
- `label` (String): Node label
- `relationship_type` (String): Relationship type
- `source_id` (Integer): Source node ID
- `target_id` (Integer): Target node ID
- `k` (Integer): Number of paths to find

## Community Detection

### Louvain Algorithm

Detects communities using modularity optimization.

```cypher
CALL gds.louvain('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members
ORDER BY size(members) DESC
```

**Returns:**
- `node`: The node
- `community`: Community ID

### Label Propagation

Detects communities using label propagation.

```cypher
CALL gds.labelPropagation('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members
```

### Weakly Connected Components

Finds connected components (ignoring edge direction).

```cypher
CALL gds.wcc('Person', 'KNOWS')
YIELD node, componentId
RETURN componentId, count(*) AS size
ORDER BY size DESC
```

**Returns:**
- `node`: The node
- `componentId`: Component ID

### Strongly Connected Components

Finds strongly connected components (respecting edge direction).

```cypher
CALL gds.scc('Person', 'KNOWS')
YIELD node, componentId
RETURN componentId, count(*) AS size
ORDER BY size DESC
```

## Graph Structure Analysis

### Triangle Counting

Counts triangles that each node participates in.

```cypher
CALL gds.triangleCount('Person', 'KNOWS')
YIELD node, triangles
RETURN node.name, triangles
ORDER BY triangles DESC
```

**Returns:**
- `node`: The node
- `triangles`: Number of triangles

### Local Clustering Coefficient

Calculates the clustering coefficient for each node.

```cypher
CALL gds.localClusteringCoefficient('Person', 'KNOWS')
YIELD node, coefficient
RETURN node.name, coefficient
ORDER BY coefficient DESC
```

**Returns:**
- `node`: The node
- `coefficient`: Clustering coefficient (0.0-1.0)

### Global Clustering Coefficient

Calculates the global clustering coefficient for the entire graph.

```cypher
CALL gds.globalClusteringCoefficient('Person', 'KNOWS')
YIELD coefficient
RETURN coefficient
```

**Returns:**
- `coefficient`: Global clustering coefficient (0.0-1.0)

## Example Use Cases

### Find Influential Users

```cypher
// Combine PageRank with degree centrality
CALL gds.pageRank('Person', 'FOLLOWS')
YIELD node AS person, score AS pagerank
CALL gds.centrality.degree('Person', 'FOLLOWS')
YIELD node, score AS degree
WHERE node = person
RETURN person.name, pagerank, degree
ORDER BY pagerank DESC
LIMIT 20
```

### Detect Communities and Leaders

```cypher
// Find community leaders
CALL gds.louvain('Person', 'KNOWS')
YIELD node, community
WITH community, collect(node) AS members
UNWIND members AS member
CALL gds.centrality.betweenness('Person', 'KNOWS')
YIELD node, score
WHERE node = member
RETURN community, node.name AS leader, score
ORDER BY community, score DESC
```

### Find Connection Paths

```cypher
// Find multiple paths between people
MATCH (alice:Person {name: 'Alice'}), (bob:Person {name: 'Bob'})
CALL gds.shortestPath.yens('Person', 'KNOWS', id(alice), id(bob), 3)
YIELD path, cost
RETURN path, cost
ORDER BY cost
```

### Analyze Network Structure

```cypher
// Get graph metrics
CALL gds.globalClusteringCoefficient('Person', 'KNOWS')
YIELD coefficient AS clustering

CALL gds.wcc('Person', 'KNOWS')
YIELD componentId
WITH clustering, count(DISTINCT componentId) AS components

RETURN clustering, components
```

## Performance Considerations

1. **Large Graphs**: For graphs with millions of nodes, consider:
   - Running algorithms on subgraphs using label filters
   - Using LIMIT to restrict result size
   - Processing results in batches

2. **Memory Usage**: Some algorithms (like Louvain) may require significant memory for large graphs.

3. **Relationship Direction**: Some algorithms (like PageRank, SCC) consider edge direction while others (like WCC) don't.

4. **Weights**: Currently, algorithms use unweighted edges. Weighted versions planned for future releases.

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Query language reference
- [Performance Guide](./PERFORMANCE.md) - Performance optimization
- [Use Cases](../use-cases/) - Real-world examples
- [Functions Reference](../reference/FUNCTIONS.md) - All Cypher functions

