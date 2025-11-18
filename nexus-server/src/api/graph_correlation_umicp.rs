//! UMICP Protocol Integration for Graph Correlation Analysis
//!
//! Provides UMICP methods for standardized access to graph correlation analysis:
//! - graph.generate - Generate correlation graphs
//! - graph.get - Retrieve a graph by ID
//! - graph.analyze - Analyze graph patterns and statistics
//! - graph.search - Search graphs semantically
//! - graph.visualize - Generate visualization
//! - graph.patterns - Detect patterns in graphs
//! - graph.export - Export graphs to various formats

use axum::{extract::Json, response::Json as AxumJson};
use nexus_core::graph::correlation::visualization::{
    VisualizationConfig, apply_layout, render_graph_to_svg,
};
use nexus_core::graph::correlation::{
    ArchitecturalPatternDetector, CorrelationGraph, EventDrivenPatternDetector,
    ExportFormat as GraphExportFormat, GraphCorrelationManager, GraphSourceData, GraphType,
    PatternDetector, PipelinePatternDetector, calculate_statistics, export_graph,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// UMICP request structure
#[derive(Debug, Deserialize)]
pub struct UmicpRequest {
    /// UMICP method name
    pub method: String,
    /// Method parameters
    pub params: Option<serde_json::Value>,
    /// Request context (optional)
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

/// UMICP response structure
#[derive(Debug, Serialize)]
pub struct UmicpResponse {
    /// Response result
    pub result: Option<serde_json::Value>,
    /// Error information if any
    pub error: Option<UmicpError>,
    /// Response context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// UMICP error structure
#[derive(Debug, Serialize)]
pub struct UmicpError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl UmicpResponse {
    /// Create a successful response
    pub fn success(result: serde_json::Value) -> Self {
        Self {
            result: Some(result),
            error: None,
            context: None,
        }
    }

    /// Create an error response
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            result: None,
            error: Some(UmicpError {
                code: code.into(),
                message: message.into(),
                data: None,
            }),
            context: None,
        }
    }
}

/// Graph UMICP handler
pub struct GraphUmicpHandler {
    /// Graph correlation manager
    manager: Arc<Mutex<GraphCorrelationManager>>,
    /// In-memory graph storage (graph_id -> graph)
    graphs: Arc<Mutex<HashMap<String, CorrelationGraph>>>,
}

impl GraphUmicpHandler {
    /// Create a new UMICP handler
    pub fn new() -> Self {
        Self {
            manager: Arc::new(Mutex::new(GraphCorrelationManager::new())),
            graphs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Handle UMICP request
    pub async fn handle_request(&self, request: UmicpRequest) -> UmicpResponse {
        match request.method.as_str() {
            "graph.generate" => self.handle_generate(request.params).await,
            "graph.get" => self.handle_get(request.params).await,
            "graph.analyze" => self.handle_analyze(request.params).await,
            "graph.search" => self.handle_search(request.params).await,
            "graph.visualize" => self.handle_visualize(request.params).await,
            "graph.patterns" => self.handle_patterns(request.params).await,
            "graph.export" => self.handle_export(request.params).await,
            _ => UmicpResponse::error(
                "METHOD_NOT_FOUND",
                format!("Unknown UMICP method: {}", request.method),
            ),
        }
    }

    /// Handle graph.generate method
    async fn handle_generate(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing parameters");
            }
        };

        let graph_type_str = match params.get("graph_type").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing graph_type parameter");
            }
        };

        let graph_type = match graph_type_str {
            "Call" => GraphType::Call,
            "Dependency" => GraphType::Dependency,
            "DataFlow" => GraphType::DataFlow,
            "Component" => GraphType::Component,
            _ => {
                return UmicpResponse::error(
                    "INVALID_PARAMS",
                    format!("Invalid graph_type: {}", graph_type_str),
                );
            }
        };

        // Parse files
        let mut source_data = GraphSourceData::new();
        if let Some(files) = params.get("files").and_then(|v| v.as_object()) {
            for (path, content) in files {
                if let Some(content_str) = content.as_str() {
                    source_data.add_file(path.clone(), content_str.to_string());
                }
            }
        }

        // Parse functions (optional)
        if let Some(functions) = params.get("functions").and_then(|v| v.as_object()) {
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
        if let Some(imports) = params.get("imports").and_then(|v| v.as_object()) {
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
        let manager = self.manager.lock().unwrap();
        let graph = match manager.build_graph(graph_type, &source_data) {
            Ok(g) => g,
            Err(e) => {
                return UmicpResponse::error("GRAPH_GENERATION_FAILED", format!("{}", e));
            }
        };

        // Generate graph ID and store
        let graph_id = format!("graph_{}", uuid::Uuid::new_v4());
        self.graphs
            .lock()
            .unwrap()
            .insert(graph_id.clone(), graph.clone());

        let result = serde_json::json!({
            "graph_id": graph_id,
            "graph": graph,
            "node_count": graph.nodes.len(),
            "edge_count": graph.edges.len(),
        });

        UmicpResponse::success(result)
    }

    /// Handle graph.get method
    async fn handle_get(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing parameters");
            }
        };

        let graph_id = match params.get("graph_id").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing graph_id parameter");
            }
        };

        let graphs = self.graphs.lock().unwrap();
        match graphs.get(graph_id) {
            Some(graph) => {
                let result = serde_json::json!({
                    "graph": graph,
                    "node_count": graph.nodes.len(),
                    "edge_count": graph.edges.len(),
                });
                UmicpResponse::success(result)
            }
            None => {
                UmicpResponse::error("GRAPH_NOT_FOUND", format!("Graph {} not found", graph_id))
            }
        }
    }

    /// Handle graph.analyze method
    async fn handle_analyze(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing parameters");
            }
        };

        // Get graph (either from graph_id or inline)
        let graph: CorrelationGraph = if let Some(graph_id) =
            params.get("graph_id").and_then(|v| v.as_str())
        {
            let graphs = self.graphs.lock().unwrap();
            match graphs.get(graph_id) {
                Some(g) => g.clone(),
                None => {
                    return UmicpResponse::error(
                        "GRAPH_NOT_FOUND",
                        format!("Graph {} not found", graph_id),
                    );
                }
            }
        } else if let Some(graph_obj) = params.get("graph") {
            match serde_json::from_value(graph_obj.clone()) {
                Ok(g) => g,
                Err(e) => {
                    return UmicpResponse::error("INVALID_PARAMS", format!("Invalid graph: {}", e));
                }
            }
        } else {
            return UmicpResponse::error("INVALID_PARAMS", "Missing graph or graph_id parameter");
        };

        let analysis_type = params
            .get("analysis_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let mut result = serde_json::json!({
            "analysis_type": analysis_type
        });

        match analysis_type {
            "statistics" => {
                let stats = calculate_statistics(&graph);
                result["statistics"] = serde_json::to_value(&stats).unwrap_or(json!({}));
            }
            "patterns" => {
                let mut all_patterns = Vec::new();

                let pipeline_detector = PipelinePatternDetector;
                if let Ok(detection_result) = pipeline_detector.detect(&graph) {
                    all_patterns.extend(detection_result.patterns);
                }

                let event_detector = EventDrivenPatternDetector;
                if let Ok(detection_result) = event_detector.detect(&graph) {
                    all_patterns.extend(detection_result.patterns);
                }

                let arch_detector = ArchitecturalPatternDetector;
                if let Ok(detection_result) = arch_detector.detect(&graph) {
                    all_patterns.extend(detection_result.patterns);
                }

                result["patterns"] = serde_json::to_value(&all_patterns).unwrap_or(json!([]));
                result["pattern_count"] = json!(all_patterns.len());
            }
            "all" => {
                let stats = calculate_statistics(&graph);
                result["statistics"] = serde_json::to_value(&stats).unwrap_or(json!({}));

                let mut all_patterns = Vec::new();
                let pipeline_detector = PipelinePatternDetector;
                if let Ok(detection_result) = pipeline_detector.detect(&graph) {
                    all_patterns.extend(detection_result.patterns);
                }
                let event_detector = EventDrivenPatternDetector;
                if let Ok(detection_result) = event_detector.detect(&graph) {
                    all_patterns.extend(detection_result.patterns);
                }
                let arch_detector = ArchitecturalPatternDetector;
                if let Ok(detection_result) = arch_detector.detect(&graph) {
                    all_patterns.extend(detection_result.patterns);
                }

                result["patterns"] = serde_json::to_value(&all_patterns).unwrap_or(json!([]));
                result["pattern_count"] = json!(all_patterns.len());
            }
            _ => {
                return UmicpResponse::error(
                    "INVALID_PARAMS",
                    format!("Invalid analysis_type: {}", analysis_type),
                );
            }
        }

        UmicpResponse::success(result)
    }

    /// Handle graph.search method (semantic search placeholder)
    async fn handle_search(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing parameters");
            }
        };

        let query = match params.get("query").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing query parameter");
            }
        };

        // For now, return empty results - full implementation would use Vectorizer
        let result = serde_json::json!({
            "query": query,
            "results": [],
            "count": 0
        });

        UmicpResponse::success(result)
    }

    /// Handle graph.visualize method
    async fn handle_visualize(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing parameters");
            }
        };

        // Get graph
        let graph: CorrelationGraph = if let Some(graph_id) =
            params.get("graph_id").and_then(|v| v.as_str())
        {
            let graphs = self.graphs.lock().unwrap();
            match graphs.get(graph_id) {
                Some(g) => g.clone(),
                None => {
                    return UmicpResponse::error(
                        "GRAPH_NOT_FOUND",
                        format!("Graph {} not found", graph_id),
                    );
                }
            }
        } else if let Some(graph_obj) = params.get("graph") {
            match serde_json::from_value(graph_obj.clone()) {
                Ok(g) => g,
                Err(e) => {
                    return UmicpResponse::error("INVALID_PARAMS", format!("Invalid graph: {}", e));
                }
            }
        } else {
            return UmicpResponse::error("INVALID_PARAMS", "Missing graph or graph_id parameter");
        };

        // Configure visualization
        let mut config = VisualizationConfig::default();
        if let Some(width) = params.get("width").and_then(|v| v.as_f64()) {
            config.width = width as f32;
        }
        if let Some(height) = params.get("height").and_then(|v| v.as_f64()) {
            config.height = height as f32;
        }

        // Apply layout
        let mut graph_with_layout = graph.clone();
        if let Err(e) = apply_layout(&mut graph_with_layout, &config) {
            return UmicpResponse::error("VISUALIZATION_FAILED", format!("Layout failed: {}", e));
        }

        // Render to SVG
        let svg = match render_graph_to_svg(&graph_with_layout, &config) {
            Ok(s) => s,
            Err(e) => {
                return UmicpResponse::error(
                    "VISUALIZATION_FAILED",
                    format!("Rendering failed: {}", e),
                );
            }
        };

        let result = serde_json::json!({
            "svg": svg,
            "width": config.width,
            "height": config.height,
            "node_count": graph.nodes.len(),
            "edge_count": graph.edges.len(),
        });

        UmicpResponse::success(result)
    }

    /// Handle graph.patterns method
    async fn handle_patterns(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        // Similar to analyze with analysis_type="patterns"
        let mut analyze_params = params.unwrap_or(json!({}));
        analyze_params["analysis_type"] = json!("patterns");
        self.handle_analyze(Some(analyze_params)).await
    }

    /// Handle graph.export method
    async fn handle_export(&self, params: Option<serde_json::Value>) -> UmicpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return UmicpResponse::error("INVALID_PARAMS", "Missing parameters");
            }
        };

        // Get graph
        let graph: CorrelationGraph = if let Some(graph_id) =
            params.get("graph_id").and_then(|v| v.as_str())
        {
            let graphs = self.graphs.lock().unwrap();
            match graphs.get(graph_id) {
                Some(g) => g.clone(),
                None => {
                    return UmicpResponse::error(
                        "GRAPH_NOT_FOUND",
                        format!("Graph {} not found", graph_id),
                    );
                }
            }
        } else if let Some(graph_obj) = params.get("graph") {
            match serde_json::from_value(graph_obj.clone()) {
                Ok(g) => g,
                Err(e) => {
                    return UmicpResponse::error("INVALID_PARAMS", format!("Invalid graph: {}", e));
                }
            }
        } else {
            return UmicpResponse::error("INVALID_PARAMS", "Missing graph or graph_id parameter");
        };

        // Parse format
        let format_str = params
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("JSON");

        let format = match format_str {
            "JSON" => GraphExportFormat::Json,
            "GraphML" => GraphExportFormat::GraphML,
            "GEXF" => GraphExportFormat::GEXF,
            "DOT" => GraphExportFormat::DOT,
            _ => {
                return UmicpResponse::error(
                    "INVALID_PARAMS",
                    format!("Invalid format: {}", format_str),
                );
            }
        };

        // Export graph
        let exported = match export_graph(&graph, format) {
            Ok(e) => e,
            Err(e) => {
                return UmicpResponse::error("EXPORT_FAILED", format!("Export failed: {}", e));
            }
        };

        let result = serde_json::json!({
            "format": format_str,
            "content": exported,
            "size_bytes": exported.len(),
        });

        UmicpResponse::success(result)
    }
}

impl Default for GraphUmicpHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Global UMICP handler instance
static UMICP_HANDLER: std::sync::OnceLock<Arc<GraphUmicpHandler>> = std::sync::OnceLock::new();

/// Initialize UMICP handler
pub fn init_umicp_handler(handler: Arc<GraphUmicpHandler>) -> anyhow::Result<()> {
    UMICP_HANDLER
        .set(handler)
        .map_err(|_| anyhow::anyhow!("Failed to set UMICP handler"))?;
    Ok(())
}

/// Get UMICP handler instance
fn get_umicp_handler() -> Arc<GraphUmicpHandler> {
    UMICP_HANDLER
        .get_or_init(|| Arc::new(GraphUmicpHandler::new()))
        .clone()
}

/// Handle UMICP request endpoint
pub async fn handle_umicp_request(Json(request): Json<UmicpRequest>) -> AxumJson<UmicpResponse> {
    let handler = get_umicp_handler();
    let response = handler.handle_request(request).await;
    AxumJson(response)
}
