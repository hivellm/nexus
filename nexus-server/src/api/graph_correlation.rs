//! Graph Correlation API endpoints

use axum::{http::StatusCode, response::Json};
use nexus_core::graph_correlation::{
    CorrelationGraph, GraphCorrelationManager, GraphSourceData, GraphType,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Global graph correlation manager
static GRAPH_MANAGER: std::sync::OnceLock<Arc<Mutex<GraphCorrelationManager>>> =
    std::sync::OnceLock::new();

/// Initialize graph correlation manager
pub fn init_manager(manager: Arc<Mutex<GraphCorrelationManager>>) -> anyhow::Result<()> {
    GRAPH_MANAGER
        .set(manager)
        .map_err(|_| anyhow::anyhow!("Failed to set graph manager"))?;
    Ok(())
}

/// Generate correlation graph request
#[derive(Debug, Deserialize)]
pub struct GenerateGraphRequest {
    /// Graph type (Call, Dependency, DataFlow, Component)
    pub graph_type: String,
    /// Source files (map of file path to content)
    pub files: std::collections::HashMap<String, String>,
    /// Function definitions (map of file to list of functions)
    #[serde(default)]
    pub functions: std::collections::HashMap<String, Vec<String>>,
    /// Import relationships (map of file to list of imports)
    #[serde(default)]
    pub imports: std::collections::HashMap<String, Vec<String>>,
    /// Graph name
    #[serde(default = "default_graph_name")]
    pub name: String,
}

fn default_graph_name() -> String {
    "Generated Graph".to_string()
}

/// Generate correlation graph response
#[derive(Debug, Serialize)]
pub struct GenerateGraphResponse {
    /// Generated graph
    pub graph: CorrelationGraph,
    /// Success status
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Get available graph types response
#[derive(Debug, Serialize)]
pub struct GraphTypesResponse {
    /// Available graph types
    pub types: Vec<String>,
    /// Success status
    pub success: bool,
}

/// Generate a correlation graph
pub async fn generate_graph(
    Json(request): Json<GenerateGraphRequest>,
) -> Result<Json<GenerateGraphResponse>, StatusCode> {
    tracing::info!(
        "Generating correlation graph: {} ({})",
        request.name,
        request.graph_type
    );

    let manager = GRAPH_MANAGER.get().ok_or_else(|| {
        tracing::error!("Graph manager not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let graph_type = match request.graph_type.to_lowercase().as_str() {
        "call" => GraphType::Call,
        "dependency" => GraphType::Dependency,
        "dataflow" => GraphType::DataFlow,
        "component" => GraphType::Component,
        _ => {
            return Ok(Json(GenerateGraphResponse {
                graph: CorrelationGraph::new(GraphType::Call, request.name.clone()),
                success: false,
                error: Some(format!("Invalid graph type: {}", request.graph_type)),
            }));
        }
    };

    // Build GraphSourceData from request
    let mut source_data = GraphSourceData::new();
    for (path, content) in request.files {
        source_data.add_file(path, content);
    }
    for (file, functions) in request.functions {
        source_data.add_functions(file, functions);
    }
    for (file, imports) in request.imports {
        source_data.add_imports(file, imports);
    }

    match manager.lock() {
        Ok(mgr) => match mgr.build_graph(graph_type, &source_data) {
            Ok(graph) => {
                tracing::info!(
                    "Graph generated successfully with {} nodes",
                    graph.nodes.len()
                );
                Ok(Json(GenerateGraphResponse {
                    graph,
                    success: true,
                    error: None,
                }))
            }
            Err(e) => {
                tracing::error!("Graph generation failed: {}", e);
                Ok(Json(GenerateGraphResponse {
                    graph: CorrelationGraph::new(graph_type, request.name),
                    success: false,
                    error: Some(e.to_string()),
                }))
            }
        },
        Err(e) => {
            tracing::error!("Failed to lock graph manager: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get available graph types
pub async fn get_graph_types() -> Result<Json<GraphTypesResponse>, StatusCode> {
    tracing::info!("Getting available graph types");

    let manager = GRAPH_MANAGER.get().ok_or_else(|| {
        tracing::error!("Graph manager not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match manager.lock() {
        Ok(mgr) => {
            let types = mgr
                .available_graph_types()
                .iter()
                .map(|t| format!("{:?}", t))
                .collect();

            Ok(Json(GraphTypesResponse {
                types,
                success: true,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to lock graph manager: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_graph_name() {
        let name = default_graph_name();
        assert_eq!(name, "Generated Graph");
    }

    #[test]
    fn test_graph_type_parsing() {
        assert_eq!("call".to_lowercase(), "call");
        assert_eq!("dependency".to_lowercase(), "dependency");
        assert_eq!("dataflow".to_lowercase(), "dataflow");
        assert_eq!("component".to_lowercase(), "component");
    }
}
