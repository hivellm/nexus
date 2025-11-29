# Nexus Graph Database - User Guide

## Table of Contents

1. [Introduction](#introduction)
2. [Quick Start](#quick-start)
3. [Multi-Database Support](#multi-database-support)
4. [Basic Operations](#basic-operations)
5. [Cypher Query Language](#cypher-query-language)
6. [User-Defined Functions (UDFs) and Procedures](#user-defined-functions-udfs-and-procedures)
7. [KNN Vector Search](#knn-vector-search)
8. [Graph Comparison](#graph-comparison)
9. [API Reference](#api-reference)
10. [Performance Tips](#performance-tips)
11. [Examples](#examples)
12. [Troubleshooting](#troubleshooting)

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

## Multi-Database Support

Nexus supports multiple databases within a single server instance, enabling data isolation, multi-tenancy, and logical separation of different data sets.

### Key Concepts

- **Database**: An isolated data store with its own nodes, relationships, and indexes
- **Default Database**: The `neo4j` database is created automatically and is the default
- **Session Database**: Each session maintains a current database context
- **Data Isolation**: Each database is completely isolated from others

### Managing Databases with Cypher

#### List All Databases

```cypher
SHOW DATABASES
```

Returns a list of all databases with their status:

| name | status |
|------|--------|
| neo4j | online |
| mydb | online |

#### Create a Database

```cypher
CREATE DATABASE mydb
```

Database names must be alphanumeric with underscores and hyphens (e.g., `my_database`, `test-db`).

#### Drop a Database

```cypher
DROP DATABASE mydb
```

**Warning**: This permanently deletes all data in the database.

#### Switch Database

Use the `:USE` command to switch to a different database:

```cypher
:USE mydb
```

After switching, all subsequent queries will execute against the selected database.

#### Get Current Database

```cypher
RETURN database() AS current_db
```

Or use the alias:

```cypher
RETURN db() AS current_db
```

### REST API Endpoints

#### List Databases

```bash
GET /databases
```

Response:
```json
{
  "databases": [
    {
      "name": "neo4j",
      "path": "data/neo4j",
      "created_at": 1700000000,
      "node_count": 1000,
      "relationship_count": 5000,
      "storage_size": 1048576
    }
  ],
  "default_database": "neo4j"
}
```

#### Create Database

```bash
POST /databases
Content-Type: application/json

{
  "name": "mydb"
}
```

#### Get Database Info

```bash
GET /databases/mydb
```

#### Drop Database

```bash
DELETE /databases/mydb
```

#### Get Current Session Database

```bash
GET /session/database
```

Response:
```json
{
  "database": "neo4j"
}
```

#### Switch Session Database

```bash
PUT /session/database
Content-Type: application/json

{
  "name": "mydb"
}
```

### SDK Examples

#### Python SDK

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")

# List databases
response = client.list_databases()
for db in response.databases:
    print(f"Database: {db.name}, Nodes: {db.node_count}")

# Create a database
client.create_database("mydb")

# Switch to the database
client.switch_database("mydb")

# Get current database
current = client.get_current_database()
print(f"Current database: {current}")

# Drop database
client.drop_database("mydb")
```

#### TypeScript SDK

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474',
  auth: { apiKey: 'your-api-key' }
});

// List databases
const { databases, defaultDatabase } = await client.listDatabases();
databases.forEach(db => console.log(`Database: ${db.name}`));

// Create a database
await client.createDatabase('mydb');

// Switch to the database
await client.switchDatabase('mydb');

// Get current database
const current = await client.getCurrentDatabase();
console.log(`Current database: ${current}`);

// Drop database
await client.dropDatabase('mydb');
```

#### Rust SDK

```rust
use nexus_sdk::NexusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NexusClient::new("http://localhost:15474")?;

    // List databases
    let response = client.list_databases().await?;
    for db in &response.databases {
        println!("Database: {}, Nodes: {}", db.name, db.node_count);
    }

    // Create a database
    client.create_database("mydb").await?;

    // Switch to the database
    client.switch_database("mydb").await?;

    // Get current database
    let current = client.get_current_database().await?;
    println!("Current database: {}", current);

    // Drop database
    client.drop_database("mydb").await?;

    Ok(())
}
```

### Best Practices

1. **Use Meaningful Names**: Choose descriptive database names (e.g., `production`, `staging`, `test`)
2. **Separate Environments**: Use different databases for development, testing, and production data
3. **Multi-Tenancy**: Use separate databases for each tenant's data
4. **Backup Strategy**: Implement backup procedures for each database independently
5. **Avoid Dropping Active Databases**: Switch to a different database before dropping

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

## User-Defined Functions (UDFs) and Procedures

Nexus supports user-defined functions (UDFs) and custom procedures for extending query capabilities.

### User-Defined Functions (UDFs)

UDFs allow you to create custom functions that can be used in Cypher expressions. UDFs are persisted in the catalog and survive server restarts.

#### Registering a UDF

UDFs can be registered programmatically using the Rust API:

```rust
use nexus_core::udf::{BuiltinUdf, UdfSignature, UdfReturnType, UdfParameter};
use nexus_core::catalog::Catalog;
use nexus_core::udf::registry::PersistentUdfRegistry;
use std::sync::Arc;

let catalog = Arc::new(Catalog::new("./data/catalog")?);
let registry = PersistentUdfRegistry::new(catalog.clone());

let signature = UdfSignature {
    name: "multiply".to_string(),
    parameters: vec![
        UdfParameter {
            name: "a".to_string(),
            param_type: UdfReturnType::Integer,
            required: true,
            default: None,
        },
        UdfParameter {
            name: "b".to_string(),
            param_type: UdfReturnType::Integer,
            required: true,
            default: None,
        },
    ],
    return_type: UdfReturnType::Integer,
    description: Some("Multiply two integers".to_string()),
};

let udf = BuiltinUdf::new(signature, |args| {
    let a = args[0].as_i64().unwrap();
    let b = args[1].as_i64().unwrap();
    Ok(Value::Number((a * b).into()))
});

registry.register(Arc::new(udf))?;
```

#### Using UDFs in Cypher

Once registered, UDFs can be used in Cypher queries:

```cypher
// Use custom UDF in RETURN clause
MATCH (n:Person)
RETURN n.name, multiply(n.age, 2) AS double_age
```

#### UDF Return Types

UDFs support multiple return types:
- `Integer`: Integer values
- `Float`: Floating-point values
- `String`: String values
- `Boolean`: Boolean values
- `Any`: Dynamic type
- `List(T)`: Lists of a specific type
- `Map`: Map/dictionary values

#### UDF Persistence

UDF signatures are automatically persisted to the catalog when registered. When the server restarts, UDF signatures are loaded from the catalog, but function implementations need to be re-registered.

#### Managing UDFs via Cypher

You can also manage UDF signatures using Cypher commands:

**Create Function Signature:**
```cypher
CREATE FUNCTION multiply(a: Integer, b: Integer) RETURNS Integer AS 'Multiply two integers'
CREATE FUNCTION IF NOT EXISTS add(a: Integer, b: Integer) RETURNS Integer
```

**Show Functions:**
```cypher
SHOW FUNCTIONS
```

**Drop Function:**
```cypher
DROP FUNCTION multiply
DROP FUNCTION IF EXISTS multiply
```

**Note:** `CREATE FUNCTION` only stores the function signature. The actual function implementation must be registered via the API or plugin system using `register_udf()`.

### Custom Procedures

Procedures extend Cypher with custom algorithms and operations. They can be called using the `CALL` statement.

#### Registering a Custom Procedure

```rust
use nexus_core::graph::procedures::{CustomProcedure, ProcedureParameter, ParameterType, ProcedureRegistry};
use nexus_core::catalog::Catalog;
use std::sync::Arc;

let catalog = Arc::new(Catalog::new("./data/catalog")?);
let registry = ProcedureRegistry::with_catalog(catalog.clone());

let procedure = CustomProcedure::new(
    "custom.add".to_string(),
    vec![
        ProcedureParameter {
            name: "a".to_string(),
            param_type: ParameterType::Integer,
            required: true,
            default: None,
        },
        ProcedureParameter {
            name: "b".to_string(),
            param_type: ParameterType::Integer,
            required: true,
            default: None,
        },
    ],
    |_graph, args| {
        let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
        let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
        Ok(ProcedureResult {
            columns: vec!["sum".to_string()],
            rows: vec![vec![Value::Number((a + b).into())]],
        })
    },
);

registry.register_custom(procedure)?;
```

#### Using Procedures in Cypher

```cypher
// Call custom procedure
CALL custom.add(10, 20) YIELD sum
RETURN sum

// Call with YIELD to select specific columns
CALL custom.add(5, 7) YIELD sum
RETURN sum AS result
```

#### Built-in Procedures

Nexus includes built-in graph algorithm procedures:

- **Shortest Path**: `gds.shortestPath.dijkstra`, `gds.shortestPath.astar`, `gds.shortestPath.bellmanFord`
- **Centrality**: `gds.centrality.pagerank`, `gds.centrality.betweenness`, `gds.centrality.closeness`, `gds.centrality.degree`
- **Community Detection**: `gds.community.louvain`, `gds.community.labelPropagation`, `gds.community.stronglyConnectedComponents`, `gds.community.weaklyConnectedComponents`
- **Similarity**: `gds.similarity.jaccard`, `gds.similarity.cosine`

#### Procedure Persistence

Custom procedure signatures are automatically persisted to the catalog. Built-in procedures cannot be unregistered.

#### Streaming Results

Procedures can support streaming results for better memory efficiency with large result sets. When a procedure supports streaming, results are processed row-by-row instead of loading everything into memory at once.

To enable streaming for a custom procedure, override the `supports_streaming` method:

```rust
impl GraphProcedure for MyCustomProcedure {
    // ... other methods ...
    
    fn supports_streaming(&self) -> bool {
        true  // Enable streaming for this procedure
    }
    
    fn execute_streaming(
        &self,
        graph: &Graph,
        args: &HashMap<String, Value>,
        mut callback: Box<dyn FnMut(&[String], &[Value]) -> Result<()> + Send>,
    ) -> Result<()> {
        // Stream results as they are generated
        for result_row in self.generate_results(graph, args)? {
            callback(&self.output_columns(), &result_row)?;
        }
        Ok(())
    }
}
```

The executor automatically uses streaming when available, providing better performance for large result sets.

## Plugin System

Nexus provides a plugin system that allows extending functionality with custom UDFs, procedures, and other extensions. Plugins are loaded at runtime and can register their extensions during initialization.

### Creating a Plugin

To create a plugin, implement the `Plugin` trait:

```rust
use nexus_core::plugin::{Plugin, PluginContext, PluginResult};
use nexus_core::udf::{BuiltinUdf, UdfSignature, UdfReturnType};
use std::sync::Arc;

#[derive(Debug)]
pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my_plugin"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn initialize(&self, ctx: &mut PluginContext) -> PluginResult<()> {
        // Register a custom UDF
        let udf = BuiltinUdf::new(
            UdfSignature {
                name: "plugin_function".to_string(),
                parameters: vec![],
                return_type: UdfReturnType::Integer,
                description: Some("Function from plugin".to_string()),
            },
            |_args| Ok(serde_json::Value::Number(42.into())),
        );
        ctx.register_udf(Arc::new(udf))?;
        
        // Register a custom procedure
        use nexus_core::graph::procedures::{CustomProcedure, ProcedureParameter, ParameterType};
        let procedure = CustomProcedure::new(
            "plugin.procedure".to_string(),
            vec![],
            |_graph, _args| {
                Ok(nexus_core::graph::procedures::ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![serde_json::Value::String("Hello from plugin".to_string())]],
                })
            },
        );
        ctx.register_procedure(procedure)?;
        
        Ok(())
    }

    fn shutdown(&self) -> PluginResult<()> {
        // Clean up resources if needed
        Ok(())
    }
}
```

### Loading Plugins

Plugins are loaded using the `PluginManager`:

```rust
use nexus_core::plugin::PluginManager;
use nexus_core::udf::UdfRegistry;
use nexus_core::graph::procedures::ProcedureRegistry;
use nexus_core::catalog::Catalog;
use std::sync::Arc;

// Create registries
let udf_registry = Arc::new(UdfRegistry::new());
let procedure_registry = Arc::new(ProcedureRegistry::new());
let catalog = Arc::new(Catalog::new("./data/catalog")?);

// Create plugin manager with registries
let manager = PluginManager::with_registries(
    Some(udf_registry.clone()),
    Some(procedure_registry.clone()),
    Some(catalog.clone()),
);

// Load plugin
let plugin = Arc::new(MyPlugin);
manager.load_plugin(plugin)?;

// Plugin extensions are now available
assert!(udf_registry.contains("plugin_function"));
assert!(procedure_registry.contains("plugin.procedure"));
```

### Plugin Lifecycle

1. **Initialization**: When a plugin is loaded, `initialize()` is called. Plugins should register their extensions here.

2. **Runtime**: Plugins remain loaded and their extensions are available for use.

3. **Shutdown**: When a plugin is unloaded, `shutdown()` is called. Plugins should clean up resources here.

### Plugin Management

```rust
// List all loaded plugins
let plugins = manager.list_plugins();

// Check if a plugin is loaded
if manager.is_loaded("my_plugin") {
    // Plugin is loaded
}

// Get plugin metadata
if let Some(metadata) = manager.get_metadata("my_plugin") {
    println!("Plugin: {} v{}", metadata.name, metadata.version);
}

// Unload a plugin
manager.unload_plugin("my_plugin")?;

// Shutdown all plugins
manager.shutdown_all()?;
```

### Plugin Context

The `PluginContext` provides access to registries during plugin initialization:

- `register_udf()`: Register a UDF
- `register_procedure()`: Register a custom procedure
- `catalog()`: Access the catalog (if available)
- `udf_registry()`: Access the UDF registry (if available)
- `procedure_registry()`: Access the procedure registry (if available)

### Dynamic Loading

Currently, plugins must be statically linked. Dynamic loading from shared libraries (.so/.dll) is planned for future releases.

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

## Graph Comparison

Nexus provides powerful graph comparison and diff functionality to analyze changes between different graph states or versions.

### Basic Graph Comparison

Compare two graphs and get a detailed diff:

```bash
POST /comparison/compare
Content-Type: application/json

{
  "options": {
    "include_property_changes": true,
    "include_label_changes": true,
    "include_structural_changes": true
  }
}
```

### Advanced Graph Comparison

Get comprehensive analysis with similarity scores and detailed reporting:

```bash
POST /comparison/advanced
Content-Type: application/json

{
  "options": {
    "use_fuzzy_matching": true,
    "fuzzy_threshold": 0.8,
    "include_topology_analysis": true,
    "calculate_metrics": true,
    "include_temporal_analysis": false
  },
  "include_detailed_analysis": true,
  "generate_report": true
}
```

### Comparison Options

- **include_property_changes**: Track property modifications
- **include_label_changes**: Track label additions/removals
- **include_structural_changes**: Track edge endpoint changes
- **use_fuzzy_matching**: Enable fuzzy matching for similar nodes/edges
- **fuzzy_threshold**: Similarity threshold for fuzzy matching (0.0-1.0)
- **include_topology_analysis**: Analyze connected components and graph structure
- **calculate_metrics**: Calculate detailed graph metrics
- **include_temporal_analysis**: Analyze temporal changes (if timestamps available)

### Similarity Calculation

Nexus calculates multiple similarity scores:

- **Overall Similarity**: Combined structural and content similarity
- **Structural Similarity**: Based on edge structure changes
- **Content Similarity**: Based on node property and label changes

### Graph Metrics

When enabled, comparison includes detailed metrics:

- Node count, edge count, graph density
- Average degree, clustering coefficient
- Graph diameter, average shortest path length
- Triangle count, assortativity coefficient

### Example Response

```json
{
  "diff": {
    "added_nodes": [...],
    "removed_nodes": [...],
    "modified_nodes": [...],
    "added_edges": [...],
    "removed_edges": [...],
    "modified_edges": [...],
    "summary": {
      "nodes_count_original": 100,
      "nodes_count_modified": 105,
      "overall_similarity": 0.85,
      "structural_similarity": 0.80,
      "content_similarity": 0.90,
      "topology_analysis": {
        "original_components": 3,
        "modified_components": 4,
        "component_changes": [...]
      },
      "metrics_comparison": {
        "original_metrics": {...},
        "modified_metrics": {...},
        "percentage_changes": {...}
      }
    }
  },
  "detailed_analysis": {
    "node_similarities": [...],
    "edge_similarities": [...],
    "isomorphism_score": 0.85,
    "structural_changes": {...}
  },
  "report": "Graph Comparison Report\n======================\nOverall Similarity: 85.00%\n..."
}
```

### Graph Statistics

G## API Referenceconnected
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

## Geospatial Support

Nexus provides comprehensive geospatial support for location-based queries and spatial analytics.

### Point Data Type

Points represent locations in 2D or 3D space with support for multiple coordinate systems:

```cypher
// Create a node with a Point property (2D Cartesian)
CREATE (l:Location {name: 'San Francisco', coords: point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})})

// Create a 3D point
CREATE (l:Location {name: 'Building', coords: point({x: 1.0, y: 2.0, z: 3.0, crs: 'cartesian'})})
```

**Supported Coordinate Systems:**
- `cartesian` - Cartesian coordinates (x, y, z)
- `wgs-84` - WGS84 geographic coordinates (longitude, latitude, height)

### Spatial Indexes

Create spatial indexes on Point properties for efficient spatial queries:

```cypher
// Create spatial index
CREATE SPATIAL INDEX ON :Location(coords)

// Create spatial index with IF NOT EXISTS
CREATE SPATIAL INDEX IF NOT EXISTS ON :Location(coords)

// Create or replace spatial index
CREATE OR REPLACE SPATIAL INDEX ON :Location(coords)
```

### Distance Functions

Calculate distances between points:

```cypher
// Calculate distance between two points
RETURN distance(point({x: 0, y: 0, crs: 'cartesian'}), point({x: 3, y: 4, crs: 'cartesian'})) AS dist

// Find locations within distance
MATCH (l:Location)
WHERE distance(l.coords, point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})) < 1000.0
RETURN l.name, distance(l.coords, point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})) AS dist
ORDER BY dist
```

### Geospatial Procedures

Use built-in procedures for spatial queries:

```cypher
// Find points within bounding box
CALL spatial.withinBBox('Location', 'coords', -123.0, 37.0, -122.0, 38.0)
YIELD node
RETURN node

// Find points within distance
CALL spatial.withinDistance('Location', 'coords', point({x: -122.4194, y: 37.7749, crs: 'wgs-84'}), 1000.0)
YIELD node
RETURN node
```

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

## Graph Correlation Analysis

### Overview

Graph Correlation Analysis enables automatic generation of code relationship graphs from source code, providing visual understanding of code flow, dependencies, and processing patterns. This feature is particularly useful for:

- Understanding code structure and relationships
- Identifying architectural patterns
- Analyzing data flow and transformations
- Detecting design patterns
- Visualizing component hierarchies

### Graph Types

Nexus supports four types of correlation graphs:

1. **Call Graph**: Function call relationships and execution flow
2. **Dependency Graph**: Module/library dependencies and imports
3. **Data Flow Graph**: Data transformation and variable usage
4. **Component Graph**: High-level architectural components (classes, interfaces)

### Using MCP Tools

The easiest way to use Graph Correlation Analysis is through MCP (Model Context Protocol) tools:

#### Generate a Call Graph

```json
{
  "name": "graph_correlation_generate",
  "arguments": {
    "graph_type": "Call",
    "files": {
      "main.rs": "fn main() { helper(); }\nfn helper() {}",
      "utils.rs": "pub fn util() {}"
    },
    "functions": {
      "main.rs": ["main", "helper"],
      "utils.rs": ["util"]
    },
    "name": "My Call Graph"
  }
}
```

#### Analyze Graph Patterns

```json
{
  "name": "graph_correlation_analyze",
  "arguments": {
    "graph": { /* graph object */ },
    "analysis_type": "patterns"
  }
}
```

#### Export Graph

```json
{
  "name": "graph_correlation_export",
  "arguments": {
    "graph": { /* graph object */ },
    "format": "GraphML"
  }
}
```

### Using REST API

#### Generate Graph

```bash
POST /api/v1/graphs/generate
Content-Type: application/json

{
  "graph_type": "Call",
  "files": {
    "main.rs": "fn main() { helper(); }"
  },
  "name": "My Graph"
}
```

#### Get Graph

```bash
GET /api/v1/graphs/{graph_id}
```

#### Analyze Graph

```bash
POST /api/v1/graphs/{graph_id}/analyze
Content-Type: application/json

{
  "analysis_type": "statistics"
}
```

### Using UMICP Protocol

UMICP provides a standardized protocol interface for graph correlation analysis.

#### Endpoint

```
POST /umicp/graph
```

#### Generate Graph

```json
{
  "method": "graph.generate",
  "params": {
    "graph_type": "Call",
    "files": {
      "main.rs": "fn main() { helper(); }"
    }
  }
}
```

#### Get Graph

```json
{
  "method": "graph.get",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000"
  }
}
```

#### Analyze Graph

```json
{
  "method": "graph.analyze",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "analysis_type": "patterns"
  }
}
```

#### Visualize Graph

```json
{
  "method": "graph.visualize",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "width": 800,
    "height": 600
  }
}
```

#### Export Graph

```json
{
  "method": "graph.export",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "format": "GraphML"
  }
}
```

**Available UMICP Methods:**
- `graph.generate` - Generate correlation graph from source code
- `graph.get` - Retrieve a stored graph by ID
- `graph.analyze` - Analyze graph patterns and statistics
- `graph.visualize` - Generate SVG visualization
- `graph.patterns` - Detect patterns in graphs
- `graph.export` - Export graph to various formats (JSON, GraphML, GEXF, DOT)
- `graph.search` - Semantic search across graphs (placeholder)

For complete UMICP examples, see `examples/umicp_graph_correlation_examples.md`.

### Pattern Recognition

Graph Correlation Analysis can detect various patterns:

- **Pipeline Patterns**: Sequential data processing chains
- **Event-Driven Patterns**: Publisher-subscriber relationships
- **Architectural Patterns**: Layered architecture, microservices
- **Design Patterns**: Observer, Factory, Singleton, Strategy

### Visualization

Graphs can be visualized as SVG, PNG, or PDF, and exported in multiple formats:

- **JSON**: Structured graph data
- **GraphML**: Standard graph format
- **GEXF**: Gephi exchange format
- **DOT**: Graphviz format

For more details, see the [Graph Correlation Analysis Specification](specs/graph-correlation-analysis.md).

---

This completes the user guide. For more detailed information, refer to the API documentation and example code in the repository.





