//! MCP tool handler implementations.

use std::collections::HashMap;
use std::sync::Arc;

use axum::response::Json;
use rmcp::model::{CallToolRequestParam, CallToolResult, Content, ErrorData};
use serde_json::json;

use crate::NexusServer;
use nexus_core::executor::Query as CypherQuery;

// ============================================================================
// Graph operation handlers
// ============================================================================

/// Handle create node tool
pub(super) async fn handle_create_node(
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

    // Use Engine to create node directly
    let mut engine = server.engine.write().await;

    match engine.create_node(labels.clone(), properties.clone()) {
        Ok(node_id) => {
            let response = json!({
                "status": "created",
                "node_id": node_id,
                "labels": labels,
                "properties": properties
            });
            Ok(CallToolResult::success(vec![Content::text(
                response.to_string(),
            )]))
        }
        Err(e) => Err(ErrorData::internal_error(
            format!("Failed to create node: {}", e),
            None,
        )),
    }
}

/// Handle create relationship tool
pub(super) async fn handle_create_relationship(
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
    let executor = server.executor.clone();

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
pub(super) async fn handle_execute_cypher(
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

    // Check if query contains CREATE - if so, use Engine for actual node creation
    let is_create_query = query.trim().to_uppercase().starts_with("CREATE");

    if is_create_query {
        // Parse and execute CREATE using Engine
        use nexus_core::executor::parser::CypherParser;

        let mut parser = CypherParser::new(query.to_string());
        let ast = parser
            .parse()
            .map_err(|e| ErrorData::internal_error(format!("Parse error: {}", e), None))?;

        // Execute CREATE clauses using Engine
        let mut engine = server.engine.write().await;
        for clause in &ast.clauses {
            if let nexus_core::executor::parser::Clause::Create(create_clause) = clause {
                // Extract pattern and create nodes
                for element in &create_clause.pattern.elements {
                    if let nexus_core::executor::parser::PatternElement::Node(node_pattern) =
                        element
                    {
                        let labels = node_pattern.labels.clone();

                        // Convert properties
                        let mut props = serde_json::Map::new();
                        if let Some(prop_map) = &node_pattern.properties {
                            for (key, expr) in &prop_map.properties {
                                // Convert expression to JSON value
                                let value = match expr {
                                    nexus_core::executor::parser::Expression::Literal(lit) => {
                                        match lit {
                                            nexus_core::executor::parser::Literal::String(s) => {
                                                serde_json::Value::String(s.clone())
                                            }
                                            nexus_core::executor::parser::Literal::Integer(i) => {
                                                serde_json::Value::Number((*i).into())
                                            }
                                            nexus_core::executor::parser::Literal::Float(f) => {
                                                serde_json::Number::from_f64(*f)
                                                    .map(serde_json::Value::Number)
                                                    .unwrap_or(serde_json::Value::Null)
                                            }
                                            nexus_core::executor::parser::Literal::Boolean(b) => {
                                                serde_json::Value::Bool(*b)
                                            }
                                            nexus_core::executor::parser::Literal::Null => {
                                                serde_json::Value::Null
                                            }
                                            nexus_core::executor::parser::Literal::Point(p) => {
                                                p.to_json_value()
                                            }
                                        }
                                    }
                                    _ => serde_json::Value::Null,
                                };
                                props.insert(key.clone(), value);
                            }
                        }

                        let properties = serde_json::Value::Object(props);

                        // Create node using Engine
                        engine.create_node(labels, properties).map_err(|e| {
                            ErrorData::internal_error(format!("Failed to create node: {}", e), None)
                        })?;
                    }
                }
            }
        }
    }

    // Execute query normally through executor for RETURN/MATCH clauses
    let executor = server.executor.clone();
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
pub(super) async fn handle_knn_search(
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

    // Access KNN index from Engine instance
    let engine = server.engine.read().await;
    match engine.knn_search(label, &vector, k) {
        Ok(results) => {
            let results_json: Vec<_> = results
                .iter()
                .map(|(node_id, similarity)| {
                    json!({
                        "node_id": node_id,
                        "similarity": similarity,
                        "score": similarity
                    })
                })
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

    /* COMMENTED OUT - needs refactoring to use Engine's indexes
    // Use real KNN index for search
    let knn_index = server.knn_index.read().await;
    match knn_index.search_knn(&vector, k) {
        Ok(results) => {
            let results_json: Vec<_> = results
                .iter()
                .map(|(node_id, distance)| {
                    json!({
                        "node_id": node_id,
                        "distance": distance,
                        "score": 1.0 / (1.0 + distance)
                    })
                })
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
    */
}

/// Handle get stats tool
pub(super) async fn handle_get_stats(
    _request: CallToolRequestParam,
    server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    // Get stats from Engine
    let mut engine = server.engine.write().await;
    match engine.stats() {
        Ok(stats) => {
            let response = json!({
                "status": "ok",
                "stats": {
                    "node_count": stats.nodes,
                    "relationship_count": stats.relationships,
                    "label_count": stats.labels,
                    "relationship_type_count": stats.rel_types,
                    "label_index_size": 0,
                    "knn_index_size": 0,
                    "memory_usage_mb": 0,
                    "uptime_seconds": 0
                },
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            Ok(CallToolResult::success(vec![Content::text(
                response.to_string(),
            )]))
        }
        Err(e) => Err(ErrorData::internal_error(
            format!("Failed to get stats: {}", e),
            None,
        )),
    }
}

// ============================================================================
// Graph correlation handlers
// ============================================================================

/// Handle graph correlation generate tool
pub(super) async fn handle_graph_correlation_generate(
    request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    use nexus_core::graph::correlation::{GraphCorrelationManager, GraphSourceData, GraphType};

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
pub(super) async fn handle_graph_correlation_analyze(
    request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    use nexus_core::graph::correlation::{
        ArchitecturalPatternDetector, CorrelationGraph, EventDrivenPatternDetector,
        PatternDetector, PipelinePatternDetector, calculate_statistics,
    };

    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| ErrorData::invalid_params("Missing arguments", None))?;

    // Parse and normalize graph input
    let mut graph_value = args.get("graph").cloned().unwrap_or(json!({}));

    // Add missing fields with defaults to make it accept partial graphs
    if let Some(obj) = graph_value.as_object_mut() {
        obj.entry("name").or_insert(json!("Graph"));
        obj.entry("created_at")
            .or_insert_with(|| json!(chrono::Utc::now().to_rfc3339()));
        obj.entry("updated_at")
            .or_insert_with(|| json!(chrono::Utc::now().to_rfc3339()));
        obj.entry("metadata").or_insert(json!({}));
        obj.entry("description").or_insert(json!(null));

        // Normalize nodes - ensure all have required fields
        if let Some(nodes) = obj.get_mut("nodes").and_then(|v| v.as_array_mut()) {
            for node in nodes.iter_mut() {
                if let Some(node_obj) = node.as_object_mut() {
                    node_obj.entry("metadata").or_insert(json!({}));
                    node_obj.entry("color").or_insert(json!(null));
                    node_obj.entry("size").or_insert(json!(null));
                    node_obj.entry("position").or_insert(json!(null));
                }
            }
        }

        // Normalize edges - ensure all have required fields
        if let Some(edges) = obj.get_mut("edges").and_then(|v| v.as_array_mut()) {
            for edge in edges.iter_mut() {
                if let Some(edge_obj) = edge.as_object_mut() {
                    edge_obj.entry("metadata").or_insert(json!({}));
                }
            }
        }
    }

    // Now deserialize with all required fields present
    let graph: CorrelationGraph = serde_json::from_value(graph_value)
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
pub(super) async fn handle_graph_correlation_export(
    request: CallToolRequestParam,
    _server: Arc<NexusServer>,
) -> Result<CallToolResult, ErrorData> {
    use nexus_core::graph::correlation::{CorrelationGraph, ExportFormat, export_graph};

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
pub(super) async fn handle_graph_correlation_types(
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

// ============================================================================
// HTTP utility handlers
// ============================================================================

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
