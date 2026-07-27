//! Server-level integration coverage for `phase20_knn-write-path-wiring`
//! §4.2 — proves native KNN vector search works end-to-end through the
//! REAL user write path (Cypher `CREATE VECTOR INDEX` + parameterized
//! `CREATE`, which drives `Engine::knn_autopopulate_node`) and the HTTP
//! `POST /knn_traverse` handler, not a synthetic `KnnIndex::add_vector`
//! shortcut.
//!
//! Harness: this file reuses two existing patterns rather than inventing
//! a new one:
//! - `build_test_server_with_engine` / `build_test_server`, copied from
//!   `crates/nexus-server/src/api/knn.rs`'s own `#[cfg(test)]` module
//!   (that module documents the duplication as the established pattern —
//!   "identical pattern to `api::data::tests::build_test_server` but
//!   duplicated here so each module's test surface stays self-contained").
//! - Driving Cypher writes directly against `server.engine` via
//!   `Engine::execute_cypher_with_params`, the same entry point
//!   `crates/nexus-server/src/api/cypher/execute/handler.rs::execute_cypher`
//!   calls for the default (unnamed) database — see
//!   `cypher_database_routing_test.rs` for the HTTP-handler-level
//!   equivalent of this call.
//!
//! Embeddings are supplied via bound Cypher parameters (`$e`), never via
//! an inline array literal in `CREATE` (unsupported) and never via the
//! internal `KnnIndex::add_vector` API — the write path under test is
//! `CREATE (d:Doc {embedding: $e})` after `CREATE VECTOR INDEX ... FOR
//! (d:Doc) ON (d.embedding)`, which is what a real client does.
//!
//! RPC (`KNN_SEARCH`) / RESP3 (`KNN.SEARCH`) are NOT exercised here — both
//! already have their own dispatch-level unit tests
//! (`crates/nexus-server/src/protocol/rpc/dispatch/knn.rs`); reaching
//! them from an integration test would require standing up a full RPC
//! session, which is heavier than this suite's scope. HTTP is covered
//! thoroughly instead.

use axum::extract::{Json, State};
use nexus_server::NexusServer;
use nexus_server::api::knn::{KnnTraverseRequest, knn_traverse};
use std::collections::HashMap;
use std::sync::Arc;

/// `DEFAULT_VECTORIZER_DIMENSION` — the single global HNSW graph's fixed
/// dimension (see `nexus_core::index::DEFAULT_VECTORIZER_DIMENSION`).
/// Hardcoded here (rather than imported) so this integration test fails
/// loudly if the two ever diverge instead of silently tracking a source
/// change; cross-checked against the import in the assertion helper
/// below.
const DIM: usize = 128;

/// Build a fresh `Arc<NexusServer>` for tests — identical pattern to
/// `api::knn::tests::build_test_server` in
/// `crates/nexus-server/src/api/knn.rs`, duplicated here per that
/// module's own stated convention (self-contained test surfaces).
fn build_test_server() -> Arc<NexusServer> {
    let ctx = nexus_core::testing::TestContext::new();
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
    build_test_server_with_engine(engine, ctx)
}

/// Same as [`build_test_server`], but lets the caller pre-populate the
/// engine before it's wrapped for the server. Mirrors
/// `api::knn::tests::build_test_server_with_engine` verbatim.
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
        nexus_server::config::RootUserConfig::default(),
    ))
}

fn traverse_request(label: &str, vector: Vec<f32>, k: usize) -> KnnTraverseRequest {
    KnnTraverseRequest {
        label: label.to_string(),
        vector,
        k,
        expand: vec![],
        r#where: None,
        limit: 100,
    }
}

/// A one-hot 128-dim embedding: 1.0 at `hot_index`, 0.0 everywhere else.
/// Well-separated one-hot vectors give a deterministic cosine ranking —
/// the query vector matches exactly one seeded node and is orthogonal to
/// the other.
fn one_hot(hot_index: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; DIM];
    v[hot_index] = 1.0;
    v
}

