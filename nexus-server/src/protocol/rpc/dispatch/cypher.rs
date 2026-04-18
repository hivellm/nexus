//! `CYPHER` handler.
//!
//! Two forms:
//!
//! - `CYPHER <query>` — run an unparameterised query through the shared
//!   `Executor`.
//! - `CYPHER <query> <params-map>` — same, but with a parameter map;
//!   values are converted `NexusValue -> serde_json::Value` before handing
//!   off to the executor.
//!
//! Clients that want a plan instead of results embed `EXPLAIN` in the
//! query string itself — the engine handles that in its Cypher parser.
//!
//! The result envelope matches the REST `CypherResponse` shape, encoded as
//! a [`NexusValue::Map`] so SDKs can decode it without an extra schema
//! round-trip:
//!
//! ```text
//! Map {
//!   columns:           Array<Str>,
//!   rows:              Array<Array<NexusValue>>,
//!   stats:             Map { rows: Int },
//!   execution_time_ms: Int,
//! }
//! ```

use std::collections::HashMap;
use std::time::Instant;

use nexus_core::executor::Query;

use crate::protocol::rpc::NexusValue;

use super::convert::{json_to_nexus, nexus_to_json};
use super::{RpcSession, arg_map, arg_str};

/// Dispatch the CYPHER command. Uppercasing and auth gating have already
/// happened in [`super::run`].
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "CYPHER" => cypher(state, args).await,
        other => Err(format!("ERR unknown cypher command '{other}'")),
    }
}

async fn cypher(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    match args.len() {
        1 => {
            let query = arg_str(args, 0)?;
            execute_query(state, query, HashMap::new()).await
        }
        2 => {
            let query = arg_str(args, 0)?;
            let params = params_from_map(arg_map(args, 1)?)?;
            execute_query(state, query, params).await
        }
        n => Err(format!("ERR wrong number of arguments for 'CYPHER' ({n})")),
    }
}

// ── Shared execution path ─────────────────────────────────────────────────────

async fn execute_query(
    state: &RpcSession,
    query: String,
    params: HashMap<String, serde_json::Value>,
) -> Result<NexusValue, String> {
    let executor = state.server.executor.clone();
    let started = Instant::now();
    let q = Query {
        cypher: query,
        params,
    };

    let out = tokio::task::spawn_blocking(move || executor.execute(&q)).await;
    let elapsed_ms = started.elapsed().as_millis() as i64;

    match out {
        Ok(Ok(rs)) => Ok(result_set_to_nexus(rs, elapsed_ms)),
        Ok(Err(e)) => Err(format!("Cypher error: {e}")),
        Err(join_err) => Err(format!("ERR internal join error: {join_err}")),
    }
}

/// Convert a `ResultSet` into the canonical NexusValue envelope described
/// in the module docs.
fn result_set_to_nexus(rs: nexus_core::executor::ResultSet, elapsed_ms: i64) -> NexusValue {
    let columns = NexusValue::Array(rs.columns.into_iter().map(NexusValue::Str).collect());
    let row_count = rs.rows.len() as i64;
    let rows = NexusValue::Array(
        rs.rows
            .into_iter()
            .map(|row| NexusValue::Array(row.values.into_iter().map(json_to_nexus).collect()))
            .collect(),
    );
    let stats = NexusValue::Map(vec![(
        NexusValue::Str("rows".into()),
        NexusValue::Int(row_count),
    )]);

    NexusValue::Map(vec![
        (NexusValue::Str("columns".into()), columns),
        (NexusValue::Str("rows".into()), rows),
        (NexusValue::Str("stats".into()), stats),
        (
            NexusValue::Str("execution_time_ms".into()),
            NexusValue::Int(elapsed_ms),
        ),
    ])
}

