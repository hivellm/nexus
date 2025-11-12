# Nexus Graph Database - User Guide

## Table of Contents

1. [Introduction](#introduction)
2. [Quick Start](#quick-start)
3. [Basic Operations](#basic-operations)
4. [Cypher Query Language](#cypher-query-language)
5. [User-Defined Functions (UDFs) and Procedures](#user-defined-functions-udfs-and-procedures)
6. [KNN Vector Search](#knn-vector-search)
7. [Graph Comparison](#graph-comparison)
8. [API Reference](#api-reference)
9. [Performance Tips](#performance-tips)
10. [Examples](#examples)
11. [Troubleshooting](#troubleshooting)

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





