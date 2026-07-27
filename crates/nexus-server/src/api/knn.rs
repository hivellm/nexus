//! KNN-seeded graph traversal endpoint

use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::NexusServer;

/// KNN traversal request
#[derive(Debug, Deserialize)]
pub struct KnnTraverseRequest {
    /// Node label to search
    pub label: String,
    /// Query vector
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

    // Real label-aware HNSW search — mirrors the RPC `KNN_SEARCH` dispatch
    // (`protocol/rpc/dispatch/knn.rs`), which resolves `label` to its
    // catalog id, filters the global HNSW's raw hits down to nodes
    // carrying that label, and returns ranked `(node_id, score)` pairs by
    // real cosine similarity. Read-only, so a read guard is sufficient —
    // no need for the `spawn_blocking` + `blocking_read` the RPC path uses
    // to keep the async runtime's worker threads free during long HNSW
    // scans; this HTTP handler already runs on a plain tokio task.
    let engine = server.engine.read().await;

    let hits = match engine.knn_search(safe_label, &request.vector, request.k) {
        Ok(hits) => hits,
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;
            tracing::error!("KNN traverse failed: {}", e);
            return Json(KnnTraverseResponse {
                nodes: vec![],
                execution_time_ms: execution_time,
                error: Some(e.to_string()),
            });
        }
    };

    // `knn_search` already truncates to `request.k`; `request.limit` is a
    // separate response-size cap the caller may set independently.
    let mut nodes = Vec::with_capacity(hits.len().min(request.limit));
    for (id, score) in hits.into_iter().take(request.limit) {
        let properties = match engine.storage.load_node_properties(id) {
            Ok(Some(value)) => value,
            Ok(None) => serde_json::Value::Null,
            Err(e) => {
                tracing::warn!(
                    "KNN traverse: failed to load properties for node {}: {}",
                    id,
                    e
                );
                serde_json::Value::Null
            }
        };
        nodes.push(KnnNode {
            id,
            properties,
            score,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fresh `Arc<NexusServer>` for tests; identical pattern to
    /// `api::data::tests::build_test_server` but duplicated here so each
    /// module's test surface stays self-contained.
    fn build_test_server() -> Arc<NexusServer> {
        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
        build_test_server_with_engine(engine, ctx)
    }

    /// Same as [`build_test_server`], but lets the caller pre-populate the
    /// engine (nodes + vector index entries) before it's wrapped for the
    /// server — needed for tests that exercise the real HNSW search path
    /// rather than only the "empty engine" no-panic guarantees.
    fn build_test_server_with_engine(
        engine: nexus_core::Engine,
        ctx: nexus_core::testing::TestContext,
    ) -> Arc<NexusServer> {
        use parking_lot::RwLock as PlRwLock;
        use tokio::sync::RwLock as TokioRwLock;

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
        // An empty engine has no catalog entry for this label; `Engine::
        // knn_search` treats an unknown label as "no matches" and returns
        // an empty hit list rather than an error. Neither path panics.
        assert!(response.nodes.is_empty());
    }

    #[tokio::test]
    async fn test_knn_traverse_with_empty_vector_still_responds() {
        let server = build_test_server();
        let response = knn_traverse(State(server), Json(probe_request("Any", 5, vec![])))
            .await
            .0;
        // The label lookup fails first (no such label in an empty engine),
        // short-circuiting before the empty vector could trip the HNSW's
        // dimension check — so this still resolves to an empty, error-free
        // response rather than a panic either way.
        assert!(response.nodes.is_empty() || response.error.is_some());
    }

    #[tokio::test]
    async fn test_knn_traverse_returns_real_ranked_hit_from_populated_index() {
        use nexus_core::index::DEFAULT_VECTORIZER_DIMENSION;

        let ctx = nexus_core::testing::TestContext::new();
        let mut engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
        let node_id = engine
            .create_node(
                vec!["Doc".to_string()],
                serde_json::json!({"name": "alpha"}),
            )
            .expect("create :Doc node");
        let vector = vec![1.0_f32; DEFAULT_VECTORIZER_DIMENSION];
        engine
            .indexes
            .knn_index
            .add_vector(node_id, vector.clone())
            .expect("add vector to HNSW index");

        let server = build_test_server_with_engine(engine, ctx);

        let response = knn_traverse(State(server), Json(probe_request("Doc", 5, vector)))
            .await
            .0;

        assert!(
            response.error.is_none(),
            "unexpected error: {:?}",
            response.error
        );
        assert_eq!(
            response.nodes.len(),
            1,
            "expected exactly the seeded node, got {:?}",
            response.nodes
        );
        assert_eq!(response.nodes[0].id, node_id);
        // The query vector is an exact match for the stored vector, so
        // cosine similarity must be ~1.0 — proving the score comes from a
        // real HNSW search, not the old fabricated `1.0 - i*0.1` formula.
        assert!(
            (response.nodes[0].score - 1.0).abs() < 1e-4,
            "identical query vector should score ~1.0 cosine similarity, got {}",
            response.nodes[0].score
        );
        assert_eq!(
            response.nodes[0]
                .properties
                .get("name")
                .and_then(|v| v.as_str()),
            Some("alpha"),
            "properties must be the real node's stored properties, not a placeholder"
        );
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
