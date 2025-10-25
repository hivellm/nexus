//! Graph comparison API endpoints

use axum::extract::Json;
use axum::http::StatusCode;
use axum::response::Json as ResponseJson;
use nexus_core::Graph;
use nexus_core::graph_comparison::{ComparisonOptions, GraphComparator, GraphDiff};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

/// Global graph instances for comparison
/// Note: Using Arc<Mutex<Graph>> instead of Arc<RwLock<Graph>> because Graph contains RefCell
static GRAPH_A: std::sync::OnceLock<Arc<Mutex<Graph>>> = std::sync::OnceLock::new();
static GRAPH_B: std::sync::OnceLock<Arc<Mutex<Graph>>> = std::sync::OnceLock::new();

/// Initialize graph instances for comparison
pub fn init_graphs(graph_a: Arc<Mutex<Graph>>, graph_b: Arc<Mutex<Graph>>) -> anyhow::Result<()> {
    GRAPH_A
        .set(graph_a)
        .map_err(|_| anyhow::anyhow!("Failed to set graph A"))?;
    GRAPH_B
        .set(graph_b)
        .map_err(|_| anyhow::anyhow!("Failed to set graph B"))?;
    Ok(())
}

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

/// Compare two graphs
pub async fn compare_graphs(
    Json(payload): Json<CompareGraphsRequest>,
) -> std::result::Result<ResponseJson<CompareGraphsResponse>, StatusCode> {
    tracing::info!("Comparing graphs with options: {:?}", payload.options);

    let graph_a = GRAPH_A.get().ok_or_else(|| {
        tracing::error!("Graph A not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let graph_b = GRAPH_B.get().ok_or_else(|| {
        tracing::error!("Graph B not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let graph_a_read = graph_a.lock().unwrap();
    let graph_b_read = graph_b.lock().unwrap();

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
                diff: GraphDiff {
                    added_nodes: vec![],
                    removed_nodes: vec![],
                    modified_nodes: vec![],
                    added_edges: vec![],
                    removed_edges: vec![],
                    modified_edges: vec![],
                    summary: nexus_core::graph_comparison::DiffSummary {
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
                    },
                },
                success: false,
                error: Some(e),
            }))
        }
    }
}

/// Calculate similarity between two graphs
pub async fn calculate_similarity(
    Json(payload): Json<CalculateSimilarityRequest>,
) -> std::result::Result<ResponseJson<CalculateSimilarityResponse>, StatusCode> {
    tracing::info!(
        "Calculating graph similarity with options: {:?}",
        payload.options
    );

    let graph_a = GRAPH_A.get().ok_or_else(|| {
        tracing::error!("Graph A not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let graph_b = GRAPH_B.get().ok_or_else(|| {
        tracing::error!("Graph B not initialized");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let graph_a_read = graph_a.lock().unwrap();
    let graph_b_read = graph_b.lock().unwrap();

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
    Json(payload): Json<GetGraphStatsRequest>,
) -> std::result::Result<ResponseJson<GetGraphStatsResponse>, StatusCode> {
    tracing::info!("Getting stats for graph: {}", payload.graph_id);

    let graph = match payload.graph_id.to_uppercase().as_str() {
        "A" => GRAPH_A.get(),
        "B" => GRAPH_B.get(),
        _ => {
            tracing::error!("Invalid graph ID: {}", payload.graph_id);
            return Ok(ResponseJson(GetGraphStatsResponse {
                stats: HashMap::new(),
                success: false,
                error: Some(format!("Invalid graph ID: {}", payload.graph_id)),
            }));
        }
    };

    let graph = graph.ok_or_else(|| {
        tracing::error!("Graph {} not initialized", payload.graph_id);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let graph_read = graph.lock().unwrap();

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

/// Health check for comparison service
pub async fn health_check() -> std::result::Result<ResponseJson<serde_json::Value>, StatusCode> {
    let graph_a_available = GRAPH_A.get().is_some();
    let graph_b_available = GRAPH_B.get().is_some();

    let status = if graph_a_available && graph_b_available {
        "healthy"
    } else {
        "unhealthy"
    };

    let response = serde_json::json!({
        "status": status,
        "graph_a_available": graph_a_available,
        "graph_b_available": graph_b_available,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(ResponseJson(response))
}
