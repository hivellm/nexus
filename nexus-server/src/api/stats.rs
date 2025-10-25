//! Database statistics endpoints

use axum::extract::Json;
use nexus_core::{
    catalog::Catalog,
    index::{KnnIndex, LabelIndex},
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global instances
static CATALOG: std::sync::OnceLock<Arc<RwLock<Catalog>>> = std::sync::OnceLock::new();
static LABEL_INDEX: std::sync::OnceLock<Arc<RwLock<LabelIndex>>> = std::sync::OnceLock::new();
static KNN_INDEX: std::sync::OnceLock<Arc<RwLock<KnnIndex>>> = std::sync::OnceLock::new();

/// Initialize the instances
pub fn init_instances(
    catalog: Arc<RwLock<Catalog>>,
    label_index: Arc<RwLock<LabelIndex>>,
    knn_index: Arc<RwLock<KnnIndex>>,
) -> anyhow::Result<()> {
    CATALOG
        .set(catalog)
        .map_err(|_| anyhow::anyhow!("Failed to set catalog"))?;
    LABEL_INDEX
        .set(label_index)
        .map_err(|_| anyhow::anyhow!("Failed to set label index"))?;
    KNN_INDEX
        .set(knn_index)
        .map_err(|_| anyhow::anyhow!("Failed to set knn index"))?;
    Ok(())
}

/// Database statistics response
#[derive(Debug, Serialize)]
pub struct DatabaseStatsResponse {
    /// Catalog statistics
    pub catalog: CatalogStats,
    /// Label index statistics
    pub label_index: LabelIndexStats,
    /// KNN index statistics
    pub knn_index: KnnIndexStats,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Catalog statistics
#[derive(Debug, Serialize)]
pub struct CatalogStats {
    /// Number of labels
    pub label_count: usize,
    /// Number of relationship types
    pub rel_type_count: usize,
    /// Number of nodes
    pub node_count: usize,
    /// Number of relationships
    pub rel_count: usize,
}

/// Label index statistics
#[derive(Debug, Serialize)]
pub struct LabelIndexStats {
    /// Number of indexed labels
    pub indexed_labels: usize,
    /// Total nodes indexed
    pub total_nodes: usize,
}

/// KNN index statistics
#[derive(Debug, Serialize)]
pub struct KnnIndexStats {
    /// Number of vectors
    pub total_vectors: usize,
    /// Vector dimension
    pub dimension: usize,
    /// Average search time in microseconds
    pub avg_search_time_us: f64,
}

/// Get database statistics
pub async fn get_stats() -> Json<DatabaseStatsResponse> {
    tracing::info!("Getting database statistics");

    // Get catalog stats
    let catalog_stats = match CATALOG.get() {
        Some(catalog) => {
            let catalog = catalog.read().await;
            match catalog.get_statistics() {
                Ok(stats) => CatalogStats {
                    label_count: stats.label_count as usize,
                    rel_type_count: stats.type_count as usize,
                    node_count: stats.node_counts.values().sum::<u64>() as usize,
                    rel_count: stats.rel_counts.values().sum::<u64>() as usize,
                },
                Err(e) => {
                    tracing::error!("Failed to get catalog stats: {}", e);
                    return Json(DatabaseStatsResponse {
                        catalog: CatalogStats {
                            label_count: 0,
                            rel_type_count: 0,
                            node_count: 0,
                            rel_count: 0,
                        },
                        label_index: LabelIndexStats {
                            indexed_labels: 0,
                            total_nodes: 0,
                        },
                        knn_index: KnnIndexStats {
                            total_vectors: 0,
                            dimension: 0,
                            avg_search_time_us: 0.0,
                        },
                        error: Some(format!("Failed to get catalog stats: {}", e)),
                    });
                }
            }
        }
        None => {
            tracing::error!("Catalog not initialized");
            return Json(DatabaseStatsResponse {
                catalog: CatalogStats {
                    label_count: 0,
                    rel_type_count: 0,
                    node_count: 0,
                    rel_count: 0,
                },
                label_index: LabelIndexStats {
                    indexed_labels: 0,
                    total_nodes: 0,
                },
                knn_index: KnnIndexStats {
                    total_vectors: 0,
                    dimension: 0,
                    avg_search_time_us: 0.0,
                },
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    // Get label index stats
    let label_index_stats = match LABEL_INDEX.get() {
        Some(label_index) => {
            let label_index = label_index.read().await;
            let stats = label_index.get_stats();
            LabelIndexStats {
                indexed_labels: stats.label_count as usize,
                total_nodes: stats.total_nodes as usize,
            }
        }
        None => {
            tracing::error!("Label index not initialized");
            LabelIndexStats {
                indexed_labels: 0,
                total_nodes: 0,
            }
        }
    };

    // Get KNN index stats
    let knn_index_stats = match KNN_INDEX.get() {
        Some(knn_index) => {
            let knn_index = knn_index.read().await;
            let stats = knn_index.get_stats();
            KnnIndexStats {
                total_vectors: stats.total_vectors as usize,
                dimension: stats.dimension,
                avg_search_time_us: stats.avg_search_time_us,
            }
        }
        None => {
            tracing::error!("KNN index not initialized");
            KnnIndexStats {
                total_vectors: 0,
                dimension: 0,
                avg_search_time_us: 0.0,
            }
        }
    };

    tracing::info!(
        "Database stats - Labels: {}, RelTypes: {}, Nodes: {}, Rels: {}, Vectors: {}",
        catalog_stats.label_count,
        catalog_stats.rel_type_count,
        catalog_stats.node_count,
        catalog_stats.rel_count,
        knn_index_stats.total_vectors
    );

    Json(DatabaseStatsResponse {
        catalog: catalog_stats,
        label_index: label_index_stats,
        knn_index: knn_index_stats,
        error: None,
    })
}
