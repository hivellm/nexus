//! Database statistics endpoints

use axum::extract::{Json, State};
use serde::Serialize;
use std::sync::Arc;

use crate::NexusServer;

/// Database statistics response
#[derive(Debug, Serialize)]
pub struct DatabaseStatsResponse {
    /// Catalog statistics
    pub catalog: CatalogStats,
    /// Label index statistics
    pub label_index: LabelIndexStats,
    /// KNN index statistics
    pub knn_index: KnnIndexStats,
    /// SIMD kernel tier per op (runtime-selected). Useful for ops to
    /// confirm which vectorised path the running binary picked. Omitted
    /// on serialisation when `None` for forward compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simd: Option<SimdStats>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Per-op SIMD kernel tier names. Values are static strings from the
/// `simd::*` dispatch modules (e.g. `"avx512"`, `"avx2"`, `"neon"`,
/// `"scalar"`, `"scalar (NEXUS_SIMD_DISABLE)"`).
#[derive(Debug, Serialize)]
pub struct SimdStats {
    /// CpuFeatures probe summary — short name of the highest-tier path
    /// the host supports.
    pub preferred_tier: &'static str,
    /// Per-op kernel tiers as `{ op_name: tier_name }`.
    pub kernels: std::collections::BTreeMap<&'static str, &'static str>,
}

fn collect_simd_stats() -> SimdStats {
    use nexus_core::simd;
    let mut kernels = std::collections::BTreeMap::new();
    for (name, tier) in simd::distance::kernel_tiers() {
        kernels.insert(name, tier);
    }
    for (name, tier) in simd::bitmap::kernel_tiers() {
        kernels.insert(name, tier);
    }
    for (name, tier) in simd::reduce::kernel_tiers() {
        kernels.insert(name, tier);
    }
    for (name, tier) in simd::compare::kernel_tiers() {
        kernels.insert(name, tier);
    }
    // RLE has a single kernel pointer; expose it under the same namespace
    // so operators see it alongside the others.
    kernels.insert("find_run_length_u64", simd::rle::kernel_tier());
    SimdStats {
        preferred_tier: simd::cpu().preferred_tier(),
        kernels,
    }
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

/// Get database statistics.
pub async fn get_stats(State(server): State<Arc<NexusServer>>) -> Json<DatabaseStatsResponse> {
    tracing::info!("Getting database statistics");

    let mut engine = server.engine.write().await;
    match engine.stats() {
        Ok(engine_stats) => {
            tracing::info!(
                "Database stats - Labels: {}, RelTypes: {}, Nodes: {}, Rels: {}",
                engine_stats.labels,
                engine_stats.rel_types,
                engine_stats.nodes,
                engine_stats.relationships
            );

            Json(DatabaseStatsResponse {
                catalog: CatalogStats {
                    label_count: engine_stats.labels as usize,
                    rel_type_count: engine_stats.rel_types as usize,
                    node_count: engine_stats.nodes as usize,
                    rel_count: engine_stats.relationships as usize,
                },
                label_index: LabelIndexStats {
                    indexed_labels: engine_stats.labels as usize,
                    total_nodes: engine_stats.nodes as usize,
                },
                knn_index: KnnIndexStats {
                    total_vectors: 0, // Engine stats doesn't expose this yet
                    dimension: 128,
                    avg_search_time_us: 0.0,
                },
                simd: Some(collect_simd_stats()),
                error: None,
            })
        }
        Err(e) => {
            tracing::error!("Failed to get engine stats: {}", e);
            Json(DatabaseStatsResponse {
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
                simd: Some(collect_simd_stats()),
                error: Some(format!("Failed to get engine stats: {e}")),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_server() -> Arc<NexusServer> {
        use parking_lot::RwLock as PlRwLock;
        use tokio::sync::RwLock as TokioRwLock;

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
    async fn test_get_stats_returns_engine_counters() {
        let server = build_test_server();
        let response = get_stats(State(server)).await.0;
        assert!(
            response.error.is_none(),
            "stats failed: {:?}",
            response.error
        );
        assert!(response.simd.is_some());
    }

    #[tokio::test]
    async fn test_two_servers_do_not_share_stats_state() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        // Create a node on server A via the Engine directly.
        {
            let mut engine = server_a.engine.write().await;
            engine
                .create_node(vec!["A".to_string()], serde_json::json!({}))
                .unwrap();
        }

        let stats_a = get_stats(State(Arc::clone(&server_a))).await.0;
        let stats_b = get_stats(State(Arc::clone(&server_b))).await.0;

        assert!(stats_a.error.is_none());
        assert!(stats_b.error.is_none());
        assert!(
            stats_a.catalog.node_count >= 1,
            "server A should see its created node"
        );
        assert_eq!(
            stats_b.catalog.node_count, 0,
            "server B should not see nodes created on server A"
        );
    }
}
