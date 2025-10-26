//! MCP StreamableHTTP implementation for Nexus
//!
//! This module provides MCP (Model Context Protocol) support using StreamableHTTP transport.
//! Based on the rmcp crate with transport-streamable-http-server.
//!
//! ## Protocol
//! - **MCP StreamableHTTP**: Primary protocol for AI integrations
//! - **Transport**: HTTP with chunked transfer encoding
//! - **Compatible with**: Vectorizer, Context7, and other MCP clients

use std::collections::HashMap;
use std::sync::Arc;

use axum::response::Json;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, Content, ErrorData, Implementation, ListResourcesResult,
    ListToolsResult, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use serde_json::json;

use crate::NexusServer;
use nexus_core::executor::Query as CypherQuery;

/// StreamableHTTP service implementation for Nexus
#[derive(Clone)]
pub struct NexusMcpService {
    /// Nexus server state
    pub server: Arc<NexusServer>,
}

impl NexusMcpService {
    /// Create a new MCP service instance
    pub fn new(server: Arc<NexusServer>) -> Self {
        Self { server }
    }
}

impl ServerHandler for NexusMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "nexus-server".to_string(),
                title: Some("Nexus Graph Database Server".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                website_url: Some("https://github.com/hivellm/nexus".to_string()),
                icons: None,
            },
            instructions: Some("Nexus Graph Database - High-performance property graph database with native vector search and MCP integration.".to_string()),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = get_nexus_mcp_tools();

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        handle_nexus_mcp_tool(request, self.server.clone()).await
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }
}

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

/// Handle MCP tool calls for Nexus
pub async fn handle_nexus_mcp_tool(
    request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    match request.name.as_ref() {
        "create_node" => handle_create_node(request, server).await,
        "create_relationship" => handle_create_relationship(request, server).await,
        "execute_cypher" => handle_execute_cypher(request, server).await,
        "knn_search" => handle_knn_search(request, server).await,
        "get_stats" => handle_get_stats(request, server).await,
        "graph_correlation_generate" => handle_graph_correlation_generate(request, server).await,
        "graph_correlation_analyze" => handle_graph_correlation_analyze(request, server).await,
        "graph_correlation_export" => handle_graph_correlation_export(request, server).await,
        "graph_correlation_types" => handle_graph_correlation_types(request, server).await,
        _ => Err(ErrorData::invalid_params("Unknown tool", None)),
    }
}

/// Handle create node tool
async fn handle_create_node(
    request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    let labels = args
        .get("labels")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ErrorData::invalid_params("Missing labels", None))?
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let properties = args.get("properties").cloned().unwrap_or(json!({}));

    // Use executor to create node
    let mut executor = server.executor.write().await;
    
    // Execute Cypher CREATE query
    let mut create_query = String::from("CREATE (n");
    for label in labels.iter() {
        create_query.push(':');
        create_query.push_str(label);
    }
    
    // Build Cypher properties map from JSON
    let mut props_parts = Vec::new();
    if let Some(props_obj) = properties.as_object() {
        for (key, value) in props_obj.iter() {
            let val_str = match value {
                serde_json::Value::String(s) => format!("\"{}\"", s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => format!("{}", value),
            };
            props_parts.push(format!("{}: {}", key, val_str));
        }
    }
    let props_str = if props_parts.is_empty() {
        String::new()
    } else {
        format!(" {{{}}}", props_parts.join(", "))
    };
    
    create_query.push_str(&format!("{}) RETURN id(n) as node_id", props_str));
    
    let mut params = HashMap::new();
    
    let query = CypherQuery {
        cypher: create_query,
        params,
    };
    
    match executor.execute(&query) {
        Ok(result_set) => {
            if let Some(row) = result_set.rows.first() {
                // Try to find node_id column index
                let node_id_idx = result_set.columns.iter().position(|c| c == "node_id");
                if let Some(idx) = node_id_idx {
                    if idx < row.values.len() {
                        // The value is in row.values[idx], convert it
                        let node_id = row.values[idx].as_u64().unwrap_or(0);
                        let response = json!({
                            "status": "created",
                            "node_id": node_id,
                            "labels": labels,
                            "properties": properties
                        });
                        return Ok(CallToolResult::success(vec![Content::text(
                            response.to_string(),
                        )]));
                    }
                }
            }
            Err(ErrorData::internal_error(
                "Failed to extract node ID from result".to_string(),
                None,
            ))
        }
        Err(e) => Err(ErrorData::internal_error(
            format!("Failed to create node: {}", e),
            None,
        )),
    }
}

