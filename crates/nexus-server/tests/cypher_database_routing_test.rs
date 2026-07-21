//! Regression suite for `phase0_fix-cypher-database-routing`.
//!
//! Multi-database isolation was non-functional over `POST /cypher`:
//! `CypherRequest.database` was parsed and never read, so every query hit the
//! single `server.engine` regardless of the requested database — including
//! queries naming a database that does not exist. These tests drive the
//! `execute_cypher` handler directly (auth disabled) and assert real isolation,
//! a typed error for an unknown database, and that the default path (no
//! `database` field) is unchanged.

use axum::extract::{Json, State};
use nexus_server::api::cypher::{CypherRequest, execute_cypher};
use nexus_server::{NexusServer, config::RootUserConfig};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Build a NexusServer (auth disabled) whose engine and database manager share
/// one data dir, mirroring `main.rs`'s production wiring.
fn build_server() -> (Arc<NexusServer>, nexus_core::testing::TestContext) {
    let ctx = nexus_core::testing::TestContext::new();
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
    let engine_arc = Arc::new(TokioRwLock::new(engine));
    let executor_arc = Arc::new(nexus_core::executor::Executor::default());
    let database_manager = nexus_core::database::DatabaseManager::new(ctx.path().into()).unwrap();
    let database_manager_arc = Arc::new(RwLock::new(database_manager));
    let rbac_arc = Arc::new(TokioRwLock::new(
        nexus_core::auth::RoleBasedAccessControl::new(),
    ));
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(
        nexus_core::auth::AuthConfig::default(),
    ));
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
        nexus_core::auth::JwtConfig::default(),
    ));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: std::path::PathBuf::from("./logs"),
            retention_days: 30,
            compress_logs: false,
        })
        .unwrap(),
    );
    let server = Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        RootUserConfig::default(),
    ));
    (server, ctx)
}

/// Run a Cypher query through the handler against an optional database.
async fn run(
    server: &Arc<NexusServer>,
    query: &str,
    database: Option<&str>,
) -> nexus_server::api::cypher::CypherResponse {
    let req = CypherRequest {
        query: query.to_string(),
        params: Default::default(),
        database: database.map(|s| s.to_string()),
    };
    execute_cypher(State(server.clone()), None, Json(req))
        .await
        .0
}

/// The single scalar of a `RETURN count(...)` response, or `None` on error/shape
/// mismatch.
fn count_of(resp: &nexus_server::api::cypher::CypherResponse) -> Option<u64> {
    resp.rows.first().and_then(|row| match row {
        serde_json::Value::Array(cells) => cells.first().and_then(|v| v.as_u64()),
        _ => None,
    })
}

/// A node created under database `alpha` must NOT be visible from database
/// `beta`, and vice versa. Before the fix both resolved to the same
/// `server.engine`, so `beta` saw `alpha`'s node.
#[tokio::test]
async fn create_under_alpha_is_invisible_from_beta() {
    let (server, _ctx) = build_server();
    run(&server, "CREATE DATABASE alpha", None).await;
    run(&server, "CREATE DATABASE beta", None).await;

    let created = run(&server, "CREATE (:Canary {m: 'a'})", Some("alpha")).await;
    assert!(
        created.error.is_none(),
        "create in alpha failed: {created:?}"
    );

    let from_alpha = run(&server, "MATCH (n:Canary) RETURN count(n)", Some("alpha")).await;
    assert_eq!(
        count_of(&from_alpha),
        Some(1),
        "alpha must see its own node: {from_alpha:?}"
    );

    let from_beta = run(&server, "MATCH (n:Canary) RETURN count(n)", Some("beta")).await;
    assert_eq!(
        count_of(&from_beta),
        Some(0),
        "beta must NOT see alpha's node — databases must be isolated: {from_beta:?}"
    );
}

/// A query naming a database that was never created must return an error, not
/// silently serve the default store.
#[tokio::test]
async fn query_against_unknown_database_is_rejected() {
    let (server, _ctx) = build_server();
    run(&server, "CREATE (:Canary {m: 'default'})", None).await;

    let resp = run(
        &server,
        "MATCH (n:Canary) RETURN count(n)",
        Some("nosuchdb_xyz"),
    )
    .await;
    assert!(
        resp.error.is_some(),
        "a query naming a nonexistent database must error, not serve the default store: {resp:?}"
    );
}

/// A query with no `database` field must keep resolving to the default engine,
/// so existing single-database clients are unaffected.
#[tokio::test]
async fn default_path_unaffected_when_no_database_field() {
    let (server, _ctx) = build_server();
    let created = run(&server, "CREATE (:Canary {m: 'default'})", None).await;
    assert!(
        created.error.is_none(),
        "default create failed: {created:?}"
    );

    let read = run(&server, "MATCH (n:Canary) RETURN count(n)", None).await;
    assert_eq!(
        count_of(&read),
        Some(1),
        "the default (no-database) path must see its own writes: {read:?}"
    );
}
