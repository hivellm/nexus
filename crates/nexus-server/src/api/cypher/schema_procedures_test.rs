//! Regression coverage for a P1: bare `CALL db.labels()` /
//! `db.relationshipTypes()` / `db.propertyKeys()` returned zero rows on a
//! live server even though the underlying catalog data was present and
//! correct.
//!
//! Root cause (see `api::cypher::routing::is_engine_clause`'s doc comment
//! for the full writeup): a bare `CALL ...` query parses to
//! `Clause::CallProcedure` only, which `routing::needs_engine_interception`
//! did not recognize, so the query fell through this handler's bottom
//! fallback (`server.executor` — a boot-time `Executor::default()` backed by
//! a throwaway, empty temp-dir `Catalog`) instead of running against the
//! resolved engine's own executor, whose catalog is the live one every
//! write actually goes through.
//!
//! This harness exercises the PUBLIC `/cypher` HTTP handler
//! (`execute_cypher`) exactly like `write_path_parity.rs` does — no direct
//! engine calls — so it fails against the pre-fix routing predicate and
//! passes once `Clause::CallProcedure` / `Clause::CallSubquery` are
//! recognized as engine clauses.
//!
//! Every assertion checks for a *specific, uniquely-named* label / type /
//! property key rather than "non-empty" or an exact row count: the
//! `Catalog::new` test-detection redirect (`catalog/store.rs`) shares one
//! LMDB environment across every `Executor::default()` in the same test
//! binary process, so a same-process sibling test's fall-through traffic
//! could in principle leave stray entries in that shared catalog. A
//! uniquely-named marker sidesteps that without relying on exact-count
//! assertions the way the original benchmark false-positive did (see
//! `proposal.md` §1.3 — `RETURN count(label)` always yielding 1 row masked
//! the same bug for months).

#![allow(unused_imports)]
use super::*;
use crate::NexusServer;
use nexus_core::auth::RoleBasedAccessControl;
use nexus_core::database::DatabaseManager;
use nexus_core::testing::TestContext;
use parking_lot::RwLock as PlRwLock;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Build a fresh, isolated `NexusServer` backed by a temp data dir.
///
/// Uses `Engine::with_isolated_catalog` rather than
/// `write_path_parity::build_test_server`'s `Engine::with_data_dir`: under
/// `cargo test`, `Catalog::new` (which `with_data_dir` uses) redirects to a
/// single shared per-process LMDB environment
/// (`catalog/store.rs::Catalog::with_map_size`'s test-detection branch), and
/// `Executor::default()` (below, for `server.executor`) goes through that
/// exact same redirect — so a naive `with_data_dir` engine and the
/// server's `Executor::default()` would end up pointing at the SAME
/// physical catalog under `cargo test`, silently masking the very bug this
/// harness exists to catch (see this module's doc comment and
/// `proposal.md` §1.2's `CARGO=1` experiment). `with_isolated_catalog`
/// bypasses that redirect for the engine side only, reproducing the real
/// production asymmetry: the engine's catalog is genuinely isolated from
/// `server.executor`'s shared/throwaway one.
fn build_test_server(ctx: &TestContext) -> Arc<NexusServer> {
    let engine = nexus_core::Engine::with_isolated_catalog(ctx.path()).unwrap();
    let engine_arc = Arc::new(RwLock::new(engine));
    let executor = nexus_core::executor::Executor::default();
    let executor_arc = Arc::new(executor);
    let database_manager = DatabaseManager::new(ctx.path().join("databases")).unwrap();
    let database_manager_arc = Arc::new(PlRwLock::new(database_manager));
    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(RwLock::new(rbac));
    let auth_config = nexus_core::auth::AuthConfig::default();
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));
    let jwt_config = nexus_core::auth::JwtConfig::default();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));
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

/// Run a query through the PUBLIC `/cypher` HTTP handler (`execute_cypher`),
/// optionally targeting a named database via `CypherRequest.database`.
async fn run_query_on(
    server: &Arc<NexusServer>,
    query: &str,
    database: Option<&str>,
) -> CypherResponse {
    execute_cypher(
        State(server.clone()),
        None,
        Json(CypherRequest {
            query: query.to_string(),
            params: HashMap::new(),
            database: database.map(str::to_string),
        }),
    )
    .await
    .0
}

fn assert_no_error(resp: &CypherResponse, context: &str) {
    assert!(
        resp.error.is_none(),
        "{}: unexpected error: {:?}",
        context,
        resp.error
    );
}

