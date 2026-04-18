//! Graph comparison API endpoints

use axum::extract::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json as ResponseJson;
use nexus_core::graph::comparison::{ComparisonOptions, DiffSummary, GraphComparator, GraphDiff};
use nexus_core::graph::{EdgeId, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::NexusServer;

/// Compare two graphs request
#[derive(Debug, Deserialize)]
pub struct CompareGraphsRequest {
    /// Comparison options
    #[serde(default)]
    pub options: ComparisonOptions,
}

/// Compare two graphs response
#[derive(Debug, Serialize)]
pub struct CompareGraphsResponse {
    /// The graph diff result
    pub diff: GraphDiff,
    /// Success status
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Calculate graph similarity request
#[derive(Debug, Deserialize)]
pub struct CalculateSimilarityRequest {
    /// Comparison options
    #[serde(default)]
    pub options: ComparisonOptions,
}

/// Calculate graph similarity response
#[derive(Debug, Serialize)]
pub struct CalculateSimilarityResponse {
    /// Similarity score (0.0 to 1.0)
    pub similarity: f64,
    /// Success status
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Get graph statistics request
#[derive(Debug, Deserialize)]
pub struct GetGraphStatsRequest {
    /// Graph identifier ("A" or "B")
    pub graph_id: String,
}

/// Get graph statistics response
#[derive(Debug, Serialize)]
pub struct GetGraphStatsResponse {
    /// Graph statistics
    pub stats: HashMap<String, serde_json::Value>,
    /// Success status
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

fn empty_diff() -> GraphDiff {
    GraphDiff {
        added_nodes: vec![],
        removed_nodes: vec![],
        modified_nodes: vec![],
        added_edges: vec![],
        removed_edges: vec![],
        modified_edges: vec![],
        summary: DiffSummary {
            nodes_count_original: 0,
            nodes_count_modified: 0,
            edges_count_original: 0,
            edges_count_modified: 0,
            nodes_added: 0,
            nodes_removed: 0,
            nodes_modified: 0,
            edges_added: 0,
            edges_removed: 0,
            edges_modified: 0,
            overall_similarity: 0.0,
            structural_similarity: 0.0,
            content_similarity: 0.0,
            topology_analysis: None,
            metrics_comparison: None,
        },
    }
}

/// Compare two graphs
pub async fn compare_graphs(
    State(server): State<Arc<NexusServer>>,
    Json(payload): Json<CompareGraphsRequest>,
) -> std::result::Result<ResponseJson<CompareGraphsResponse>, StatusCode> {
    tracing::info!("Comparing graphs with options: {:?}", payload.options);

    let graph_a_read = server
        .graph_a
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let graph_b_read = server
        .graph_b
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match GraphComparator::compare_graphs(&graph_a_read, &graph_b_read, &payload.options) {
        Ok(diff) => {
            tracing::info!(
                "Graph comparison completed: {} nodes added, {} removed, {} modified",
                diff.summary.nodes_added,
                diff.summary.nodes_removed,
                diff.summary.nodes_modified
            );

            Ok(ResponseJson(CompareGraphsResponse {
                diff,
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            tracing::error!("Graph comparison failed: {}", e);
            Ok(ResponseJson(CompareGraphsResponse {
                diff: empty_diff(),
                success: false,
                error: Some(e),
            }))
        }
    }
}

/// Calculate similarity between two graphs
pub async fn calculate_similarity(
    State(server): State<Arc<NexusServer>>,
    Json(payload): Json<CalculateSimilarityRequest>,
) -> std::result::Result<ResponseJson<CalculateSimilarityResponse>, StatusCode> {
    tracing::info!(
        "Calculating graph similarity with options: {:?}",
        payload.options
    );

    let graph_a_read = server
        .graph_a
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let graph_b_read = server
        .graph_b
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match GraphComparator::calculate_similarity(&graph_a_read, &graph_b_read, &payload.options) {
        Ok(similarity) => {
            tracing::info!("Graph similarity calculated: {:.4}", similarity);

            Ok(ResponseJson(CalculateSimilarityResponse {
                similarity,
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            tracing::error!("Similarity calculation failed: {}", e);
            Ok(ResponseJson(CalculateSimilarityResponse {
                similarity: 0.0,
                success: false,
                error: Some(e),
            }))
        }
    }
}

/// Get statistics for a specific graph
pub async fn get_graph_stats(
    State(server): State<Arc<NexusServer>>,
    Json(payload): Json<GetGraphStatsRequest>,
) -> std::result::Result<ResponseJson<GetGraphStatsResponse>, StatusCode> {
    tracing::info!("Getting stats for graph: {}", payload.graph_id);

    let graph = match payload.graph_id.to_uppercase().as_str() {
        "A" => Arc::clone(&server.graph_a),
        "B" => Arc::clone(&server.graph_b),
        _ => {
            tracing::error!("Invalid graph ID: {}", payload.graph_id);
            return Ok(ResponseJson(GetGraphStatsResponse {
                stats: HashMap::new(),
                success: false,
                error: Some(format!("Invalid graph ID: {}", payload.graph_id)),
            }));
        }
    };

    let graph_read = graph
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match graph_read.stats() {
        Ok(stats) => {
            let mut stats_map = HashMap::new();
            stats_map.insert(
                "total_nodes".to_string(),
                serde_json::Value::Number(stats.total_nodes.into()),
            );
            stats_map.insert(
                "total_edges".to_string(),
                serde_json::Value::Number(stats.total_edges.into()),
            );
            stats_map.insert(
                "storage_nodes".to_string(),
                serde_json::Value::Number(stats.storage_nodes.into()),
            );
            stats_map.insert(
                "storage_edges".to_string(),
                serde_json::Value::Number(stats.storage_edges.into()),
            );
            stats_map.insert(
                "cached_nodes".to_string(),
                serde_json::Value::Number(stats.cached_nodes.into()),
            );
            stats_map.insert(
                "cached_edges".to_string(),
                serde_json::Value::Number(stats.cached_edges.into()),
            );
            stats_map.insert(
                "avg_degree".to_string(),
                serde_json::Value::Number(serde_json::Number::from_f64(stats.avg_degree).unwrap()),
            );
            stats_map.insert(
                "max_degree".to_string(),
                serde_json::Value::Number(stats.max_degree.into()),
            );
            stats_map.insert(
                "min_degree".to_string(),
                serde_json::Value::Number(stats.min_degree.into()),
            );
            stats_map.insert(
                "graph_density".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(stats.graph_density).unwrap(),
                ),
            );
            stats_map.insert(
                "connected_components".to_string(),
                serde_json::Value::Number(stats.connected_components.into()),
            );
            stats_map.insert(
                "avg_clustering_coefficient".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(stats.avg_clustering_coefficient).unwrap(),
                ),
            );
            stats_map.insert(
                "avg_shortest_path_length".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(stats.avg_shortest_path_length).unwrap(),
                ),
            );
            stats_map.insert(
                "diameter".to_string(),
                serde_json::Value::Number(stats.diameter.into()),
            );
            stats_map.insert(
                "isolated_nodes".to_string(),
                serde_json::Value::Number(stats.isolated_nodes.into()),
            );
            stats_map.insert(
                "leaf_nodes".to_string(),
                serde_json::Value::Number(stats.leaf_nodes.into()),
            );
            stats_map.insert(
                "self_loops".to_string(),
                serde_json::Value::Number(stats.self_loops.into()),
            );
            stats_map.insert(
                "bidirectional_edges".to_string(),
                serde_json::Value::Number(stats.bidirectional_edges.into()),
            );

            tracing::info!("Graph stats retrieved for graph {}", payload.graph_id);

            Ok(ResponseJson(GetGraphStatsResponse {
                stats: stats_map,
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to get graph stats: {}", e);
            Ok(ResponseJson(GetGraphStatsResponse {
                stats: HashMap::new(),
                success: false,
                error: Some(format!("Failed to get graph stats: {}", e)),
            }))
        }
    }
}

/// Advanced graph comparison request
#[derive(Debug, Deserialize)]
pub struct AdvancedCompareRequest {
    /// Comparison options
    #[serde(default)]
    pub options: ComparisonOptions,
    /// Whether to include detailed analysis
    pub include_detailed_analysis: bool,
    /// Whether to generate comparison report
    pub generate_report: bool,
}

/// Advanced graph comparison response
#[derive(Debug, Serialize)]
pub struct AdvancedCompareResponse {
    /// The graph diff result
    pub diff: GraphDiff,
    /// Detailed analysis results
    pub detailed_analysis: Option<DetailedAnalysis>,
    /// Comparison report
    pub report: Option<String>,
    /// Success status
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Detailed analysis results
#[derive(Debug, Serialize)]
pub struct DetailedAnalysis {
    /// Node similarity matrix
    pub node_similarities: Vec<NodeSimilarity>,
    /// Edge similarity matrix
    pub edge_similarities: Vec<EdgeSimilarity>,
    /// Graph isomorphism score
    pub isomorphism_score: f64,
    /// Structural changes summary
    pub structural_changes: StructuralChangesSummary,
}

/// Node similarity information
#[derive(Debug, Serialize)]
pub struct NodeSimilarity {
    /// Node ID from original graph
    pub original_id: NodeId,
    /// Node ID from modified graph
    pub modified_id: NodeId,
    /// Similarity score (0.0 to 1.0)
    pub similarity: f64,
    /// Similarity type (exact, fuzzy, none)
    pub similarity_type: String,
}

/// Edge similarity information
#[derive(Debug, Serialize)]
pub struct EdgeSimilarity {
    /// Edge ID from original graph
    pub original_id: EdgeId,
    /// Edge ID from modified graph
    pub modified_id: EdgeId,
    /// Similarity score (0.0 to 1.0)
    pub similarity: f64,
    /// Similarity type (exact, fuzzy, none)
    pub similarity_type: String,
}

/// Structural changes summary
#[derive(Debug, Serialize)]
pub struct StructuralChangesSummary {
    /// Number of connected components added
    pub components_added: usize,
    /// Number of connected components removed
    pub components_removed: usize,
    /// Number of cycles added
    pub cycles_added: usize,
    /// Number of cycles removed
    pub cycles_removed: usize,
    /// Changes in graph diameter
    pub diameter_change: Option<f64>,
}

/// Advanced graph comparison endpoint
pub async fn advanced_compare_graphs(
    State(server): State<Arc<NexusServer>>,
    Json(payload): Json<AdvancedCompareRequest>,
) -> std::result::Result<ResponseJson<AdvancedCompareResponse>, StatusCode> {
    tracing::info!(
        "Advanced graph comparison with options: {:?}",
        payload.options
    );

    let graph_a_read = server
        .graph_a
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let graph_b_read = server
        .graph_b
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match GraphComparator::compare_graphs(&graph_a_read, &graph_b_read, &payload.options) {
        Ok(diff) => {
            let detailed_analysis = if payload.include_detailed_analysis {
                Some(DetailedAnalysis {
                    node_similarities: Vec::new(),
                    edge_similarities: Vec::new(),
                    isomorphism_score: diff.summary.overall_similarity,
                    structural_changes: StructuralChangesSummary {
                        components_added: 0,
                        components_removed: 0,
                        cycles_added: 0,
                        cycles_removed: 0,
                        diameter_change: None,
                    },
                })
            } else {
                None
            };

            let report = if payload.generate_report {
                Some(format!(
                    "Graph Comparison Report\n\
                    ======================\n\
                    Overall Similarity: {:.2}%\n\
                    Structural Similarity: {:.2}%\n\
                    Content Similarity: {:.2}%\n\
                    \n\
                    Changes:\n\
                    - Nodes: +{} -{} ~{}\n\
                    - Edges: +{} -{} ~{}",
                    diff.summary.overall_similarity * 100.0,
                    diff.summary.structural_similarity * 100.0,
                    diff.summary.content_similarity * 100.0,
                    diff.summary.nodes_added,
                    diff.summary.nodes_removed,
                    diff.summary.nodes_modified,
                    diff.summary.edges_added,
                    diff.summary.edges_removed,
                    diff.summary.edges_modified
                ))
            } else {
                None
            };

            tracing::info!(
                "Advanced graph comparison completed: overall similarity {:.4}",
                diff.summary.overall_similarity
            );

            Ok(ResponseJson(AdvancedCompareResponse {
                diff,
                detailed_analysis,
                report,
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            tracing::error!("Advanced graph comparison failed: {}", e);
            Ok(ResponseJson(AdvancedCompareResponse {
                diff: empty_diff(),
                detailed_analysis: None,
                report: None,
                success: false,
                error: Some(e),
            }))
        }
    }
}

/// Health check for comparison service
pub async fn health_check(
    State(_server): State<Arc<NexusServer>>,
) -> std::result::Result<ResponseJson<serde_json::Value>, StatusCode> {
    // phase2d: both graphs are always owned by NexusServer, so the
    // healthy / unhealthy distinction collapses. We keep the response
    // shape for backward compatibility.
    let response = serde_json::json!({
        "status": "healthy",
        "graph_a_available": true,
        "graph_b_available": true,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(ResponseJson(response))
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

    #[tokio::test]
    async fn test_compare_graphs_empty_server_succeeds() {
        let server = build_test_server();
        let req = CompareGraphsRequest {
            options: ComparisonOptions::default(),
        };
        let resp = compare_graphs(State(server), Json(req)).await.expect("ok");
        assert!(resp.0.success, "diff must succeed on empty graphs");
    }

    #[tokio::test]
    async fn test_calculate_similarity_returns_bounded_value() {
        let server = build_test_server();
        let req = CalculateSimilarityRequest {
            options: ComparisonOptions::default(),
        };
        let resp = calculate_similarity(State(server), Json(req))
            .await
            .expect("ok");
        assert!(resp.0.success);
        assert!((0.0..=1.0).contains(&resp.0.similarity));
    }

    #[tokio::test]
    async fn test_get_graph_stats_rejects_invalid_id() {
        let server = build_test_server();
        let req = GetGraphStatsRequest {
            graph_id: "C".to_string(),
        };
        let resp = get_graph_stats(State(server), Json(req)).await.expect("ok");
        assert!(!resp.0.success);
        assert!(resp.0.error.as_ref().unwrap().contains("Invalid graph ID"));
    }

    #[tokio::test]
    async fn test_get_graph_stats_for_graph_a() {
        let server = build_test_server();
        let req = GetGraphStatsRequest {
            graph_id: "A".to_string(),
        };
        let resp = get_graph_stats(State(server), Json(req)).await.expect("ok");
        assert!(resp.0.success);
        assert!(resp.0.stats.contains_key("total_nodes"));
    }

    #[tokio::test]
    async fn test_health_check_reports_healthy() {
        let server = build_test_server();
        let resp = health_check(State(server)).await.expect("ok");
        assert_eq!(resp.0["status"], "healthy");
    }

    #[tokio::test]
    async fn test_two_servers_do_not_share_comparison_state() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        // Arc identities must differ.
        assert!(!Arc::ptr_eq(&server_a.graph_a, &server_b.graph_a));
        assert!(!Arc::ptr_eq(&server_a.graph_b, &server_b.graph_b));
        assert!(!Arc::ptr_eq(
            &server_a.graph_correlation_manager,
            &server_b.graph_correlation_manager
        ));
        assert!(!Arc::ptr_eq(
            &server_a.umicp_handler,
            &server_b.umicp_handler
        ));
    }
}
