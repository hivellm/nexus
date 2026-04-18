---
title: Functions Reference
module: reference
id: functions-reference
order: 1
description: All Cypher functions
tags: [functions, reference, cypher]
---

# Functions Reference

Complete reference for all Cypher functions in Nexus.

## String Functions

- `upper(str)` - Convert to uppercase
- `lower(str)` - Convert to lowercase
- `trim(str)` - Remove leading/trailing whitespace
- `left(str, length)` - Get left substring
- `right(str, length)` - Get right substring
- `substring(str, start, length)` - Get substring
- `replace(str, search, replacement)` - Replace substring

## Mathematical Functions

- `abs(x)` - Absolute value
- `round(x)` - Round to nearest integer
- `ceil(x)` - Ceiling function
- `floor(x)` - Floor function
- `sqrt(x)` - Square root
- `sin(x)` - Sine
- `cos(x)` - Cosine
- `tan(x)` - Tangent
- `asin(x)` - Arc sine
- `acos(x)` - Arc cosine
- `atan(x)` - Arc tangent
- `atan2(y, x)` - Arc tangent of y/x
- `exp(x)` - Exponential function
- `log(x)` - Natural logarithm
- `log10(x)` - Base-10 logarithm
- `pi()` - Pi constant
- `e()` - Euler's number
- `radians(x)` - Convert degrees to radians
- `degrees(x)` - Convert radians to degrees

## Temporal Functions

- `date()` - Current date
- `datetime()` - Current datetime
- `time()` - Current time
- `timestamp()` - Unix timestamp
- `localtime()` - Local time
- `localdatetime()` - Local datetime
- `year(date)` - Extract year
- `month(date)` - Extract month
- `day(date)` - Extract day
- `hour(time)` - Extract hour
- `minute(time)` - Extract minute
- `second(time)` - Extract second
- `quarter(date)` - Extract quarter
- `week(date)` - Extract week
- `dayOfWeek(date)` - Day of week (1-7)
- `dayOfYear(date)` - Day of year (1-365)

## List Functions

- `size(list)` - List size
- `head(list)` - First element
- `last(list)` - Last element
- `extract(x IN list | expression)` - Transform list
- `filter(x IN list | condition)` - Filter list
- `flatten(list)` - Flatten nested lists
- `zip(list1, list2)` - Zip two lists

## Aggregation Functions

- `COUNT(expr)` - Count values
- `SUM(expr)` - Sum values
- `AVG(expr)` - Average values
- `MIN(expr)` - Minimum value
- `MAX(expr)` - Maximum value
- `COLLECT(expr)` - Collect into list

## Geospatial Functions

- `point({x, y, crs})` - Create point
- `distance(point1, point2)` - Calculate distance
- `point.x` - X coordinate
- `point.y` - Y coordinate
- `point.z` - Z coordinate
- `point.latitude` - Latitude
- `point.longitude` - Longitude
- `point.crs` - Coordinate system

## GDS Procedures (Graph Algorithms)

Nexus provides 15 Neo4j GDS-compatible procedures for graph analytics.

### Centrality

- `gds.pageRank(label, relType)` - PageRank algorithm
- `gds.centrality.betweenness(label, relType)` - Betweenness centrality
- `gds.centrality.closeness(label, relType)` - Closeness centrality
- `gds.centrality.degree(label, relType)` - Degree centrality
- `gds.centrality.eigenvector(label, relType)` - Eigenvector centrality

### Pathfinding

- `gds.shortestPath.dijkstra(label, relType, sourceId, targetId)` - Dijkstra's shortest path
- `gds.shortestPath.astar(label, relType, sourceId, targetId)` - A* with heuristic
- `gds.shortestPath.yens(label, relType, sourceId, targetId, k)` - K shortest paths (Yen's algorithm)

### Community Detection

- `gds.louvain(label, relType)` - Louvain modularity optimization
- `gds.labelPropagation(label, relType)` - Label propagation algorithm
- `gds.wcc(label, relType)` - Weakly connected components
- `gds.scc(label, relType)` - Strongly connected components

### Graph Structure

- `gds.triangleCount(label, relType)` - Count triangles per node
- `gds.localClusteringCoefficient(label, relType)` - Per-node clustering coefficient
- `gds.globalClusteringCoefficient(label, relType)` - Graph-wide clustering coefficient

See [Graph Algorithms Guide](../guides/GRAPH_ALGORITHMS.md) for detailed documentation and examples.

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language
- [Data Types](./DATA_TYPES.md) - Supported data types
- [Error Reference](./ERRORS.md) - Error codes

