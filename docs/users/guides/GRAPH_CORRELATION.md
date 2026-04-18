---
title: Graph Correlation
module: guides
id: graph-correlation
order: 3
description: Code analysis and visualization
tags: [graph-correlation, code-analysis, visualization]
---

# Graph Correlation

Complete guide for code analysis and visualization with Nexus.

## Overview

Graph Correlation Analysis enables automatic generation of code relationship graphs from source code.

## Graph Types

### Call Graph

Represents function call relationships:

```json
{
  "graph_type": "Call",
  "files": {
    "main.rs": "fn main() { helper(); }",
    "utils.rs": "pub fn helper() {}"
  }
}
```

### Dependency Graph

Represents module/library dependencies:

```json
{
  "graph_type": "Dependency",
  "files": {
    "main.rs": "use utils::helper;",
    "utils.rs": "pub mod helper;"
  }
}
```

### Data Flow Graph

Represents data transformation and variable usage:

```json
{
  "graph_type": "DataFlow",
  "files": {
    "main.rs": "let x = 1; let y = x + 1;"
  }
}
```

### Component Graph

Represents high-level architectural components:

```json
{
  "graph_type": "Component",
  "files": {
    "api.rs": "pub struct Api {}",
    "db.rs": "pub struct Database {}"
  }
}
```

## Using MCP Tools

### Generate Graph

```json
{
  "name": "graph_correlation_generate",
  "arguments": {
    "graph_type": "Call",
    "files": {
      "main.rs": "fn main() { helper(); }"
    },
    "name": "My Call Graph"
  }
}
```

### Analyze Graph

```json
{
  "name": "graph_correlation_analyze",
  "arguments": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "analysis_type": "patterns"
  }
}
```

### Export Graph

```json
{
  "name": "graph_correlation_export",
  "arguments": {
    "graph_id": "graph_550e8400-e29b-41d4-a716-446655440000",
    "format": "GraphML"
  }
}
```

## Pattern Recognition

Detects various patterns:
- **Pipeline Patterns**: Sequential data processing
- **Event-Driven Patterns**: Publisher-subscriber
- **Architectural Patterns**: Layered architecture
- **Design Patterns**: Observer, Factory, Singleton

## Visualization

Graphs can be visualized as:
- **SVG**: Scalable vector graphics
- **PNG**: Raster images
- **PDF**: Document format

## Related Topics

- [MCP Protocol](../api/MCP.md) - MCP integration
- [API Reference](../api/API_REFERENCE.md) - REST API
- [Use Cases](../use-cases/) - Real-world examples

