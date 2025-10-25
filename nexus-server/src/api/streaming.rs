//! StreamableHTTP implementation for Nexus
//!
//! This module provides StreamableHTTP support for the Nexus graph database,
//! enabling high-performance streaming communication over HTTP using MCP protocol.
//!
//! Based on Vectorizer's implementation using rmcp with transport-streamable-http-server.

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

    // Get or create label IDs
    let mut label_ids = Vec::new();
    {
        let catalog = server.catalog.read().await;
        for label in &labels {
            let label_id = catalog.get_or_create_label(label)?;
            label_ids.push(label_id);
        }
    }

    // Create node in storage
    let node_id = {
        let mut catalog = server.catalog.write().await;
        catalog.increment_node_count();
        catalog.get_node_count()
    };

    // Add to label index
    {
        let mut label_index = server.label_index.write().await;
        for label_id in &label_ids {
            label_index.add_node(*label_id, node_id);
        }
    }

    let response = json!({
        "status": "created",
        "node_id": node_id,
        "labels": labels,
        "label_ids": label_ids,
        "properties": properties
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
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
        .ok_or_else(|| ErrorData::invalid_params("Missing rel_type", None))?;

    let properties = args.get("properties").cloned().unwrap_or(json!({}));

    // Get or create relationship type ID
    let rel_type_id = {
        let mut catalog = server.catalog.write().await;
        catalog.get_or_create_rel_type(rel_type)?
    };

    // Create relationship in storage
    let rel_id = {
        let mut catalog = server.catalog.write().await;
        catalog.increment_rel_count();
        catalog.get_rel_count()
    };

    let response = json!({
        "status": "created",
        "relationship_id": rel_id,
        "source_id": source_id,
        "target_id": target_id,
        "rel_type": rel_type,
        "rel_type_id": rel_type_id,
        "properties": properties
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
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
    let executor = server.executor.read().await;
    let query_obj = nexus_core::executor::Query::new(query.to_string());
    
    let result = executor.execute(&query_obj).map_err(|e| {
        ErrorData::internal_error(format!("Cypher execution failed: {}", e), None)
    })?;

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    // Convert result to JSON
    let mut rows = Vec::new();
    for row in &result.rows {
        let mut row_obj = serde_json::Map::new();
        for (i, value) in row.values.iter().enumerate() {
            if i < result.columns.len() {
                let column_name = &result.columns[i];
                row_obj.insert(column_name.clone(), serde_json::to_value(value).unwrap_or(json!(null)));
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

    let start_time = std::time::Instant::now();

    // Get label ID from catalog
    let label_id = {
        let catalog = server.catalog.read().await;
        catalog.get_label_id(label).ok_or_else(|| {
            ErrorData::invalid_params(format!("Label '{}' not found", label), None)
        })?
    };

    // Get nodes with this label from label index
    let node_ids = {
        let label_index = server.label_index.read().await;
        label_index.get_nodes_with_label(label_id)
    };

    if node_ids.is_empty() {
        let response = json!({
            "status": "completed",
            "label": label,
            "label_id": label_id,
            "k": k,
            "limit": limit,
            "results": [],
            "total_nodes": 0,
            "execution_time_ms": start_time.elapsed().as_millis() as u64
        });

        return Ok(CallToolResult::success(vec![Content::text(
            response.to_string(),
        )]));
    }

    // Perform KNN search
    let knn_index = server.knn_index.read().await;
    let search_results = knn_index.search(&vector, k.min(node_ids.len())).map_err(|e| {
        ErrorData::internal_error(format!("KNN search failed: {}", e), None)
    })?;

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    // Convert results to JSON
    let mut results = Vec::new();
    for (i, (node_id, score)) in search_results.iter().enumerate() {
        if i >= limit {
            break;
        }
        
        results.push(json!({
            "node_id": node_id,
            "score": score,
            "rank": i + 1
        }));
    }

    let response = json!({
        "status": "completed",
        "label": label,
        "label_id": label_id,
        "k": k,
        "limit": limit,
        "results": results,
        "total_nodes": node_ids.len(),
        "execution_time_ms": execution_time_ms
    });

    Ok(CallToolResult::success(vec![Content::text(
        response.to_string(),
    )]))
}

/// Handle get stats tool
async fn handle_get_stats(
    _request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    // TODO: Implement actual stats collection
    let response = json!({
        "status": "ok",
        "stats": {
            "node_count": 0,
            "relationship_count": 0,
            "label_count": 0,
            "index_count": 0,
            "memory_usage_mb": 0,
            "uptime_seconds": 0
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
