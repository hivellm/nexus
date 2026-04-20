//! Graph Correlation API endpoints

use axum::extract::State;
use axum::{http::StatusCode, response::Json};
use nexus_core::graph::correlation::{CorrelationGraph, GraphSourceData, GraphType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::NexusServer;

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
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<GenerateGraphRequest>,
) -> Result<Json<GenerateGraphResponse>, StatusCode> {
    tracing::info!(
        "Generating correlation graph: {} ({})",
        request.name,
        request.graph_type
    );

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

    match server.graph_correlation_manager.lock() {
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
pub async fn get_graph_types(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<GraphTypesResponse>, StatusCode> {
    tracing::info!("Getting available graph types");

    match server.graph_correlation_manager.lock() {
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
    use parking_lot::RwLock as PlRwLock;
    use tokio::sync::RwLock as TokioRwLock;

    fn build_test_server() -> Arc<NexusServer> {
        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_isolated_catalog(ctx.path()).expect("engine init");
        let engine_arc = Arc::new(TokioRwLock::new(engine));
        let executor = Arc::new(nexus_core::executor::Executor::default());
        let dbm = Arc::new(PlRwLock::new(
            nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).expect("dbm init"),
        ));
        let rbac = Arc::new(TokioRwLock::new(
            nexus_core::auth::RoleBasedAccessControl::new(),
        ));
        let auth_mgr = Arc::new(nexus_core::auth::AuthManager::new(
            nexus_core::auth::AuthConfig::default(),
        ));
        let jwt = Arc::new(nexus_core::auth::JwtManager::new(
            nexus_core::auth::JwtConfig::default(),
        ));
        let audit = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: ctx.path().join("audit"),
                retention_days: 1,
                compress_logs: false,
            })
            .expect("audit init"),
        );
        let _leaked = Box::leak(Box::new(ctx));

        Arc::new(NexusServer::new(
            executor,
            engine_arc,
            dbm,
            rbac,
            auth_mgr,
            jwt,
            audit,
            crate::config::RootUserConfig::default(),
        ))
    }

    #[test]
    fn test_default_graph_name() {
        let name = default_graph_name();
        assert_eq!(name, "Generated Graph");
    }

    #[tokio::test]
    async fn test_generate_graph_rejects_unknown_type() {
        let server = build_test_server();
        let req = GenerateGraphRequest {
            graph_type: "not-a-type".to_string(),
            files: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            imports: std::collections::HashMap::new(),
            name: "t".to_string(),
        };
        let resp = generate_graph(State(server), Json(req)).await.expect("ok");
        assert!(!resp.0.success);
        assert!(
            resp.0
                .error
                .as_ref()
                .unwrap()
                .contains("Invalid graph type")
        );
    }

    #[tokio::test]
    async fn test_get_graph_types_lists_known_types() {
        let server = build_test_server();
        let resp = get_graph_types(State(server)).await.expect("ok");
        assert!(resp.0.success);
        assert!(!resp.0.types.is_empty());
    }
}