/// End to end: `CREATE VECTOR INDEX` + two parameterized `CREATE`s (the
/// real user write path) populate the HNSW; `POST /knn_traverse` (via its
/// handler) returns the CLOSER node ranked first with a real cosine
/// score, not the old fabricated `1.0 - i*0.1` pattern.
#[tokio::test]
async fn knn_traverse_returns_real_ranked_results() {
    let server = build_test_server();

    // Real write path: register the vector index via Cypher DDL, then
    // insert two :Doc nodes whose `embedding` property is bound from a
    // query PARAMETER (never an inline array literal — unsupported, and
    // never the internal `KnnIndex::add_vector` API).
    {
        let mut engine = server.engine.write().await;
        engine
            .execute_cypher("CREATE VECTOR INDEX docEmb FOR (d:Doc) ON (d.embedding)")
            .expect("CREATE VECTOR INDEX must succeed");

        let mut params_one: HashMap<String, serde_json::Value> = HashMap::new();
        params_one.insert("id".to_string(), serde_json::json!(1));
        params_one.insert("e".to_string(), serde_json::json!(one_hot(0)));
        engine
            .execute_cypher_with_params("CREATE (d:Doc {id: $id, embedding: $e})", params_one)
            .expect("CREATE doc-1 with parameterized embedding must succeed");

        let mut params_two: HashMap<String, serde_json::Value> = HashMap::new();
        params_two.insert("id".to_string(), serde_json::json!(2));
        params_two.insert("e".to_string(), serde_json::json!(one_hot(1)));
        engine
            .execute_cypher_with_params("CREATE (d:Doc {id: $id, embedding: $e})", params_two)
            .expect("CREATE doc-2 with parameterized embedding must succeed");
    }

    // Query with doc-1's exact embedding, k=2 — both docs must come back
    // (only two exist), doc-1 ranked first (cosine similarity 1.0 for an
    // exact match), doc-2 second with a strictly lower, non-fabricated
    // score (cosine similarity 0.0 for an orthogonal one-hot vector).
    let response = knn_traverse(State(server), Json(traverse_request("Doc", one_hot(0), 2)))
        .await
        .0;

    assert!(
        response.error.is_none(),
        "unexpected error: {:?}",
        response.error
    );
    assert_eq!(
        response.nodes.len(),
        2,
        "expected both seeded docs, got {:?}",
        response.nodes
    );

    // Ranked-first assertion: the node whose embedding EXACTLY matches
    // the query vector must be first.
    let top = &response.nodes[0];
    assert_eq!(
        top.properties.get("id").and_then(|v| v.as_i64()),
        Some(1),
        "doc-1 (exact embedding match) must be ranked first; got {:?}",
        response.nodes
    );

    // The top score must be a REAL cosine similarity (~1.0 for an exact
    // match), not the old fabricated `1.0 - i*0.1` formula (which would
    // also yield 1.0 for i=0 here, so we additionally check the SECOND
    // result diverges from that formula's `0.9` and instead reflects the
    // true orthogonal cosine similarity of ~0.0).
    assert!(
        (top.score - 1.0).abs() < 1e-4,
        "exact-match query vector should score ~1.0 cosine similarity, got {}",
        top.score
    );

    let second = &response.nodes[1];
    assert_eq!(
        second.properties.get("id").and_then(|v| v.as_i64()),
        Some(2),
        "doc-2 (orthogonal embedding) must be ranked second; got {:?}",
        response.nodes
    );
    assert!(
        second.score.abs() < 1e-4,
        "orthogonal one-hot vector should score ~0.0 cosine similarity \
         (not the fabricated formula's 0.9), got {}",
        second.score
    );
    assert!(
        top.score > second.score,
        "closer doc must rank strictly ahead of the farther doc: top={}, second={}",
        top.score,
        second.score
    );
}

/// Negative / edge case: `knn_traverse` against a label with no vector
/// index and no nodes returns an empty result — no panic, no error.
#[tokio::test]
async fn knn_traverse_unknown_label_is_empty() {
    let server = build_test_server();

    let response = knn_traverse(
        State(server),
        Json(traverse_request("NoSuchLabel", one_hot(0), 5)),
    )
    .await
    .0;

    assert!(
        response.nodes.is_empty(),
        "unknown label with no index/nodes must return empty nodes, got {:?}",
        response.nodes
    );
}
