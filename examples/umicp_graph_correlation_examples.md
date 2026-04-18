# UMICP Graph Correlation Analysis Examples

This document provides practical examples for using UMICP (Universal Model Interoperability Protocol) methods for Graph Correlation Analysis in Nexus.

## Overview

UMICP provides a standardized protocol interface for graph correlation analysis. All methods are accessed through the `/umicp/graph` endpoint.

## Basic Usage

### Endpoint

```
POST /umicp/graph
Content-Type: application/json
```

### Request Format

```json
{
  "method": "graph.generate",
  "params": {
    "graph_type": "Call",
    "files": {
      "file.rs": "fn main() { helper(); }"
    }
  },
  "context": {
    "trace_id": "abc-123",
    "caller": "llm-agent"
  }
}
```

### Response Format

```json
{
  "result": {
    "graph_id": "graph_uuid",
    "graph": { ... },
    "node_count": 5,
    "edge_count": 3
  },
  "error": null,
  "context": null
}
```

## Method Examples

### 1. graph.generate - Generate Correlation Graph

Generate a graph from source code.

**Request:**
```json
{
  "method": "graph.generate",
  "params": {
    "graph_type": "Call",
    "files": {
      "main.rs": "fn main() { helper(); }\nfn helper() {}",
      "utils.rs": "pub fn util() {}"
    },
    "functions": {
      "main.rs": ["main", "helper"],
      "utils.rs": ["util"]
    },
    "imports": {
      "main.rs": ["utils"]
    }
  }
}
```

**Response:**
```json
{
  "result": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "graph": {
      "name": "Generated Graph",
      "graph_type": "Call",
      "nodes": [
        {
          "id": "main",
          "node_type": "Function",
          "label": "main",
          "metadata": {"file": "main.rs"}
        }
      ],
      "edges": [
        {
          "source": "main",
          "target": "helper",
          "edge_type": "Calls"
        }
      ]
    },
    "node_count": 3,
    "edge_count": 2
  },
  "error": null
}
```

**Supported Graph Types:**
- `Call` - Function call relationships
- `Dependency` - Module dependencies
- `DataFlow` - Data flow and transformations
- `Component` - High-level components

### 2. graph.get - Retrieve Graph by ID

Retrieve a previously generated graph.

