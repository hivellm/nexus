# Nexus Graph Database - User Guide

## Table of Contents

1. [Introduction](#introduction)
2. [Quick Start](#quick-start)
3. [Basic Operations](#basic-operations)
4. [Cypher Query Language](#cypher-query-language)
5. [KNN Vector Search](#knn-vector-search)
6. [API Reference](#api-reference)
7. [Performance Tips](#performance-tips)
8. [Examples](#examples)
9. [Troubleshooting](#troubleshooting)

## Introduction

Nexus is a high-performance graph database with native support for vector similarity search. It combines the power of graph traversal with k-nearest neighbor (KNN) search capabilities, making it ideal for applications requiring both relationship analysis and semantic similarity.

### Key Features

- **Graph Database**: Store and query complex relationships
- **Vector Search**: Native KNN support for similarity queries
- **Cypher Support**: Industry-standard query language
- **REST API**: Easy integration with any application
- **High Performance**: Optimized for both graph traversal and vector operations
- **ACID Transactions**: Data consistency and reliability

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/nexus.git
cd nexus

# Build the project
cargo build --release

# Run the server
cargo run --bin nexus-server
```

The server will start on `http://localhost:3000` by default.

### First Query

Let's create some data and run a simple query:

```bash
# Create a node
curl -X POST http://localhost:3000/data/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "labels": ["Person"],
    "properties": {
      "name": "Alice",
      "age": 30,
      "email": "alice@example.com"
    }
  }'

# Query the data
curl -X POST http://localhost:3000/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) RETURN n.name, n.age"
  }'
```

## Basic Operations

### Creating Nodes

Nodes represent entities in your graph. Each node can have multiple labels and properties.

```bash
# Create a person node
curl -X POST http://localhost:3000/data/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "labels": ["Person", "Developer"],
    "properties": {
      "name": "Bob",
      "age": 28,
      "skills": ["Rust", "Python", "Graph Databases"]
    }
  }'
```

### Creating Relationships

Relationships connect nodes and can also have properties.

```bash
# Create a relationship
curl -X POST http://localhost:3000/data/relationships \
  -H "Content-Type: application/json" \
  -d '{
    "src": 1,
    "dst": 2,
    "type": "WORKS_WITH",
    "properties": {
      "since": "2024-01-01",
      "project": "Nexus Database"
    }
  }'
```

### Bulk Data Ingestion

For large datasets, use the bulk ingest endpoint:

```bash
curl -X POST http://localhost:3000/ingest \
  -H "Content-Type: application/json" \
  -d '{
    "nodes": [
      {
        "labels": ["Person"],
        "properties": {"name": "Alice", "age": 30}
      },
      {
        "labels": ["Person"],
        "properties": {"name": "Bob", "age": 28}
      }
    ],
    "relationships": [
      {
        "src": 1,
        "dst": 2,
        "type": "KNOWS",
        "properties": {"since": "2020"}
      }
    ]
  }'
```

## Cypher Query Language

Nexus supports a subset of the Cypher query language. Here are the most common patterns:

### Basic Matching

```cypher
// Find all nodes
MATCH (n) RETURN n LIMIT 10

// Find nodes with specific label
MATCH (n:Person) RETURN n

// Find nodes with conditions
MATCH (n:Person) WHERE n.age > 25 RETURN n.name, n.age
```

### Relationship Traversal

```cypher
// Find direct relationships
MATCH (a:Person)-[r:KNOWS]->(b:Person) 
RETURN a.name, b.name, r.since

// Variable length paths
MATCH (a:Person)-[:KNOWS*2..3]->(b:Person) 
RETURN a.name, b.name

// Undirected relationships
MATCH (a:Person)-[:KNOWS]-(b:Person) 
RETURN a.name, b.name
```

### Aggregation

```cypher
// Count nodes
MATCH (n:Person) RETURN COUNT(n) as person_count

// Group by property
MATCH (n:Person) 
RETURN n.location, COUNT(n) as count 
ORDER BY count DESC

// Calculate averages
MATCH (n:Person) 
RETURN AVG(n.age) as avg_age, MAX(n.age) as max_age
```

### Complex Patterns

```cypher
// Find mutual connections
MATCH (a:Person)-[:KNOWS]->(b:Person), 
      (b:Person)-[:KNOWS]->(a:Person) 
RETURN a.name, b.name

// Find shortest path
MATCH (a:Person {name: "Alice"}), 
      (b:Person {name: "Bob"}), 
      p = shortestPath((a)-[:KNOWS*]-(b)) 
RETURN p
```

## KNN Vector Search

Nexus supports vector similarity search using the `<->` operator:

### Basic Vector Search

```cypher
// Find similar vectors
MATCH (n:Person) 
WHERE n.vector IS NOT NULL 
RETURN n.name, n.vector 
ORDER BY n.vector <-> [0.1, 0.2, 0.3, 0.4] 
LIMIT 5
```

### KNN with Threshold

```cypher
// Find vectors within similarity threshold
MATCH (n:Person) 
WHERE n.vector IS NOT NULL 
  AND n.vector <-> [0.1, 0.2, 0.3, 0.4] < 0.5 
RETURN n.name, n.vector
```

### Hybrid Queries

Combine graph traversal with vector search:

```cypher
// Find similar people who are connected
MATCH (a:Person)-[:KNOWS]->(b:Person) 
WHERE a.vector IS NOT NULL 
  AND b.vector IS NOT NULL 
RETURN a.name, b.name, 
       a.vector <-> b.vector as similarity 
ORDER BY similarity 
LIMIT 10
```

## API Reference

### Health Check

```bash
GET /health
```

Returns server status and version information.

### Cypher Query

```bash
POST /cypher
Content-Type: application/json

{
  "query": "MATCH (n) RETURN n LIMIT 10",
  "params": {},
  "timeout_ms": 5000
}
```

### KNN Traversal

```bash
POST /knn_traverse
Content-Type: application/json

{
  "label": "Person",
  "vector": [0.1, 0.2, 0.3, 0.4],
  "k": 10,
  "where": "n.age > 25",
  "limit": 100
}
```

### Bulk Ingest

```bash
POST /ingest
Content-Type: application/json

{
  "nodes": [...],
  "relationships": [...]
}
```

### Schema Management

```bash
# Create label
POST /schema/labels
{"name": "Person"}

# List labels
GET /schema/labels

# Create relationship type
POST /schema/rel_types
{"name": "KNOWS"}

# List relationship types
GET /schema/rel_types
```

### Data Management

```bash
# Create node
POST /data/nodes
{"labels": ["Person"], "properties": {...}}

# Update node
PUT /data/nodes
{"id": 1, "properties": {...}}

# Delete node
DELETE /data/nodes
{"id": 1}
```

### Statistics

```bash
GET /stats
```

Returns database statistics including node counts, relationship counts, and index information.

## Performance Tips

### Query Optimization

1. **Use LIMIT**: Always limit result sets for better performance
2. **Index Properties**: Create indexes on frequently queried properties
3. **Avoid Cartesian Products**: Be careful with multiple MATCH clauses
4. **Use WHERE Early**: Filter data as early as possible in your query

### Vector Search Optimization

1. **Dimension Size**: Keep vector dimensions reasonable (64-512 dimensions)
2. **Batch Queries**: Use bulk operations for multiple vector searches
3. **Threshold Filtering**: Use similarity thresholds to reduce computation

### Memory Management

1. **Monitor Memory**: Use `/stats` endpoint to monitor memory usage
2. **Batch Operations**: Use bulk ingest for large datasets
3. **Connection Pooling**: Reuse HTTP connections when possible

## Examples

### Social Network Analysis

```cypher
// Find most connected users
MATCH (n:Person)-[r:KNOWS]-() 
RETURN n.name, COUNT(r) as connections 
ORDER BY connections DESC 
LIMIT 10

// Find communities (users with mutual connections)
MATCH (a:Person)-[:KNOWS]->(b:Person), 
      (b:Person)-[:KNOWS]->(c:Person), 
      (c:Person)-[:KNOWS]->(a:Person) 
RETURN a.name, b.name, c.name
```

### Recommendation System

```cypher
// Find similar users based on vector similarity
MATCH (user:Person {id: 1})-[:LIKES]->(item:Item), 
      (similar:Person)-[:LIKES]->(item) 
WHERE similar.vector IS NOT NULL 
  AND similar.vector <-> user.vector < 0.3 
RETURN DISTINCT similar.name, 
       similar.vector <-> user.vector as similarity 
ORDER BY similarity 
LIMIT 10
```

### Knowledge Graph Queries

```cypher
// Find entities connected to a concept
MATCH (entity:Entity)-[:RELATED_TO]->(concept:Concept {name: "Machine Learning"}) 
RETURN entity.name, entity.type

// Find paths between concepts
MATCH (a:Concept {name: "AI"}), 
      (b:Concept {name: "Neural Networks"}), 
      p = shortestPath((a)-[:RELATED_TO*]-(b)) 
RETURN p
```

## Troubleshooting

### Common Issues

#### Query Timeout

```bash
# Increase timeout
curl -X POST http://localhost:3000/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n) RETURN n",
    "timeout_ms": 10000
  }'
```

#### Memory Issues

```bash
# Check memory usage
curl http://localhost:3000/stats
```

#### Vector Dimension Mismatch

```bash
# Check KNN index dimension
curl http://localhost:3000/stats | jq '.knn_index.dimension'
```

### Error Codes

- `400 Bad Request`: Invalid query syntax or parameters
- `408 Request Timeout`: Query exceeded timeout limit
- `500 Internal Server Error`: Server-side error

### Debugging Tips

1. **Start Simple**: Begin with basic queries and add complexity gradually
2. **Use EXPLAIN**: Add `EXPLAIN` to queries to see execution plan
3. **Monitor Logs**: Check server logs for detailed error information
4. **Test Incrementally**: Test each part of complex queries separately

### Getting Help

- Check the [API documentation](docs/api/)
- Review [performance benchmarks](examples/benchmarks/)
- Look at [example datasets](examples/datasets/)
- Run [test suites](examples/cypher_tests/)

## Advanced Features

### Server-Sent Events (SSE)

For real-time data streaming:

```bash
# Stream query results
curl http://localhost:3000/sse/cypher?query=MATCH%20(n)%20RETURN%20n&interval=1000&limit=100

# Stream statistics
curl http://localhost:3000/sse/stats?interval=5000&limit=50

# Heartbeat
curl http://localhost:3000/sse/heartbeat?interval=10000&limit=1000
```

### MCP Integration

Nexus supports the Model Context Protocol (MCP) for AI integration:

```bash
# MCP endpoint
POST /mcp/call_tool
{
  "name": "execute_cypher",
  "arguments": {
    "query": "MATCH (n) RETURN n LIMIT 5"
  }
}
```

This completes the user guide. For more detailed information, refer to the API documentation and example code in the repository.
