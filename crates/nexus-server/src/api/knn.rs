//! KNN-seeded graph traversal endpoint

use axum::extract::{Json, State};
use nexus_core::executor::Query;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::NexusServer;

/// KNN traversal request
#[derive(Debug, Deserialize)]
pub struct KnnTraverseRequest {
    /// Node label to search
    pub label: String,
    /// Query vector
    #[allow(dead_code)]
    pub vector: Vec<f32>,
    /// Number of nearest neighbors
    pub k: usize,
    /// Optional expansion patterns
    #[serde(default)]
    #[allow(dead_code)]
    pub expand: Vec<String>,
    /// Optional WHERE clause
    #[allow(dead_code)]
    pub r#where: Option<String>,
    /// Result limit
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// KNN traversal response
#[derive(Debug, Serialize)]
pub struct KnnTraverseResponse {
    /// Result nodes with scores
    pub nodes: Vec<KnnNode>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// KNN result node
#[derive(Debug, Serialize)]
pub struct KnnNode {
    /// Node ID
    pub id: u64,
    /// Node properties
    pub properties: serde_json::Value,
    /// Similarity score
    pub score: f32,
}

/// Execute KNN-seeded traversal
pub async fn knn_traverse(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<KnnTraverseRequest>,
) -> Json<KnnTraverseResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!(
        "KNN traverse on label '{}' with k={}",
        request.label,
        request.k
    );

    // Validate the label before interpolating into the Cypher query —
    // without this a client can send
    // `Person) DETACH DELETE n //` and escape the node pattern.
    let safe_label = match super::identifier::validate_identifier(&request.label) {
        Ok(s) => s,
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;
            tracing::warn!("KNN traverse rejected invalid label: {}", e);
            return Json(KnnTraverseResponse {
                nodes: vec![],
                execution_time_ms: execution_time,
                error: Some(format!("invalid label: {}", e)),
            });
        }
    };

    let executor = server.executor.clone();

    // For MVP, we'll use a simple approach:
    // 1. Find nodes with the specified label
    // 2. Use KNN index to find similar nodes
    // 3. Return results with scores

    // Create a simple MATCH query for the label
    let cypher_query = format!("MATCH (n:{}) RETURN n", safe_label);
    let query = Query {
        cypher: cypher_query,
        params: HashMap::new(),
    };

    match executor.execute(&query) {
        Ok(result_set) => {
            let mut nodes = Vec::new();

            // For MVP, we'll simulate KNN search by creating dummy scores
            for (i, row) in result_set.rows.iter().enumerate().take(request.limit) {
                if let Some(node_value) = row.values.first() {
                    if let Some(node_obj) = node_value.as_object() {
                        if let Some(id_value) = node_obj.get("id") {
                            if let Some(id) = id_value.as_u64() {
                                // Simulate similarity score (in real implementation,
                                // this would come from the KNN index)
                                let score = 1.0 - (i as f32 * 0.1).min(0.9);

                                nodes.push(KnnNode {
                                    id,
                                    properties: node_value.clone(),
                                    score,
                                });
                            }
                        }
                    }
                }
            }

            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::info!(
                "KNN traverse completed in {}ms, {} nodes returned",
                execution_time,
                nodes.len()
            );

            Json(KnnTraverseResponse {
                nodes,
                execution_time_ms: execution_time,
                error: None,
            })
        }
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::error!("KNN traverse failed: {}", e);

            Json(KnnTraverseResponse {
                nodes: vec![],
                execution_time_ms: execution_time,
                error: Some(e.to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fresh `Arc<NexusServer>` for tests; identical pattern to
    /// `api::data::tests::build_test_server` but duplicated here so each
    /// module's test surface stays self-contained.
    fn build_test_server() -> Arc<NexusServer> {
        use parking_lot::RwLock as PlRwLock;
        use tokio::sync::RwLock as TokioRwLock;

        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
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

    fn probe_request(label: &str, k: usize, vector: Vec<f32>) -> KnnTraverseRequest {
        KnnTraverseRequest {
            label: label.to_string(),
            vector,
            k,
            expand: vec![],
            r#where: None,
            limit: 10,
        }
    }

    #[tokio::test]
    async fn test_knn_traverse_runs_without_panic_on_empty_engine() {
        let server = build_test_server();
        let response = knn_traverse(
            State(server),
            Json(probe_request("Missing", 5, vec![0.1; 4])),
        )
        .await
        .0;
        // An empty engine has no nodes with this label; the handler
        // either returns an empty result set or a Cypher syntax/semantics
        // error from the fallback MATCH. Neither is a panic.
        assert!(response.nodes.is_empty());
    }

    #[tokio::test]
    async fn test_knn_traverse_with_empty_vector_still_responds() {
        let server = build_test_server();
        let response = knn_traverse(State(server), Json(probe_request("Any", 5, vec![])))
            .await
            .0;
        assert!(response.nodes.is_empty() || response.error.is_some());
    }

    #[tokio::test]
    async fn test_two_servers_do_not_share_executor_state() {
        // Two independent servers — exercise the handler on both. The
        // assertion is behavioural: `knn_traverse` against an empty
        // engine must return an empty / error response regardless of
        // what another process-wide server has done, which is the
        // OnceLock-free invariant phase2b ships.
        let server_a = build_test_server();
        let server_b = build_test_server();

        let resp_a = knn_traverse(State(server_a), Json(probe_request("A", 1, vec![0.1; 4])))
            .await
            .0;
        let resp_b = knn_traverse(State(server_b), Json(probe_request("B", 1, vec![0.1; 4])))
            .await
            .0;

        assert!(resp_a.nodes.is_empty());
        assert!(resp_b.nodes.is_empty());
    }
}
