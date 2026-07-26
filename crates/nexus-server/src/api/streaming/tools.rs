//! MCP tool schema definitions — `get_nexus_mcp_tools`.

use serde_json::json;

/// Get Nexus MCP tools definitions
pub fn get_nexus_mcp_tools() -> Vec<rmcp::model::Tool> {
    vec![
        // Graph Operations
        rmcp::model::Tool::new(
            "create_node",
            "Create a new node in the graph with specified labels and properties.",
            json!({
                "type": "object",
                "properties": {
                    "labels": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Node labels"
                    },
                    "properties": {
                        "type": "object",
                        "description": "Node properties"
                    }
                },
                "required": ["labels"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Create Node")
        .with_annotations(rmcp::model::ToolAnnotations::new().read_only(false)),
        rmcp::model::Tool::new(
            "create_relationship",
            "Create a new relationship between two nodes.",
            json!({
                "type": "object",
                "properties": {
                    "source_id": {
                        "type": "integer",
                        "description": "Source node ID"
                    },
                    "target_id": {
                        "type": "integer",
                        "description": "Target node ID"
                    },
                    "rel_type": {
                        "type": "string",
                        "description": "Relationship type"
                    },
                    "properties": {
                        "type": "object",
                        "description": "Relationship properties"
                    }
                },
                "required": ["source_id", "target_id", "rel_type"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Create Relationship")
        .with_annotations(rmcp::model::ToolAnnotations::new().read_only(false)),
        rmcp::model::Tool::new(
            "execute_cypher",
            "Execute a Cypher query against the graph database.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Cypher query to execute"
                    }
                },
                "required": ["query"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Execute Cypher Query")
        .with_annotations(
            rmcp::model::ToolAnnotations::new()
                .read_only(true)
                .idempotent(true),
        ),
        rmcp::model::Tool::new(
            "knn_search",
            "Perform K-nearest neighbors vector search on nodes.",
            json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Node label to search"
                    },
                    "vector": {
                        "type": "array",
                        "items": {"type": "number"},
                        "description": "Query vector"
                    },
                    "k": {
                        "type": "integer",
                        "description": "Number of nearest neighbors",
                        "default": 10
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results to return",
                        "default": 100
                    }
                },
                "required": ["label", "vector", "k"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("KNN Vector Search")
        .with_annotations(
            rmcp::model::ToolAnnotations::new()
                .read_only(true)
                .idempotent(true),
        ),
        rmcp::model::Tool::new(
            "get_stats",
            "Get database statistics including node count, relationship count, and index information.",
            json!({
                "type": "object",
                "properties": {},
                "required": []
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Get Database Statistics")
        .with_annotations(
            rmcp::model::ToolAnnotations::new()
                .read_only(true)
                .idempotent(true),
        ),
        // Graph Correlation Tools
        rmcp::model::Tool::new(
            "graph_correlation_generate",
            "Generate a correlation graph from source code (Call, Dependency, DataFlow, or Component graph).",
            json!({
                "type": "object",
                "properties": {
                    "graph_type": {
                        "type": "string",
                        "enum": ["Call", "Dependency", "DataFlow", "Component"],
                        "description": "Type of graph to generate"
                    },
                    "files": {
                        "type": "object",
                        "description": "Map of file paths to content"
                    },
                    "functions": {
                        "type": "object",
                        "description": "Map of files to function lists (optional)"
                    },
                    "imports": {
                        "type": "object",
                        "description": "Map of files to import lists (optional)"
                    },
                    "name": {
                        "type": "string",
                        "description": "Graph name (optional)"
                    }
                },
                "required": ["graph_type", "files"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Generate Correlation Graph")
        .with_annotations(rmcp::model::ToolAnnotations::new().read_only(false)),
        rmcp::model::Tool::new(
            "graph_correlation_analyze",
            "Analyze a correlation graph to extract patterns and statistics.",
            json!({
                "type": "object",
                "properties": {
                    "graph": {
                        "type": "object",
                        "description": "Graph to analyze"
                    },
                    "analysis_type": {
                        "type": "string",
                        "enum": ["statistics", "patterns", "all"],
                        "description": "Type of analysis to perform"
                    }
                },
                "required": ["graph", "analysis_type"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Analyze Correlation Graph")
        .with_annotations(
            rmcp::model::ToolAnnotations::new()
                .read_only(true)
                .idempotent(true),
        ),
        rmcp::model::Tool::new(
            "graph_correlation_export",
            "Export a correlation graph to various formats (JSON, GraphML, GEXF, DOT).",
            json!({
                "type": "object",
                "properties": {
                    "graph": {
                        "type": "object",
                        "description": "Graph to export"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["JSON", "GraphML", "GEXF", "DOT"],
                        "description": "Export format"
                    }
                },
                "required": ["graph", "format"]
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("Export Correlation Graph")
        .with_annotations(
            rmcp::model::ToolAnnotations::new()
                .read_only(true)
                .idempotent(true),
        ),
        rmcp::model::Tool::new(
            "graph_correlation_types",
            "List available graph correlation types.",
            json!({
                "type": "object",
                "properties": {},
                "required": []
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .with_title("List Graph Correlation Types")
        .with_annotations(
            rmcp::model::ToolAnnotations::new()
                .read_only(true)
                .idempotent(true),
        ),
    ]
}
