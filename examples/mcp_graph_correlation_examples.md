# Graph Correlation MCP Tools - Usage Examples

This document provides practical examples for using the Graph Correlation MCP tools in Nexus.

## Overview

The Graph Correlation MCP tools allow you to:
- Generate correlation graphs from source code
- Analyze graphs for patterns and statistics
- Export graphs in various formats
- List available graph types

## Prerequisites

- Nexus server running with MCP support
- MCP client configured to connect to Nexus
- Source code files to analyze

## Tool 1: graph_correlation_generate

Generate a correlation graph from source code.

### Example 1: Generate Call Graph

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_generate",
    "arguments": {
      "graph_type": "Call",
      "files": {
        "main.rs": "fn main() {\n    let result = process_data();\n    println!(\"{}\", result);\n}\n\nfn process_data() -> String {\n    transform_data()\n}\n\nfn transform_data() -> String {\n    \"transformed\".to_string()\n}",
        "utils.rs": "pub fn helper() {\n    println!(\"Helper function\");\n}"
      },
      "name": "My Project Call Graph"
    }
  }
}
```

**Response**:
```json
{
  "status": "success",
  "graph": {
    "name": "My Project Call Graph",
    "graph_type": "Call",
    "nodes": [
      {"id": "main", "node_type": "Function", "label": "main", "metadata": {}, "position": null, "size": null, "color": null},
      {"id": "process_data", "node_type": "Function", "label": "process_data", "metadata": {}, "position": null, "size": null, "color": null},
      {"id": "transform_data", "node_type": "Function", "label": "transform_data", "metadata": {}, "position": null, "size": null, "color": null}
    ],
    "edges": [
      {"id": "edge_main_process_data", "source": "main", "target": "process_data", "edge_type": "Calls", "weight": 1.0, "label": null, "metadata": {}},
      {"id": "edge_process_data_transform_data", "source": "process_data", "target": "transform_data", "edge_type": "Calls", "weight": 1.0, "label": null, "metadata": {}}
    ],
    "node_count": 3,
    "edge_count": 2
  }
}
```

### Example 2: Generate Dependency Graph

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_generate",
    "arguments": {
      "graph_type": "Dependency",
      "files": {
        "mod_a.rs": "use mod_b;\nuse mod_c;",
        "mod_b.rs": "use mod_d;",
        "mod_c.rs": "",
        "mod_d.rs": ""
      },
      "imports": {
        "mod_a.rs": ["mod_b", "mod_c"],
        "mod_b.rs": ["mod_d"]
      }
    }
  }
}
```

### Example 3: Generate DataFlow Graph

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_generate",
    "arguments": {
      "graph_type": "DataFlow",
      "files": {
        "pipeline.rs": "fn input() -> Data { Data::new() }\nfn process(data: Data) -> Data { transform(data) }\nfn output(data: Data) { println!(\"{}\", data); }"
      },
      "functions": {
        "pipeline.rs": ["input", "process", "output"]
      }
    }
  }
}
```

## Tool 2: graph_correlation_analyze

Analyze a correlation graph to extract patterns and statistics.

### Example 1: Analyze Statistics

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_analyze",
    "arguments": {
      "graph": {
        "name": "Test Graph",
        "graph_type": "Call",
        "nodes": [
          {"id": "node1", "node_type": "Function", "label": "func1", "metadata": {}, "position": null, "size": null, "color": null},
          {"id": "node2", "node_type": "Function", "label": "func2", "metadata": {}, "position": null, "size": null, "color": null},
          {"id": "node3", "node_type": "Function", "label": "func3", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [
          {"id": "edge_node1_node2", "source": "node1", "target": "node2", "edge_type": "Calls", "weight": 1.0, "label": null, "metadata": {}},
          {"id": "edge_node2_node3", "source": "node2", "target": "node3", "edge_type": "Calls", "weight": 1.0, "label": null, "metadata": {}}
        ],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "analysis_type": "statistics"
    }
  }
}
```

**Response**:
```json
{
  "status": "success",
  "analysis_type": "statistics",
  "statistics": {
    "node_count": 3,
    "edge_count": 2,
    "avg_degree": 1.33,
    "max_degree": 2,
    "graph_density": 0.33
  }
}
```

