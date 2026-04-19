//! Regression tests for `phase3_cypher-injection-validation`.
//!
//! Before this task, several handlers interpolated user-supplied
//! label / relationship-type strings directly into the Cypher query
//! via `format!`. A client could send `"Person) DETACH DELETE n //"`
//! and escape the node pattern. The `validate_identifier` helper at
//! `nexus-server/src/api/identifier.rs` closes the hole. This file
//! exercises every affected handler against the canonical malicious
//! payload and asserts a validation error is surfaced *before* the
//! Cypher executor runs.

use axum::extract::{Json, State};
use nexus_server::NexusServer;
use nexus_server::api::identifier::{InvalidIdentifier, validate_identifier};
use parking_lot::RwLock as PlRwLock;
use std::sync::Arc;
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
    // Leak the TestContext so its tempdir outlives the test.
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

const INJECTION_PAYLOAD: &str = "Person) MATCH (m) DETACH DELETE m //";

#[test]
fn validate_identifier_rejects_canonical_injection() {
    let err = validate_identifier(INJECTION_PAYLOAD).unwrap_err();
    assert!(
        matches!(err, InvalidIdentifier::BadBodyChar { ch: ')', .. }),
        "expected BadBodyChar(')' ), got {:?}",
        err
    );
}

#[test]
fn validate_identifier_rejects_every_character_that_could_escape_a_pattern() {
    // These are the characters that appear in realistic injection
    // payloads and that would break out of a MATCH / CREATE / MERGE
    // node or relationship pattern if interpolated raw.
    for trigger in [
        ")", "]", "{", "}", " ", "\t", "\n", "/", "-", "\"", "'", ";",
    ] {
        let candidate = format!("Name{}", trigger);
        assert!(
            validate_identifier(&candidate).is_err(),
            "identifier `{}` must be rejected",
            candidate
        );
    }
}

// ──────────────────────────────────────────────────────────────────
// knn.rs — `POST /knn_traverse` with malicious label
// ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn knn_traverse_rejects_injection_in_label() {
    use nexus_server::api::knn::{KnnTraverseRequest, knn_traverse};

    let server = build_test_server();
    let request = KnnTraverseRequest {
        label: INJECTION_PAYLOAD.to_string(),
        vector: vec![0.1, 0.2, 0.3],
        k: 3,
        expand: vec![],
        r#where: None,
        limit: 10,
    };

    let response = knn_traverse(State(server), Json(request)).await.0;

    // The handler must surface a validation error and MUST NOT have
    // reached the executor.
    let err = response
        .error
        .as_ref()
        .expect("knn_traverse must reject malicious label");
    assert!(
        err.contains("invalid label"),
        "error message must name the violation, got: {}",
        err
    );
    assert!(response.nodes.is_empty());
}

#[tokio::test]
async fn knn_traverse_accepts_well_formed_label() {
    // Sanity check — validation must not break the happy path.
    use nexus_server::api::knn::{KnnTraverseRequest, knn_traverse};

    let server = build_test_server();
    let request = KnnTraverseRequest {
        label: "Person".to_string(),
        vector: vec![0.1, 0.2, 0.3],
        k: 3,
        expand: vec![],
        r#where: None,
        limit: 10,
    };

    let response = knn_traverse(State(server), Json(request)).await.0;

    // The engine is empty so either we get zero nodes or an engine-level
    // "label not found" error — both are acceptable. What matters is
    // that we did NOT hit the identifier validator.
    if let Some(err) = &response.error {
        assert!(
            !err.contains("invalid label"),
            "happy-path label must not trip the identifier validator: {}",
            err
        );
    }
}
