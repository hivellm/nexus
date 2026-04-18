---
title: Functions Reference
module: cypher
id: functions-reference
order: 3
description: All Cypher functions
tags: [functions, reference, cypher]
---

# Functions Reference

Complete reference for all Cypher functions in Nexus.

## String Functions

### Basic Operations

- `upper(str)` - Convert to uppercase
- `lower(str)` - Convert to lowercase
- `trim(str)` - Remove leading/trailing whitespace
- `left(str, length)` - Get left substring
- `right(str, length)` - Get right substring
- `substring(str, start, length)` - Get substring
- `replace(str, search, replacement)` - Replace substring

### Pattern Matching

- `STARTS WITH` - Check if string starts with pattern
- `ENDS WITH` - Check if string ends with pattern
- `CONTAINS` - Check if string contains pattern
- `=~` - Regex pattern matching

## Mathematical Functions

### Basic Math

- `abs(x)` - Absolute value
- `round(x)` - Round to nearest integer
- `ceil(x)` - Ceiling function
- `floor(x)` - Floor function
- `sqrt(x)` - Square root

### Trigonometric

- `sin(x)` - Sine
- `cos(x)` - Cosine
- `tan(x)` - Tangent
- `asin(x)` - Arc sine
- `acos(x)` - Arc cosine
- `atan(x)` - Arc tangent
- `atan2(y, x)` - Arc tangent of y/x

### Advanced Math

- `exp(x)` - Exponential function
- `log(x)` - Natural logarithm
- `log10(x)` - Base-10 logarithm
- `pi()` - Pi constant
- `e()` - Euler's number
- `radians(x)` - Convert degrees to radians
- `degrees(x)` - Convert radians to degrees

## Temporal Functions

### Current Date/Time

- `date()` - Current date
- `datetime()` - Current datetime
- `time()` - Current time
- `timestamp()` - Unix timestamp
- `localtime()` - Local time
- `localdatetime()` - Local datetime

### Component Extraction

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

### Duration Functions

- `years(duration)` - Extract years
- `months(duration)` - Extract months
- `weeks(duration)` - Extract weeks
- `days(duration)` - Extract days
- `hours(duration)` - Extract hours
- `minutes(duration)` - Extract minutes
- `seconds(duration)` - Extract seconds

## List Functions

### Basic Operations

- `size(list)` - List size
- `head(list)` - First element
- `last(list)` - Last element
- `tail(list)` - All but first element

### Transformations

- `extract(x IN list | expression)` - Transform list
- `filter(x IN list | condition)` - Filter list
- `flatten(list)` - Flatten nested lists
- `zip(list1, list2)` - Zip two lists

## Aggregation Functions

### Basic Aggregations

- `COUNT(expr)` - Count values
- `SUM(expr)` - Sum values
- `AVG(expr)` - Average values
- `MIN(expr)` - Minimum value
- `MAX(expr)` - Maximum value
- `COLLECT(expr)` - Collect into list

## Geospatial Functions

### Point Operations

- `point({x, y, crs})` - Create point
- `distance(point1, point2)` - Calculate distance

### Point Accessors

- `point.x` - X coordinate
- `point.y` - Y coordinate
- `point.z` - Z coordinate
- `point.latitude` - Latitude
- `point.longitude` - Longitude
- `point.crs` - Coordinate system

## GDS Procedures (Graph Algorithms)

Nexus provides 15 Neo4j GDS-compatible procedures for graph analytics.

### Centrality Algorithms

| Procedure | Description |
|-----------|-------------|
| `gds.pageRank(label, relType)` | PageRank algorithm |
| `gds.centrality.betweenness(label, relType)` | Betweenness centrality |
| `gds.centrality.closeness(label, relType)` | Closeness centrality |
| `gds.centrality.degree(label, relType)` | Degree centrality |
| `gds.centrality.eigenvector(label, relType)` | Eigenvector centrality |

### Pathfinding Algorithms

| Procedure | Description |
|-----------|-------------|
| `gds.shortestPath.dijkstra(label, relType, sourceId, targetId)` | Dijkstra's shortest path |
| `gds.shortestPath.astar(label, relType, sourceId, targetId)` | A* with heuristic |
| `gds.shortestPath.yens(label, relType, sourceId, targetId, k)` | K shortest paths |

### Community Detection

| Procedure | Description |
|-----------|-------------|
| `gds.louvain(label, relType)` | Louvain modularity optimization |
| `gds.labelPropagation(label, relType)` | Label propagation |
| `gds.wcc(label, relType)` | Weakly connected components |
| `gds.scc(label, relType)` | Strongly connected components |

### Graph Structure Analysis

| Procedure | Description |
|-----------|-------------|
| `gds.triangleCount(label, relType)` | Count triangles per node |
| `gds.localClusteringCoefficient(label, relType)` | Per-node clustering coefficient |
| `gds.globalClusteringCoefficient(label, relType)` | Graph-wide clustering coefficient |

### Example Usage

```cypher
// PageRank
CALL gds.pageRank('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC
LIMIT 10

// Shortest path
MATCH (alice:Person {name: 'Alice'}), (bob:Person {name: 'Bob'})
CALL gds.shortestPath.dijkstra('Person', 'KNOWS', id(alice), id(bob))
YIELD path, cost
RETURN path, cost

// Community detection
CALL gds.louvain('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members
```

See [Graph Algorithms Guide](../guides/GRAPH_ALGORITHMS.md) for detailed documentation.

## Related Topics

- [Cypher Basics](./BASIC.md) - Basic syntax
- [Advanced Cypher](./ADVANCED.md) - Advanced patterns
- [Complete Cypher Guide](./CYPHER.md) - Comprehensive reference