### Example 2: Analyze Patterns

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_analyze",
    "arguments": {
      "graph": {
        "name": "Pipeline Graph",
        "graph_type": "DataFlow",
        "nodes": [
          {"id": "stage1", "node_type": "Function", "label": "input", "metadata": {}, "position": null, "size": null, "color": null},
          {"id": "stage2", "node_type": "Function", "label": "process", "metadata": {}, "position": null, "size": null, "color": null},
          {"id": "stage3", "node_type": "Function", "label": "output", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [
          {"id": "edge_stage1_stage2", "source": "stage1", "target": "stage2", "edge_type": "Transforms", "weight": 1.0, "label": null, "metadata": {}},
          {"id": "edge_stage2_stage3", "source": "stage2", "target": "stage3", "edge_type": "Transforms", "weight": 1.0, "label": null, "metadata": {}}
        ],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "analysis_type": "patterns"
    }
  }
}
```

**Response**:
```json
{
  "status": "success",
  "analysis_type": "patterns",
  "patterns": [
    {
      "pattern_type": "Pipeline",
      "description": "Linear data processing pipeline",
      "nodes": ["stage1", "stage2", "stage3"],
      "confidence": 0.95
    }
  ],
  "pattern_count": 1
}
```

### Example 3: Analyze All (Statistics + Patterns)

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_analyze",
    "arguments": {
      "graph": {
        "name": "Full Graph",
        "graph_type": "Call",
        "nodes": [
          {"id": "n1", "node_type": "Function", "label": "f1", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "analysis_type": "all"
    }
  }
}
```

## Tool 3: graph_correlation_export

Export a correlation graph in various formats.

### Example 1: Export as JSON

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_export",
    "arguments": {
      "graph": {
        "name": "Export Test",
        "graph_type": "Call",
        "nodes": [
          {"id": "n1", "node_type": "Function", "label": "func", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "format": "JSON"
    }
  }
}
```

### Example 2: Export as GraphML

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_export",
    "arguments": {
      "graph": {
        "name": "GraphML Export",
        "graph_type": "Dependency",
        "nodes": [
          {"id": "mod1", "node_type": "Module", "label": "module1", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "format": "GraphML"
    }
  }
}
```

### Example 3: Export as GEXF

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_export",
    "arguments": {
      "graph": {
        "name": "GEXF Export",
        "graph_type": "Call",
        "nodes": [
          {"id": "n1", "node_type": "Function", "label": "func", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "format": "GEXF"
    }
  }
}
```

### Example 4: Export as DOT

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_export",
    "arguments": {
      "graph": {
        "name": "DOT Export",
        "graph_type": "Call",
        "nodes": [
          {"id": "n1", "node_type": "Function", "label": "func", "metadata": {}, "position": null, "size": null, "color": null}
        ],
        "edges": [],
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z"
      },
      "format": "DOT"
    }
  }
}
```

## Tool 4: graph_correlation_types

List available graph correlation types.

### Example: List Types

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_types",
    "arguments": {}
  }
}
```

**Response**:
```json
{
  "status": "success",
  "types": ["Call", "Dependency", "DataFlow", "Component"],
  "descriptions": {
    "Call": "Function call relationships and execution flow",
    "Dependency": "Module and package dependency relationships",
    "DataFlow": "Data flow and transformation pipelines",
    "Component": "High-level component and module relationships"
  }
}
```

## Complete Workflow Example

Here's a complete workflow: generate a graph, analyze it, and export it:

### Step 1: Generate Graph

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_generate",
    "arguments": {
      "graph_type": "Call",
      "files": {
        "main.rs": "fn main() { helper(); }"
      }
    }
  }
}
```

### Step 2: Analyze Generated Graph

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_analyze",
    "arguments": {
      "graph": <graph_from_step_1>,
      "analysis_type": "all"
    }
  }
}
```

### Step 3: Export Graph

```json
{
  "method": "tools/call",
  "params": {
    "name": "graph_correlation_export",
    "arguments": {
      "graph": <graph_from_step_1>,
      "format": "GraphML"
    }
  }
}
```

## Error Handling

All tools return standard error responses:

```json
{
  "error": {
    "code": "INVALID_PARAMS",
    "message": "Missing required parameter: graph_type"
  }
}
```

Common error codes:
- `INVALID_PARAMS`: Missing or invalid parameters
- `INVALID_GRAPH`: Graph structure is invalid
- `INVALID_FORMAT`: Export format not supported
- `INTERNAL_ERROR`: Server-side error

## Best Practices

1. **Graph Normalization**: The tools automatically normalize partial graph structures, but providing complete graphs with all required fields is recommended for better performance.

2. **Graph Types**: Choose the appropriate graph type based on your analysis needs:
   - Use `Call` for function call analysis
   - Use `Dependency` for module dependency analysis
   - Use `DataFlow` for data transformation analysis
   - Use `Component` for high-level architecture analysis

3. **Analysis Types**: 
   - Use `statistics` for quick overview
   - Use `patterns` for pattern detection
   - Use `all` for comprehensive analysis

4. **Export Formats**:
   - Use `JSON` for programmatic processing
   - Use `GraphML` for Gephi and other graph tools
   - Use `GEXF` for Gephi visualization
   - Use `DOT` for Graphviz visualization

5. **Performance**: For large codebases, consider:
   - Processing files in batches
   - Using incremental graph updates
   - Caching generated graphs

## Testing

All MCP tools have comprehensive test coverage (32 tests, 100% pass rate). See `nexus-server/src/api/graph_correlation_mcp_tests.rs` for test examples.