**Request:**
```json
{
  "method": "graph.get",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Response:**
```json
{
  "result": {
    "graph": {
      "name": "Generated Graph",
      "graph_type": "Call",
      "nodes": [...],
      "edges": [...]
    },
    "node_count": 3,
    "edge_count": 2
  },
  "error": null
}
```

**Error Response (Graph Not Found):**
```json
{
  "result": null,
  "error": {
    "code": "GRAPH_NOT_FOUND",
    "message": "Graph nonexistent not found",
    "data": null
  }
}
```

### 3. graph.analyze - Analyze Graph Patterns and Statistics

Analyze a graph for patterns, statistics, or both.

**Request (Statistics Only):**
```json
{
  "method": "graph.analyze",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "analysis_type": "statistics"
  }
}
```

**Response:**
```json
{
  "result": {
    "analysis_type": "statistics",
    "statistics": {
      "node_count": 10,
      "edge_count": 15,
      "average_degree": 3.0,
      "density": 0.33,
      "connected_components": 1,
      "max_depth": 5
    }
  },
  "error": null
}
```

**Request (Patterns Only):**
```json
{
  "method": "graph.analyze",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "analysis_type": "patterns"
  }
}
```

**Response:**
```json
{
  "result": {
    "analysis_type": "patterns",
    "patterns": [
      {
        "pattern_type": "Pipeline",
        "confidence": 0.85,
        "node_ids": ["node1", "node2", "node3"],
        "metadata": {
          "description": "Sequential processing chain"
        }
      }
    ],
    "pattern_count": 1
  },
  "error": null
}
```

**Request (All Analysis):**
```json
{
  "method": "graph.analyze",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "analysis_type": "all"
  }
}
```

**Supported Analysis Types:**
- `statistics` - Graph metrics and statistics
- `patterns` - Pattern detection results
- `all` - Both statistics and patterns

### 4. graph.visualize - Generate SVG Visualization

Generate an SVG visualization of the graph.

**Request:**
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

**Response:**
```json
{
  "result": {
    "svg": "<svg width=\"800\" height=\"600\">...</svg>",
    "width": 800,
    "height": 600,
    "node_count": 10,
    "edge_count": 15
  },
  "error": null
}
```

**Using Inline Graph:**
```json
{
  "method": "graph.visualize",
  "params": {
    "graph": {
      "name": "Test Graph",
      "graph_type": "Call",
      "nodes": [...],
      "edges": [...]
    },
    "width": 1200,
    "height": 800
  }
}
```

### 5. graph.patterns - Detect Patterns

Convenience method for pattern detection (equivalent to `graph.analyze` with `analysis_type: "patterns"`).

**Request:**
```json
{
  "method": "graph.patterns",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Response:**
```json
{
  "result": {
    "analysis_type": "patterns",
    "patterns": [
      {
        "pattern_type": "Observer",
        "confidence": 0.92,
        "node_ids": ["subject", "observer1", "observer2"],
        "metadata": {
          "description": "Observer pattern detected"
        }
      },
      {
        "pattern_type": "Pipeline",
        "confidence": 0.78,
        "node_ids": ["step1", "step2", "step3"],
        "metadata": {
          "description": "Pipeline pattern detected"
        }
      }
    ],
    "pattern_count": 2
  },
  "error": null
}
```

### 6. graph.export - Export Graph to Various Formats

Export a graph to JSON, GraphML, GEXF, or DOT format.

**Request (JSON):**
```json
{
  "method": "graph.export",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "format": "JSON"
  }
}
```

**Request (GraphML):**
```json
{
  "method": "graph.export",
  "params": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "format": "GraphML"
  }
}
```

**Response:**
```json
{
  "result": {
    "format": "GraphML",
    "content": "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml>...</graphml>",
    "size_bytes": 1234
  },
  "error": null
}
```

**Supported Formats:**
- `JSON` - Structured JSON format
- `GraphML` - Standard GraphML XML format
- `GEXF` - GEXF format for Gephi
- `DOT` - Graphviz DOT format

### 7. graph.search - Semantic Search (Placeholder)

Search graphs semantically (currently returns empty results, full implementation pending).

**Request:**
```json
{
  "method": "graph.search",
  "params": {
    "query": "function calls"
  }
}
```

**Response:**
```json
{
  "result": {
    "query": "function calls",
    "results": [],
    "count": 0
  },
  "error": null
}
```

## Error Handling

All methods return errors in the standard UMICP format:

```json
{
  "result": null,
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error message",
    "data": {
      "additional": "error details"
    }
  }
}
```

**Common Error Codes:**
- `INVALID_PARAMS` - Missing or invalid parameters
- `GRAPH_NOT_FOUND` - Graph ID not found
- `GRAPH_GENERATION_FAILED` - Graph generation error
- `VISUALIZATION_FAILED` - Visualization rendering error
- `EXPORT_FAILED` - Export format error
- `METHOD_NOT_FOUND` - Unknown method name

## Complete Workflow Example

```json
// Step 1: Generate a call graph
{
  "method": "graph.generate",
  "params": {
    "graph_type": "Call",
    "files": {
      "main.rs": "fn main() { process(); }\nfn process() { output(); }",
      "io.rs": "fn output() {}"
    }
  }
}

// Response contains graph_id: "graph_abc123"

// Step 2: Analyze the graph
{
  "method": "graph.analyze",
  "params": {
    "graph_id": "graph_abc123",
    "analysis_type": "all"
  }
}

// Step 3: Visualize the graph
{
  "method": "graph.visualize",
  "params": {
    "graph_id": "graph_abc123",
    "width": 1000,
    "height": 800
  }
}

// Step 4: Export to GraphML
{
  "method": "graph.export",
  "params": {
    "graph_id": "graph_abc123",
    "format": "GraphML"
  }
}
```

## cURL Examples

### Generate Graph

```bash
curl -X POST http://localhost:8080/umicp/graph \
  -H "Content-Type: application/json" \
  -d '{
    "method": "graph.generate",
    "params": {
      "graph_type": "Call",
      "files": {
        "main.rs": "fn main() { helper(); }"
      }
    }
  }'
```

### Get Graph

```bash
curl -X POST http://localhost:8080/umicp/graph \
  -H "Content-Type: application/json" \
  -d '{
    "method": "graph.get",
    "params": {
      "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000"
    }
  }'
```

### Analyze Graph

```bash
curl -X POST http://localhost:8080/umicp/graph \
  -H "Content-Type: application/json" \
  -d '{
    "method": "graph.analyze",
    "params": {
      "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
      "analysis_type": "patterns"
    }
  }'
```

## Integration with LLM Agents

UMICP is designed for integration with LLM agents and AI systems:

```python
import requests

class NexusUMICPClient:
    def __init__(self, base_url="http://localhost:8080"):
        self.base_url = base_url
        self.endpoint = f"{base_url}/umicp/graph"
    
    def generate_graph(self, graph_type, files, **kwargs):
        request = {
            "method": "graph.generate",
            "params": {
                "graph_type": graph_type,
                "files": files,
                **kwargs
            }
        }
        response = requests.post(self.endpoint, json=request)
        return response.json()
    
    def analyze_graph(self, graph_id, analysis_type="all"):
        request = {
            "method": "graph.analyze",
            "params": {
                "graph_id": graph_id,
                "analysis_type": analysis_type
            }
        }
        response = requests.post(self.endpoint, json=request)
        return response.json()

# Usage
client = NexusUMICPClient()
result = client.generate_graph("Call", {"main.rs": "fn main() {}"})
graph_id = result["result"]["graph_id"]
analysis = client.analyze_graph(graph_id, "patterns")
```

## Comparison with MCP Tools

UMICP provides a unified protocol interface, while MCP tools are individual tool calls:

**MCP Approach:**
```json
{
  "name": "graph_correlation_generate",
  "arguments": {
    "graph_type": "Call",
    "files": {...}
  }
}
```

**UMICP Approach:**
```json
{
  "method": "graph.generate",
  "params": {
    "graph_type": "Call",
    "files": {...}
  }
}
```

Both approaches provide the same functionality, but UMICP offers:
- Standardized request/response format
- Built-in error handling
- Context propagation
- Protocol-level features (discovery, versioning)