/// Collect every String value out of a single-column `CypherResponse` (the
/// shape `db.labels` / `db.relationshipTypes` / `db.propertyKeys` all
/// return).
fn string_column(resp: &CypherResponse) -> Vec<String> {
    resp.rows
        .iter()
        .filter_map(|row| row.as_array().and_then(|arr| arr.first()))
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// Seed a uniquely-named label/type/property-key triple via a plain Cypher
/// write (the same path every real client uses), then assert all three
/// schema-introspection procedures — run as bare, standalone `CALL ...`
/// statements exactly as a driver/browser/admin tool would issue them — see
/// the seeded names. `database` is forwarded on every request so this same
/// helper covers both the default database and a freshly created named one.
async fn assert_schema_procedures_see_seeded_data(
    server: &Arc<NexusServer>,
    database: Option<&str>,
    label: &str,
    rel_type: &str,
    prop_key: &str,
) {
    let seed = format!("CREATE (a:{label} {{{prop_key}: 1}})-[:{rel_type}]->(b:{label})",);
    let seed_resp = run_query_on(server, &seed, database).await;
    assert_no_error(&seed_resp, "seed CREATE");

    let labels_resp = run_query_on(server, "CALL db.labels()", database).await;
    assert_no_error(&labels_resp, "CALL db.labels()");
    assert_eq!(labels_resp.columns, vec!["label".to_string()]);
    assert!(
        string_column(&labels_resp).contains(&label.to_string()),
        "db.labels() must include '{label}' after CREATE, got {:?}",
        labels_resp.rows
    );

    let types_resp = run_query_on(server, "CALL db.relationshipTypes()", database).await;
    assert_no_error(&types_resp, "CALL db.relationshipTypes()");
    assert_eq!(types_resp.columns, vec!["relationshipType".to_string()]);
    assert!(
        string_column(&types_resp).contains(&rel_type.to_string()),
        "db.relationshipTypes() must include '{rel_type}' after CREATE, got {:?}",
        types_resp.rows
    );

    let keys_resp = run_query_on(server, "CALL db.propertyKeys()", database).await;
    assert_no_error(&keys_resp, "CALL db.propertyKeys()");
    assert_eq!(keys_resp.columns, vec!["propertyKey".to_string()]);
    assert!(
        string_column(&keys_resp).contains(&prop_key.to_string()),
        "db.propertyKeys() must include '{prop_key}' after CREATE, got {:?}",
        keys_resp.rows
    );
}

#[tokio::test]
async fn schema_procedures_see_seeded_data_on_default_database() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    assert_schema_procedures_see_seeded_data(
        &server,
        None,
        "SchemaProcDefaultDbLabel",
        "SCHEMA_PROC_DEFAULT_DB_REL",
        "schemaProcDefaultDbProp",
    )
    .await;
}

/// NOTE on regression strength: unlike the default-database case above,
/// this one cannot independently prove the pre-fix routing predicate was
/// broken *under `cargo test`* — `DatabaseManager::create_database`
/// (`database/mod.rs`) builds the named database's `Engine` via
/// `Engine::with_data_dir`, which (like `server.executor`'s
/// `Executor::default()`) goes through `Catalog::new`'s shared-test-catalog
/// redirect, so the two coincide in the test process regardless of the
/// routing fix. It still asserts a real, independent claim post-fix — that
/// `resolved_engine` (the request's `database` field) is actually honoured
/// for bare `CALL ...` — which the default-database case cannot cover on
/// its own. Confirmed manually: with `Clause::CallProcedure` /
/// `Clause::CallSubquery` removed from `routing::is_engine_clause`, this
/// case still passes (masked, as expected) while
/// `schema_procedures_see_seeded_data_on_default_database` fails — see the
/// sibling test's doc comment.
#[tokio::test]
async fn schema_procedures_see_seeded_data_on_named_database() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let create_db_resp = run_query_on(&server, "CREATE DATABASE schemaprocnameddb", None).await;
    assert_no_error(&create_db_resp, "CREATE DATABASE schemaprocnameddb");

    assert_schema_procedures_see_seeded_data(
        &server,
        Some("schemaprocnameddb"),
        "SchemaProcNamedDbLabel",
        "SCHEMA_PROC_NAMED_DB_REL",
        "schemaProcNamedDbProp",
    )
    .await;
}
