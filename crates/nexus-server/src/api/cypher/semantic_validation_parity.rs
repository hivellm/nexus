//! Transport parity for the semantic-analysis pass: a semantically invalid
//! query must be rejected the same way over HTTP `/cypher` and over the RPC
//! `CYPHER` command.
//!
//! It used not to be. `semantic_validation::validate` had a single call site,
//! in the entry point that parses query TEXT, so validation was a property of
//! one entry point rather than of the engine:
//!
//! - the pre-parsed-AST entry point (`Engine::execute_cypher_ast_with_params`),
//!   which exists to skip a re-parse inside the write lock and is what the RPC
//!   dispatcher uses, called the shared body directly and skipped validation;
//! - and BOTH transports carve a pure autocommit read out of the engine lock
//!   and run it on a cloned `Executor`, which never entered the engine at all.
//!
//! So `RETURN b` erred over HTTP for engine-intercepted shapes and executed
//! silently elsewhere. Validation now sits at the two places every executed
//! query must pass through — the engine's shared AST body and the executor's
//! own parse — and this harness drives both surfaces against the SAME server so
//! they cannot drift apart again.
//!
//! Both surfaces are called as functions rather than over a socket: the RPC
//! dispatcher's `run` takes an `RpcSession`, which is cheap to build, so this
//! exercises the real dispatch path without any transport framing in the way.

use super::*;
use crate::NexusServer;
use crate::protocol::rpc::NexusValue;
use crate::protocol::rpc::dispatch::RpcSession;
use nexus_core::auth::RoleBasedAccessControl;
use nexus_core::database::DatabaseManager;
use nexus_core::testing::TestContext;
use parking_lot::RwLock as PlRwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::RwLock;

fn build_test_server(ctx: &TestContext) -> Arc<NexusServer> {
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
    let engine_arc = Arc::new(RwLock::new(engine));
    let executor = nexus_core::executor::Executor::default();
    let executor_arc = Arc::new(executor);
    let database_manager = DatabaseManager::new(ctx.path().join("databases")).unwrap();
    let database_manager_arc = Arc::new(PlRwLock::new(database_manager));
    let rbac_arc = Arc::new(RwLock::new(RoleBasedAccessControl::new()));
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
    Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        crate::config::RootUserConfig::default(),
    ))
}

/// The query's error over HTTP `/cypher`, or `None` when it succeeded.
async fn http_error(server: &Arc<NexusServer>, query: &str) -> Option<String> {
    execute_cypher(
        State(server.clone()),
        None,
        Json(CypherRequest {
            query: query.to_string(),
            params: HashMap::new(),
            database: None,
        }),
    )
    .await
    .0
    .error
}

/// The query's error over the RPC `CYPHER` command, or `None` when it
/// succeeded.
async fn rpc_error(server: &Arc<NexusServer>, query: &str) -> Option<String> {
    let session = RpcSession {
        server: server.clone(),
        authenticated: Arc::new(AtomicBool::new(true)),
        auth_required: false,
        connection_id: 0,
    };
    crate::protocol::rpc::dispatch::cypher::run(
        &session,
        "CYPHER",
        &[NexusValue::from(query.to_string())],
    )
    .await
    .err()
}

/// One semantically invalid query per check the pass implements. Each must be
/// rejected on BOTH transports, carrying the same openCypher detail token.
///
/// Chosen to span the two bypass shapes as well as the checks: the bare
/// `RETURN`/`WITH` cases take each transport's lock-free read carve-out (no
/// engine lock at all), while the `CREATE` case goes through the engine's
/// pre-parsed-AST write path.
fn invalid_queries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("UndefinedVariable", "RETURN b"),
        ("VariableTypeConflict", "MATCH (r)-[r]->(x) RETURN r"),
        // `CREATE (a {…})` RE-DECLARES with structure a variable `MATCH` already
        // bound — openCypher TCK `clauses/create/Create1.feature`. Using the
        // variable as a bare endpoint (`CREATE (a)-[:T]->(b)`) is legal and is
        // the normal way to attach an edge to a matched node; the bare
        // `CREATE (a)` form is a deliberate carve-out in the check itself (see
        // `check_create_element_rebind`'s `has_structure`), so neither is the
        // shape to assert here.
        (
            "VariableAlreadyBound",
            "MATCH (a) CREATE (a {name: 'foo'}) RETURN a",
        ),
        (
            "InvalidAggregation",
            "MATCH (n) WHERE count(n) > 1 RETURN n",
        ),
        ("NegativeIntegerArgument", "MATCH (n) RETURN n LIMIT -1"),
        ("ColumnNameConflict", "MATCH (n) RETURN n.a AS x, n.b AS x"),
        (
            "RelationshipUniquenessViolation",
            "MATCH (a)-[r]->()-[r]->(a) RETURN r",
        ),
    ]
}

#[tokio::test]
async fn semantically_invalid_queries_are_rejected_on_both_transports() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    for (token, query) in invalid_queries() {
        let http = http_error(&server, query).await;
        let rpc = rpc_error(&server, query).await;

        let http_msg = http.unwrap_or_else(|| panic!("HTTP accepted `{query}`; expected {token}"));
        let rpc_msg = rpc.unwrap_or_else(|| panic!("RPC accepted `{query}`; expected {token}"));

        assert!(
            http_msg.contains(token),
            "HTTP rejected `{query}` with the wrong error: {http_msg}"
        );
        assert!(
            rpc_msg.contains(token),
            "RPC rejected `{query}` with the wrong error: {rpc_msg}"
        );
    }
}

/// The mirror-image guard: a VALID query must still succeed on both
/// transports. Without it the test above could be satisfied by a pass that
/// rejects everything.
#[tokio::test]
async fn valid_queries_still_succeed_on_both_transports() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    for query in [
        "RETURN 1 AS x",
        "CREATE (a:ParityL {n: 1}) RETURN a.n",
        "MATCH (a:ParityL) RETURN a.n",
        "MATCH (a)-[r]->(b) MATCH (c)-[r2]->(d) RETURN count(*)",
    ] {
        assert_eq!(
            http_error(&server, query).await,
            None,
            "HTTP rejected the valid query `{query}`"
        );
        assert_eq!(
            rpc_error(&server, query).await,
            None,
            "RPC rejected the valid query `{query}`"
        );
    }
}