/// Convert a client-supplied parameter map (`NexusValue::Map`) into the
/// `HashMap<String, serde_json::Value>` the executor expects. Keys must be
/// strings — anything else is a protocol error.
fn params_from_map(
    pairs: &[(NexusValue, NexusValue)],
) -> Result<HashMap<String, serde_json::Value>, String> {
    let mut out = HashMap::with_capacity(pairs.len());
    for (k, v) in pairs {
        let key = k
            .as_str()
            .ok_or_else(|| "ERR parameter map keys must be strings".to_string())?;
        out.insert(key.to_owned(), nexus_to_json(v.clone())?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn session() -> RpcSession {
        let ctx = nexus_core::testing::TestContext::new();
        let engine =
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for cypher test");
        let engine_arc = Arc::new(tokio::sync::RwLock::new(engine));

        let executor_arc = Arc::new(nexus_core::executor::Executor::default());
        let dbm_arc = Arc::new(parking_lot::RwLock::new(
            nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).expect("dbm init"),
        ));
        let rbac_arc = Arc::new(tokio::sync::RwLock::new(
            nexus_core::auth::RoleBasedAccessControl::new(),
        ));
        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: ctx.path().join("audit"),
                retention_days: 1,
                compress_logs: false,
            })
            .expect("audit init"),
        );
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(
            nexus_core::auth::AuthConfig::default(),
        ));
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
            nexus_core::auth::JwtConfig::default(),
        ));

        let server = Arc::new(crate::NexusServer::new(
            executor_arc,
            engine_arc,
            dbm_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            crate::config::RootUserConfig::default(),
        ));
        let _leaked = Box::leak(Box::new(ctx));

        RpcSession {
            server,
            authenticated: Arc::new(AtomicBool::new(true)),
            auth_required: false,
            connection_id: 1,
        }
    }

    fn expect_map(v: NexusValue) -> Vec<(NexusValue, NexusValue)> {
        match v {
            NexusValue::Map(p) => p,
            other => panic!("expected Map, got {other:?}"),
        }
    }

    fn lookup<'a>(pairs: &'a [(NexusValue, NexusValue)], key: &str) -> &'a NexusValue {
        pairs
            .iter()
            .find_map(|(k, v)| (k.as_str() == Some(key)).then_some(v))
            .unwrap_or_else(|| panic!("key '{key}' missing"))
    }

    #[tokio::test]
    async fn cypher_return_1_produces_single_row() {
        let s = session();
        let out = run(&s, "CYPHER", &[NexusValue::Str("RETURN 1".into())])
            .await
            .unwrap();
        let pairs = expect_map(out);

        match lookup(&pairs, "rows") {
            NexusValue::Array(rows) => {
                assert_eq!(rows.len(), 1);
                match &rows[0] {
                    NexusValue::Array(cols) => {
                        assert_eq!(cols.len(), 1);
                        assert_eq!(cols[0].as_int(), Some(1));
                    }
                    other => panic!("expected row Array, got {other:?}"),
                }
            }
            other => panic!("expected rows Array, got {other:?}"),
        }
        match lookup(&pairs, "execution_time_ms") {
            NexusValue::Int(ms) => assert!(*ms >= 0),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cypher_reports_column_names() {
        let s = session();
        let out = run(&s, "CYPHER", &[NexusValue::Str("RETURN 1 AS x".into())])
            .await
            .unwrap();
        let pairs = expect_map(out);
        match lookup(&pairs, "columns") {
            NexusValue::Array(cols) => {
                assert_eq!(cols.len(), 1);
                assert_eq!(cols[0].as_str(), Some("x"));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cypher_reports_rows_in_stats() {
        let s = session();
        let out = run(&s, "CYPHER", &[NexusValue::Str("RETURN 1".into())])
            .await
            .unwrap();
        let pairs = expect_map(out);
        let stats = match lookup(&pairs, "stats") {
            NexusValue::Map(p) => p.clone(),
            other => panic!("expected stats Map, got {other:?}"),
        };
        assert_eq!(lookup(&stats, "rows").as_int(), Some(1));
    }

    #[tokio::test]
    async fn cypher_accepts_parameter_map_as_second_arg() {
        let s = session();
        let params = NexusValue::Map(vec![(NexusValue::Str("x".into()), NexusValue::Int(42))]);
        let out = run(
            &s,
            "CYPHER",
            &[NexusValue::Str("RETURN $x AS v".into()), params],
        )
        .await
        .unwrap();
        let pairs = expect_map(out);
        match lookup(&pairs, "rows") {
            NexusValue::Array(rows) => {
                assert_eq!(rows.len(), 1);
                match &rows[0] {
                    NexusValue::Array(cols) => {
                        assert_eq!(cols[0].as_int(), Some(42));
                    }
                    other => panic!("expected row Array, got {other:?}"),
                }
            }
            other => panic!("expected rows Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cypher_rejects_non_map_params() {
        let s = session();
        let err = run(
            &s,
            "CYPHER",
            &[
                NexusValue::Str("RETURN 1".into()),
                NexusValue::Int(5), // not a Map
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("must be a map"));
    }

    #[tokio::test]
    async fn cypher_rejects_non_string_keys() {
        let s = session();
        let params = NexusValue::Map(vec![(NexusValue::Int(1), NexusValue::Int(42))]);
        let err = run(&s, "CYPHER", &[NexusValue::Str("RETURN 1".into()), params])
            .await
            .unwrap_err();
        assert!(err.contains("parameter map keys must be strings"));
    }

    #[tokio::test]
    async fn cypher_rejects_missing_argument() {
        let s = session();
        let err = run(&s, "CYPHER", &[]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn cypher_rejects_too_many_arguments() {
        let s = session();
        let err = run(
            &s,
            "CYPHER",
            &[
                NexusValue::Str("RETURN 1".into()),
                NexusValue::Map(vec![]),
                NexusValue::Int(1),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn cypher_wraps_executor_error() {
        let s = session();
        let err = run(&s, "CYPHER", &[NexusValue::Str("NOT CYPHER".into())])
            .await
            .unwrap_err();
        assert!(err.contains("Cypher error"));
    }

    #[tokio::test]
    async fn cypher_pipes_explain_prefix_through_executor() {
        let s = session();
        // EXPLAIN is handled by the Cypher parser itself; the dispatcher
        // does not need special-casing. We verify the handler routes the
        // query without synthesising a response of its own.
        let out = run(
            &s,
            "CYPHER",
            &[NexusValue::Str(
                "EXPLAIN CREATE (n:Foo {id: 1}) RETURN n".into(),
            )],
        )
        .await;
        // Accept either success (if the executor can plan it) or a
        // "Cypher error" surfacing the planner/engine message; what we
        // assert is that the dispatcher never produces a panic-driven
        // unwrap or a non-Cypher error.
        match out {
            Ok(NexusValue::Map(_)) => {}
            Err(msg) => assert!(msg.contains("Cypher error") || msg.contains("ERR")),
            Ok(other) => panic!("unexpected success shape: {other:?}"),
        }
    }
}
