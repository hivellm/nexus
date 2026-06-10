//! MCP tool schema definitions — `get_nexus_mcp_tools`.

use serde_json::json;

/// Get Nexus MCP tools definitions
pub fn get_nexus_mcp_tools() -> Vec<rmcp::model::Tool> {
    vec![
        // Graph Operations
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("create_node"),
            title: Some("Create Node".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Create a new node in the graph with specified labels and properties.",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(rmcp::model::ToolAnnotations::new().read_only(false)),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("create_relationship"),
            title: Some("Create Relationship".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Create a new relationship between two nodes.",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(rmcp::model::ToolAnnotations::new().read_only(false)),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("execute_cypher"),
            title: Some("Execute Cypher Query".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Execute a Cypher query against the graph database.",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(
                rmcp::model::ToolAnnotations::new()
                    .read_only(true)
                    .idempotent(true),
            ),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("knn_search"),
            title: Some("KNN Vector Search".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Perform K-nearest neighbors vector search on nodes.",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(
                rmcp::model::ToolAnnotations::new()
                    .read_only(true)
                    .idempotent(true),
            ),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("get_stats"),
            title: Some("Get Database Statistics".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Get database statistics including node count, relationship count, and index information.",
            )),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            })
            .as_object()
            .unwrap()
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(
                rmcp::model::ToolAnnotations::new()
                    .read_only(true)
                    .idempotent(true),
            ),
        },
        // Graph Correlation Tools
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("graph_correlation_generate"),
            title: Some("Generate Correlation Graph".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Generate a correlation graph from source code (Call, Dependency, DataFlow, or Component graph).",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(rmcp::model::ToolAnnotations::new().read_only(false)),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("graph_correlation_analyze"),
            title: Some("Analyze Correlation Graph".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Analyze a correlation graph to extract patterns and statistics.",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(
                rmcp::model::ToolAnnotations::new()
                    .read_only(true)
                    .idempotent(true),
            ),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("graph_correlation_export"),
            title: Some("Export Correlation Graph".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "Export a correlation graph to various formats (JSON, GraphML, GEXF, DOT).",
            )),
            input_schema: json!({
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
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(
                rmcp::model::ToolAnnotations::new()
                    .read_only(true)
                    .idempotent(true),
            ),
        },
        rmcp::model::Tool {
            name: std::borrow::Cow::Borrowed("graph_correlation_types"),
            title: Some("List Graph Correlation Types".to_string()),
            description: Some(std::borrow::Cow::Borrowed(
                "List available graph correlation types.",
            )),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            })
            .as_object()
            .unwrap()
            .clone()
            .into(),
            output_schema: None,
            icons: None,
            annotations: Some(
                rmcp::model::ToolAnnotations::new()
                    .read_only(true)
                    .idempotent(true),
            ),
        },
    ]
}