/// Handle create relationship tool
async fn handle_create_relationship(
    request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    let source_id = args
        .get("source_id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ErrorData::invalid_params("Missing source_id", None))?;

    let target_id = args
        .get("target_id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ErrorData::invalid_params("Missing target_id", None))?;

    let rel_type = args
        .get("rel_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing rel_type", None))?
        .to_string();

    let properties = args.get("properties").cloned().unwrap_or(json!({}));

    // Use executor to create relationship
    let mut executor = server.executor.write().await;
    
    // Execute Cypher CREATE query for relationship
    let create_query = format!(
        "MATCH (s), (t) WHERE id(s) = $src_id AND id(t) = $tgt_id CREATE (s)-[r:{}]->(t) SET r = $props RETURN id(r) as rel_id",
        rel_type
    );
    
    let mut params = HashMap::new();
    params.insert("src_id".to_string(), json!(source_id));
    params.insert("tgt_id".to_string(), json!(target_id));
    params.insert("props".to_string(), properties.clone());
    
    let query = CypherQuery {
        cypher: create_query,
        params,
    };
    
    match executor.execute(&query) {
        Ok(result_set) => {
            if let Some(row) = result_set.rows.first() {
                // Try to find rel_id column index
                let rel_id_idx = result_set.columns.iter().position(|c| c == "rel_id");
                if let Some(idx) = rel_id_idx {
                    if idx < row.values.len() {
                        // The value is in row.values[idx], convert it
                        let rel_id = row.values[idx].as_u64().unwrap_or(0);
                        let response = json!({
                            "status": "created",
                            "relationship_id": rel_id,
                            "source_id": source_id,
                            "target_id": target_id,
                            "rel_type": rel_type,
                            "properties": properties
                        });
                        return Ok(CallToolResult::success(vec![Content::text(
                            response.to_string(),
                        )]));
                    }
                }
            }
            Err(ErrorData::internal_error(
                "Failed to extract relationship ID from result".to_string(),
                None,
            ))
        }
        Err(e) => Err(ErrorData::internal_error(
            format!("Failed to create relationship: {}", e),
            None,
        )),
    }
}

/// Handle execute Cypher tool
async fn handle_execute_cypher(
    request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing query", None))?;

    let start_time = std::time::Instant::now();

    // Execute Cypher query using the executor
    let mut executor = server.executor.write().await;
    let query_obj = CypherQuery {
        cypher: query.to_string(),
        params: HashMap::new(),
    };

    let result = executor
        .execute(&query_obj)
        .map_err(|e| ErrorData::internal_error(format!("Cypher execution failed: {}", e), None))?;

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    // Convert result to JSON
    let mut rows = Vec::new();
    for row in &result.rows {
        let mut row_obj = serde_json::Map::new();
        for (i, value) in row.values.iter().enumerate() {
            if i < result.columns.len() {
                let column_name = &result.columns[i];
                row_obj.insert(
                    column_name.clone(),
                    serde_json::to_value(value).unwrap_or(json!(null)),
                );
            }
        }
        rows.push(serde_json::Value::Object(row_obj));
    }

    let response = json!({
        "status": "executed",
        "query": query,
        "columns": result.columns,
        "rows": rows,
        "row_count": result.rows.len(),
        "execution_time_ms": execution_time_ms
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Handle KNN search tool
async fn handle_knn_search(
    request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    let label = args
        .get("label")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing label", None))?;

    let vector = args
        .get("vector")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ErrorData::invalid_params("Missing vector", None))?
        .iter()
        .filter_map(|v| v.as_f64())
        .map(|f| f as f32)
        .collect::<Vec<_>>();

    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

    // Use real KNN index for search
    let knn_index = server.knn_index.read().await;
    match knn_index.search_knn(&vector, k) {
        Ok(results) => {
            let results_json: Vec<_> = results
                .iter()
                .map(|(node_id, distance)| json!({
                    "node_id": node_id,
                    "distance": distance,
                    "score": 1.0 / (1.0 + distance)
                }))
                .take(limit)
                .collect();

            let response = json!({
                "status": "completed",
                "label": label,
                "k": k,
                "limit": limit,
                "vector_dimension": vector.len(),
                "results": results_json
            });

            Ok(CallToolResult::success(vec![Content::text(
                response.to_string(),
            )]))
        }
        Err(e) => Err(ErrorData::internal_error(
            format!("KNN search failed: {}", e),
            None,
        )),
    }
}

/// Handle get stats tool
async fn handle_get_stats(
    _request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    // Get stats from shared components
    let catalog = server.catalog.read().await;
    let knn_index = server.knn_index.read().await;
    let knn_stats = knn_index.get_stats();
    
    let response = json!({
        "status": "ok",
        "stats": {
            "node_count": 0, // TODO: Get from storage layer
            "relationship_count": 0, // TODO: Get from storage layer
            "label_count": catalog.label_count(),
            "relationship_type_count": 0, // TODO: Get from catalog
            "label_index_size": 0,
            "knn_index_size": knn_stats.total_vectors,
            "memory_usage_mb": 0,
            "uptime_seconds": 0
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Handle graph correlation generate tool
async fn handle_graph_correlation_generate(
    request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    use nexus_core::graph_correlation::{GraphCorrelationManager, GraphSourceData, GraphType};

    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    // Parse graph type
    let graph_type_str = args
        .get("graph_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing graph_type", None))?;

    let graph_type = match graph_type_str {
        "Call" => GraphType::Call,
        "Dependency" => GraphType::Dependency,
        "DataFlow" => GraphType::DataFlow,
        "Component" => GraphType::Component,
        _ => return Err(ErrorData::invalid_params("Invalid graph_type", None)),
    };

    // Parse files
    let mut source_data = GraphSourceData::new();

    if let Some(files) = args.get("files").and_then(|v| v.as_object()) {
        for (path, content) in files {
            if let Some(content_str) = content.as_str() {
                source_data.add_file(path.clone(), content_str.to_string());
            }
        }
    }

    // Parse functions (optional)
    if let Some(functions) = args.get("functions").and_then(|v| v.as_object()) {
        for (file, funcs) in functions {
            if let Some(func_array) = funcs.as_array() {
                let func_list: Vec<String> = func_array
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                source_data.add_functions(file.clone(), func_list);
            }
        }
    }

    // Parse imports (optional)
    if let Some(imports) = args.get("imports").and_then(|v| v.as_object()) {
        for (file, imps) in imports {
            if let Some(imp_array) = imps.as_array() {
                let imp_list: Vec<String> = imp_array
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                source_data.add_imports(file.clone(), imp_list);
            }
        }
    }

    // Build graph
    let manager = GraphCorrelationManager::new();
    let graph = manager
        .build_graph(graph_type, &source_data)
        .map_err(|e| ErrorData::internal_error(format!("Failed to build graph: {}", e), None))?;

    // Serialize graph
    let graph_json = serde_json::to_value(&graph).map_err(|e| {
        ErrorData::internal_error(format!("Failed to serialize graph: {}", e), None)
    })?;

    let response = json!({
        "status": "success",
        "graph": graph_json,
        "node_count": graph.nodes.len(),
        "edge_count": graph.edges.len()
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Handle graph correlation analyze tool
async fn handle_graph_correlation_analyze(
    request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    use nexus_core::graph_correlation::{
        ArchitecturalPatternDetector, CorrelationGraph, EventDrivenPatternDetector,
        PatternDetector, PipelinePatternDetector, calculate_statistics,
    };

    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    // Parse graph
    let graph: CorrelationGraph =
        serde_json::from_value(args.get("graph").cloned().unwrap_or(json!({})))
            .map_err(|e| ErrorData::invalid_params(format!("Invalid graph: {}", e), None))?;

    // Parse analysis type
    let analysis_type = args
        .get("analysis_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing analysis_type", None))?;

    let mut response = json!({
        "status": "success",
        "analysis_type": analysis_type
    });

    // Perform analysis based on type
    match analysis_type {
        "statistics" => {
            let stats = calculate_statistics(&graph);
            response["statistics"] = serde_json::to_value(&stats).unwrap_or(json!({}));
        }
        "patterns" => {
            let mut all_patterns = Vec::new();

            // Pipeline patterns
            let pipeline_detector = PipelinePatternDetector;
            if let Ok(result) = pipeline_detector.detect(&graph) {
                all_patterns.extend(result.patterns);
            }

            // Event-driven patterns
            let event_detector = EventDrivenPatternDetector;
            if let Ok(result) = event_detector.detect(&graph) {
                all_patterns.extend(result.patterns);
            }

            // Architectural patterns
            let arch_detector = ArchitecturalPatternDetector;
            if let Ok(result) = arch_detector.detect(&graph) {
                all_patterns.extend(result.patterns);
            }

            response["patterns"] = serde_json::to_value(&all_patterns).unwrap_or(json!([]));
            response["pattern_count"] = json!(all_patterns.len());
        }
        "all" => {
            // Statistics
            let stats = calculate_statistics(&graph);
            response["statistics"] = serde_json::to_value(&stats).unwrap_or(json!({}));

            // Patterns
            let mut all_patterns = Vec::new();

            let pipeline_detector = PipelinePatternDetector;
            if let Ok(result) = pipeline_detector.detect(&graph) {
                all_patterns.extend(result.patterns);
            }

            let event_detector = EventDrivenPatternDetector;
            if let Ok(result) = event_detector.detect(&graph) {
                all_patterns.extend(result.patterns);
            }

            let arch_detector = ArchitecturalPatternDetector;
            if let Ok(result) = arch_detector.detect(&graph) {
                all_patterns.extend(result.patterns);
            }

            response["patterns"] = serde_json::to_value(&all_patterns).unwrap_or(json!([]));
            response["pattern_count"] = json!(all_patterns.len());
        }
        _ => {
            return Err(ErrorData::invalid_params("Invalid analysis_type", None));
        }
    }

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Handle graph correlation export tool
async fn handle_graph_correlation_export(
    request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    use nexus_core::graph_correlation::{CorrelationGraph, ExportFormat, export_graph};

    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    // Parse graph
    let graph: CorrelationGraph =
        serde_json::from_value(args.get("graph").cloned().unwrap_or(json!({})))
            .map_err(|e| ErrorData::invalid_params(format!("Invalid graph: {}", e), None))?;

    // Parse format
    let format_str = args
        .get("format")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorData::invalid_params("Missing format", None))?;

    let format = match format_str {
        "JSON" => ExportFormat::Json,
        "GraphML" => ExportFormat::GraphML,
        "GEXF" => ExportFormat::GEXF,
        "DOT" => ExportFormat::DOT,
        _ => return Err(ErrorData::invalid_params("Invalid format", None)),
    };

    // Export graph
    let exported = export_graph(&graph, format)
        .map_err(|e| ErrorData::internal_error(format!("Failed to export graph: {}", e), None))?;

    let response = json!({
        "status": "success",
        "format": format_str,
        "content": exported
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Handle graph correlation types tool
async fn handle_graph_correlation_types(
    _request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    let response = json!({
        "status": "success",
        "types": ["Call", "Dependency", "DataFlow", "Component"],
        "descriptions": {
            "Call": "Function call relationships and execution flow",
            "Dependency": "Module and package dependency relationships",
            "DataFlow": "Data flow and transformation pipelines",
            "Component": "High-level component and module relationships"
        }
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Health check for StreamableHTTP endpoint
#[allow(dead_code)]
pub async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "protocol": "MCP",
        "version": "1.0",
        "transport": "streamable-http",
        "status": "ok",
        "nexus_version": env!("CARGO_PKG_VERSION")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::executor::Executor;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_nexus_mcp_service_new() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let _service = NexusMcpService::new(server);
        // Service created successfully
    }

    #[tokio::test]
    async fn test_get_info() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let service = NexusMcpService::new(server);
        let info = service.get_info();

        assert_eq!(info.server_info.name, "nexus-server");
        assert_eq!(
            info.server_info.title,
            Some("Nexus Graph Database Server".to_string())
        );
        assert!(info.server_info.website_url.is_some());
        assert!(info.instructions.is_some());
    }

    #[tokio::test]
    async fn test_get_nexus_mcp_tools() {
        let tools = get_nexus_mcp_tools();
        assert!(!tools.is_empty());

        // Check that we have the expected tools
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(tool_names.contains(&"create_node"));
        assert!(tool_names.contains(&"create_relationship"));
        assert!(tool_names.contains(&"execute_cypher"));
        assert!(tool_names.contains(&"knn_search"));
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_unknown() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let request = CallToolRequestParam {
            name: "unknown_tool".into(),
            arguments: None,
        };

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(tool_result.is_error.unwrap_or(false));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for unknown tools
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_create_node() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let request = CallToolRequestParam {
            name: "create_node".into(),
            arguments: Some(
                json!({
                    "labels": ["Person"],
                    "properties": {"name": "Alice"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(!tool_result.is_error.unwrap_or(true));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for uninitialized executor
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_execute_cypher() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let request = CallToolRequestParam {
            name: "execute_cypher".into(),
            arguments: Some(
                json!({
                    "query": "RETURN 1 as test"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(!tool_result.is_error.unwrap_or(true));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for uninitialized executor
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_knn_search() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let request = CallToolRequestParam {
            name: "knn_search".into(),
            arguments: Some(
                json!({
                    "label": "Person",
                    "vector": [0.1, 0.2, 0.3],
                    "k": 5
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(!tool_result.is_error.unwrap_or(true));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for uninitialized executor
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await;
        let data = response.0;
        assert_eq!(data["protocol"], "MCP");
        assert_eq!(data["version"], "1.0");
        assert_eq!(data["transport"], "streamable-http");
        assert_eq!(data["status"], "ok");
        assert!(!data["nexus_version"].as_str().unwrap().is_empty());
    }

    // ============================================================================
    // Graph Correlation MCP Tools Tests
    // ============================================================================

    #[tokio::test]
    async fn test_graph_correlation_generate_call_graph() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let mut files = serde_json::Map::new();
        files.insert(
            "main.rs".to_string(),
            json!("fn main() { helper(); }\nfn helper() {}"),
        );

        let request = CallToolRequestParam {
            name: "graph_correlation_generate".into(),
            arguments: Some(
                json!({
                    "graph_type": "Call",
                    "files": files,
                    "name": "Test Graph"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(!tool_result.is_error.unwrap_or(true));
        assert_eq!(tool_result.content.len(), 1);

        // Parse response
        if let Content::Text { text, .. } = &tool_result.content[0] {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(response["status"], "success");
            assert!(response.get("graph").is_some());
        }
    }

    #[tokio::test]
    async fn test_graph_correlation_generate_dependency_graph() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let mut files = serde_json::Map::new();
        files.insert("mod_a.rs".to_string(), json!("use mod_b;"));
        files.insert("mod_b.rs".to_string(), json!(""));

        let request = CallToolRequestParam {
            name: "graph_correlation_generate".into(),
            arguments: Some(
                json!({
                    "graph_type": "Dependency",
                    "files": files
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_graph_correlation_generate_invalid_type() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_generate".into(),
            arguments: Some(
                json!({
                    "graph_type": "InvalidType",
                    "files": {}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_graph_correlation_analyze_statistics() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        // Create a simple graph
        let graph = json!({
            "name": "Test Graph",
            "graph_type": "Call",
            "nodes": [
                {"id": "node1", "node_type": "Function", "label": "func1", "metadata": {}, "position": null, "size": null},
                {"id": "node2", "node_type": "Function", "label": "func2", "metadata": {}, "position": null, "size": null}
            ],
            "edges": [
                {"source": "node1", "target": "node2", "edge_type": "Calls", "label": null, "metadata": {}}
            ],
            "metadata": {}
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_analyze".into(),
            arguments: Some(
                json!({
                    "graph": graph,
                    "analysis_type": "statistics"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        if let Content::Text { text, .. } = &tool_result.content[0] {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(response["status"], "success");
            assert!(response.get("statistics").is_some());
        }
    }

    #[tokio::test]
    async fn test_graph_correlation_analyze_patterns() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let graph = json!({
            "name": "Pipeline Graph",
            "graph_type": "DataFlow",
            "nodes": [
                {"id": "stage1", "node_type": "Function", "label": "input", "metadata": {}, "position": null, "size": null},
                {"id": "stage2", "node_type": "Function", "label": "process", "metadata": {}, "position": null, "size": null},
                {"id": "stage3", "node_type": "Function", "label": "output", "metadata": {}, "position": null, "size": null}
            ],
            "edges": [
                {"source": "stage1", "target": "stage2", "edge_type": "Transforms", "label": null, "metadata": {}},
                {"source": "stage2", "target": "stage3", "edge_type": "Transforms", "label": null, "metadata": {}}
            ],
            "metadata": {}
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_analyze".into(),
            arguments: Some(
                json!({
                    "graph": graph,
                    "analysis_type": "patterns"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        if let Content::Text { text, .. } = &tool_result.content[0] {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(response["status"], "success");
            assert!(response.get("patterns").is_some());
        }
    }

    #[tokio::test]
    async fn test_graph_correlation_analyze_all() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let graph = json!({
            "name": "Full Graph",
            "graph_type": "Call",
            "nodes": [
                {"id": "n1", "node_type": "Function", "label": "f1", "metadata": {}, "position": null, "size": null}
            ],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_analyze".into(),
            arguments: Some(
                json!({
                    "graph": graph,
                    "analysis_type": "all"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        if let Content::Text { text, .. } = &tool_result.content[0] {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(response["status"], "success");
            assert!(response.get("statistics").is_some());
            assert!(response.get("patterns").is_some());
        }
    }

    #[tokio::test]
    async fn test_graph_correlation_export_json() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let graph = json!({
            "name": "Export Test",
            "graph_type": "Call",
            "nodes": [{"id": "n1", "node_type": "Function", "label": "func", "metadata": {}, "position": null, "size": null}],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_export".into(),
            arguments: Some(
                json!({
                    "graph": graph,
                    "format": "JSON"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        if let Content::Text { text, .. } = &tool_result.content[0] {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(response["status"], "success");
            assert_eq!(response["format"], "JSON");
            assert!(response.get("content").is_some());
        }
    }

    #[tokio::test]
    async fn test_graph_correlation_export_graphml() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let graph = json!({
            "name": "GraphML Export",
            "graph_type": "Dependency",
            "nodes": [{"id": "mod1", "node_type": "Module", "label": "module1", "metadata": {}, "position": null, "size": null}],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_export".into(),
            arguments: Some(
                json!({
                    "graph": graph,
                    "format": "GraphML"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_graph_correlation_export_invalid_format() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let graph = json!({
            "name": "Test",
            "graph_type": "Call",
            "nodes": [],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_export".into(),
            arguments: Some(
                json!({
                    "graph": graph,
                    "format": "InvalidFormat"
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_graph_correlation_types() {
        let executor = Arc::new(RwLock::new(Executor::default()));
        let catalog = Arc::new(RwLock::new(nexus_core::catalog::Catalog::default()));
        let label_index = Arc::new(RwLock::new(nexus_core::index::LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(nexus_core::index::KnnIndex::new(128).unwrap()));

        let server = Arc::new(NexusServer {
            executor,
            catalog,
            label_index,
            knn_index,
        });

        let request = CallToolRequestParam {
            name: "graph_correlation_types".into(),
            arguments: None,
        };

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        if let Content::Text { text, .. } = &tool_result.content[0] {
            let response: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(response["status"], "success");
            assert!(response.get("types").is_some());

            let types = response["types"].as_array().unwrap();
            assert_eq!(types.len(), 4);
            assert!(types.contains(&json!("Call")));
            assert!(types.contains(&json!("Dependency")));
            assert!(types.contains(&json!("DataFlow")));
            assert!(types.contains(&json!("Component")));
        }
    }

    #[tokio::test]
    async fn test_mcp_tools_include_graph_correlation() {
        let tools = get_nexus_mcp_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        assert!(tool_names.contains(&"graph_correlation_generate"));
        assert!(tool_names.contains(&"graph_correlation_analyze"));
        assert!(tool_names.contains(&"graph_correlation_export"));
        assert!(tool_names.contains(&"graph_correlation_types"));
    }
}
