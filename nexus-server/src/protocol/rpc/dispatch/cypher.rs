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

/// Convert a [`NexusValue`] into the corresponding `serde_json::Value`.
///
/// Bytes are interpreted as UTF-8 text; non-UTF-8 bytes are rejected so a
/// parameter can never silently turn into an encoding-dependent string.
fn nexus_to_json(value: NexusValue) -> Result<serde_json::Value, String> {
    match value {
        NexusValue::Null => Ok(serde_json::Value::Null),
        NexusValue::Bool(b) => Ok(serde_json::Value::Bool(b)),
        NexusValue::Int(i) => Ok(serde_json::Value::Number(i.into())),
        NexusValue::Float(f) => {
            let n = serde_json::Number::from_f64(f).ok_or_else(|| {
                "ERR non-finite Float parameters cannot be represented in JSON".to_string()
            })?;
            Ok(serde_json::Value::Number(n))
        }
        NexusValue::Bytes(b) => {
            let s = String::from_utf8(b)
                .map_err(|_| "ERR Bytes parameter must be valid UTF-8".to_string())?;
            Ok(serde_json::Value::String(s))
        }
        NexusValue::Str(s) => Ok(serde_json::Value::String(s)),
        NexusValue::Array(items) => items
            .into_iter()
            .map(nexus_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        NexusValue::Map(pairs) => {
            let mut map = serde_json::Map::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key = k
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "ERR nested parameter map keys must be strings".to_string())?;
                map.insert(key, nexus_to_json(v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
    }
}

/// Convert a `serde_json::Value` (as returned inside `ResultSet` rows) into
/// the matching [`NexusValue`]. Integer fits become `Int`; the rest of the
/// variants map 1:1.
fn json_to_nexus(value: serde_json::Value) -> NexusValue {
    match value {
        serde_json::Value::Null => NexusValue::Null,
        serde_json::Value::Bool(b) => NexusValue::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                NexusValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                NexusValue::Float(f)
            } else {
                // u64 > i64::MAX — preserve precision as a string.
                NexusValue::Str(n.to_string())
            }
        }
        serde_json::Value::String(s) => NexusValue::Str(s),
        serde_json::Value::Array(items) => {
            NexusValue::Array(items.into_iter().map(json_to_nexus).collect())
        }
        serde_json::Value::Object(obj) => NexusValue::Map(
            obj.into_iter()
                .map(|(k, v)| (NexusValue::Str(k), json_to_nexus(v)))
                .collect(),
        ),
    }
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

    #[test]
    fn nexus_to_json_covers_all_scalar_variants() {
        assert_eq!(
            nexus_to_json(NexusValue::Null).unwrap(),
            serde_json::Value::Null
        );
        assert_eq!(
            nexus_to_json(NexusValue::Bool(true)).unwrap(),
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Int(-7)).unwrap(),
            serde_json::json!(-7)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Float(1.5)).unwrap(),
            serde_json::json!(1.5)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Str("x".into())).unwrap(),
            serde_json::json!("x")
        );
        assert_eq!(
            nexus_to_json(NexusValue::Bytes(b"abc".to_vec())).unwrap(),
            serde_json::json!("abc")
        );
    }

    #[test]
    fn nexus_to_json_rejects_non_finite_float() {
        let err = nexus_to_json(NexusValue::Float(f64::NAN)).unwrap_err();
        assert!(err.contains("non-finite"));
        let err = nexus_to_json(NexusValue::Float(f64::INFINITY)).unwrap_err();
        assert!(err.contains("non-finite"));
    }

    #[test]
    fn nexus_to_json_rejects_non_utf8_bytes() {
        let err = nexus_to_json(NexusValue::Bytes(vec![0xFF, 0xFE])).unwrap_err();
        assert!(err.contains("UTF-8"));
    }

    #[test]
    fn nexus_to_json_preserves_nested_structures() {
        let v = NexusValue::Map(vec![(
            NexusValue::Str("a".into()),
            NexusValue::Array(vec![NexusValue::Int(1), NexusValue::Int(2)]),
        )]);
        let out = nexus_to_json(v).unwrap();
        assert_eq!(out, serde_json::json!({ "a": [1, 2] }));
    }

    #[test]
    fn json_to_nexus_covers_all_variants() {
        assert_eq!(json_to_nexus(serde_json::Value::Null), NexusValue::Null);
        assert_eq!(
            json_to_nexus(serde_json::json!(true)),
            NexusValue::Bool(true)
        );
        assert_eq!(json_to_nexus(serde_json::json!(-3)), NexusValue::Int(-3));
        assert_eq!(
            json_to_nexus(serde_json::json!(2.5)),
            NexusValue::Float(2.5)
        );
        assert_eq!(
            json_to_nexus(serde_json::json!("s")),
            NexusValue::Str("s".into())
        );
        assert_eq!(
            json_to_nexus(serde_json::json!([1, 2])),
            NexusValue::Array(vec![NexusValue::Int(1), NexusValue::Int(2)])
        );
        match json_to_nexus(serde_json::json!({ "k": 1 })) {
            NexusValue::Map(pairs) => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0].0.as_str(), Some("k"));
                assert_eq!(pairs[0].1.as_int(), Some(1));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }
}
