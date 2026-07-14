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
use nexus_core::executor::parser::{Clause, CypherParser, CypherQuery};

use crate::api::cypher::routing::needs_engine_interception;
use crate::api::cypher::{
    CypherResponse, execute_api_key_commands, execute_database_commands,
    execute_query_management_commands, execute_user_commands,
};
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

/// If the parsed AST contains an admin-level clause (database / user /
/// api-key / query-management), route to the shared REST handler and
/// convert its `CypherResponse` into the RPC envelope. Returns `None`
/// for regular data queries so the caller falls through to
/// `executor.execute()`.
///
/// This is the bridge that lets `SHOW USERS`, `SHOW API KEYS`,
/// `CREATE USER`, `DROP DATABASE`, `TERMINATE QUERY`, etc. run over
/// RPC with the exact same semantics as the REST `/cypher` endpoint.
async fn dispatch_admin_if_any(
    state: &RpcSession,
    ast: &CypherQuery,
    started: Instant,
) -> Option<Result<NexusValue, String>> {
    let has_db = ast.clauses.iter().any(|c| {
        matches!(
            c,
            Clause::CreateDatabase(_)
                | Clause::DropDatabase(_)
                | Clause::ShowDatabases
                | Clause::UseDatabase(_)
        )
    });
    let has_user = ast.clauses.iter().any(|c| {
        matches!(
            c,
            Clause::ShowUsers
                | Clause::ShowUser(_)
                | Clause::CreateUser(_)
                | Clause::DropUser(_)
                | Clause::Grant(_)
                | Clause::Revoke(_)
        )
    });
    let has_api_key = ast.clauses.iter().any(|c| {
        matches!(
            c,
            Clause::CreateApiKey(_)
                | Clause::ShowApiKeys(_)
                | Clause::RevokeApiKey(_)
                | Clause::DeleteApiKey(_)
        )
    });
    let has_query_mgmt = ast
        .clauses
        .iter()
        .any(|c| matches!(c, Clause::ShowQueries | Clause::TerminateQuery(_)));

    // Same waterfall order as the REST handler (api/cypher/execute.rs)
    // so behaviour is identical regardless of transport.
    let resp: Option<CypherResponse> = if has_db {
        Some(
            execute_database_commands(state.server.clone(), ast, started)
                .await
                .0,
        )
    } else if has_api_key {
        Some(
            execute_api_key_commands(state.server.clone(), ast, started)
                .await
                .0,
        )
    } else if has_user {
        Some(
            execute_user_commands(state.server.clone(), ast, started)
                .await
                .0,
        )
    } else if has_query_mgmt {
        Some(
            execute_query_management_commands(state.server.clone(), ast, started)
                .await
                .0,
        )
    } else {
        None
    };

    resp.map(|r| Ok(cypher_response_to_nexus(r)))
}

/// Convert a REST `CypherResponse` (`{columns, rows: Vec<serde_json::Value>,
/// execution_time_ms, error}`) into the canonical RPC envelope that
/// matches `result_set_to_nexus`'s output. Errors surface as a
/// [`Result::Err`] exactly the way the direct-executor path does.
fn cypher_response_to_nexus(resp: CypherResponse) -> NexusValue {
    // If the REST helper reported an error, surface it as a server-
    // error response with no rows. The caller wraps this in `Ok(...)`
    // so we convert the error to a Map envelope with an `error` field;
    // the RPC caller (CLI / SDK) distinguishes by checking whether
    // `rows` is populated. This matches the REST JSON shape byte-for-
    // byte so SDKs decode it identically on both transports.
    let CypherResponse {
        columns,
        rows,
        execution_time_ms,
        error,
        notifications,
    } = resp;

    let columns_val = NexusValue::Array(columns.into_iter().map(NexusValue::Str).collect());
    let rows_val = NexusValue::Array(
        rows.into_iter()
            .map(|row| match row {
                serde_json::Value::Array(arr) => {
                    NexusValue::Array(arr.into_iter().map(json_to_nexus).collect())
                }
                other => NexusValue::Array(vec![json_to_nexus(other)]),
            })
            .collect(),
    );
    let stats = NexusValue::Map(vec![(
        NexusValue::Str("rows".into()),
        NexusValue::Int(match &rows_val {
            NexusValue::Array(a) => a.len() as i64,
            _ => 0,
        }),
    )]);

    let mut entries = vec![
        (NexusValue::Str("columns".into()), columns_val),
        (NexusValue::Str("rows".into()), rows_val),
        (NexusValue::Str("stats".into()), stats),
        (
            NexusValue::Str("execution_time_ms".into()),
            NexusValue::Int(execution_time_ms as i64),
        ),
    ];
    if let Some(e) = error {
        entries.push((NexusValue::Str("error".into()), NexusValue::Str(e)));
    }
    // Mirror the REST envelope's `notifications` field on the RPC
    // wire so SDKs see an identical shape on both transports. Empty
    // vec is omitted to keep the hot-path payload compact.
    if !notifications.is_empty() {
        let notes_val = NexusValue::Array(
            notifications
                .into_iter()
                .map(|n| {
                    NexusValue::Map(vec![
                        (NexusValue::Str("code".into()), NexusValue::Str(n.code)),
                        (NexusValue::Str("title".into()), NexusValue::Str(n.title)),
                        (
                            NexusValue::Str("description".into()),
                            NexusValue::Str(n.description),
                        ),
                        (
                            NexusValue::Str("severity".into()),
                            NexusValue::Str(format!("{:?}", n.severity).to_uppercase()),
                        ),
                        (
                            NexusValue::Str("category".into()),
                            NexusValue::Str(format!("{:?}", n.category).to_uppercase()),
                        ),
                    ])
                })
                .collect(),
        );
        entries.push((NexusValue::Str("notifications".into()), notes_val));
    }
    NexusValue::Map(entries)
}

// ── Shared execution path ─────────────────────────────────────────────────────

async fn execute_query(
    state: &RpcSession,
    query: String,
    params: HashMap<String, serde_json::Value>,
) -> Result<NexusValue, String> {
    let started = Instant::now();

    // Parse once. If the AST carries an admin clause, route through
    // the shared REST helpers so `SHOW USERS` / `SHOW API KEYS` /
    // `CREATE DATABASE` / etc. behave identically over both
    // transports. Otherwise fall through to the plain executor path.
    let ast = match CypherParser::new(query.clone()).parse() {
        Ok(ast) => ast,
        Err(e) => return Err(format!("Parse error: {e}")),
    };

    if let Some(admin_result) = dispatch_admin_if_any(state, &ast, started).await {
        return admin_result;
    }

    // Route mutating and MATCH queries through
    // `engine.execute_cypher_with_params` — the only path that
    // intercepts DELETE / DETACH DELETE / CREATE / MERGE / SET / REMOVE
    // / FOREACH before they hit the executor's operator pipeline. The
    // executor's `Operator::Delete` / `DetachDelete` handler is a no-op
    // that relies on this upstream interception; calling
    // `executor.execute` directly for a query with `DELETE` silently
    // succeeds with zero rows and leaves the database untouched (see
    // phase6_nexus-delete-executor-bug). Threading `params` through here
    // (instead of the params-dropping `execute_cypher(&str)`) fixes
    // silent `$params` data loss on the RPC transport (bug B6).
    //
    // `needs_engine_interception` now lives in `api::cypher::routing`
    // (write-path unification Step 3) so the HTTP `/cypher` handler and
    // this RPC dispatcher share one definition of "needs the engine"
    // instead of keeping independently-drifting copies.
    if needs_engine_interception(&ast) {
        // phase5_lock-free-read-path: mirrors the HTTP `/cypher` handler's
        // carve-out (`api::cypher::execute::handler`) — a pure autocommit
        // read (`routing::is_read_only`) with no open explicit transaction
        // on the "default" session runs through a cloned `Engine::executor`
        // snapshot in `spawn_blocking` instead of the exclusive
        // `engine.write().await` every other clause here still needs. See
        // that handler's inline comments for the freshness argument
        // (`Engine::refresh_executor` keeps `Engine::executor` current
        // after every commit/rollback/write) and the read-your-own-writes
        // argument for staying on the engine inside an explicit
        // transaction.
        if crate::api::cypher::routing::is_read_only(&ast) {
            // phase9_store-lock-read-concurrency §1 — instrument the
            // tokio `engine` RwLock read-guard wait (no-op unless
            // `NEXUS_PERF_PROBE=1`; see `nexus_core::perf_probe`).
            let engine_read_start = std::time::Instant::now();
            let (lock_free_executor, in_explicit_tx) = {
                let engine_guard = state.server.engine.read().await;
                if nexus_core::perf_probe::enabled() {
                    nexus_core::perf_probe::ENGINE_TOKIO_READ.record(engine_read_start.elapsed());
                }
                let in_tx = engine_guard
                    .session_manager
                    .get_session(&"default".to_string())
                    .map(|session| session.has_active_transaction())
                    .unwrap_or(false);
                (engine_guard.executor.clone(), in_tx)
            };

            if !in_explicit_tx {
                let q = Query {
                    cypher: query.clone(),
                    params: params.clone(),
                };
                // phase9_store-lock-read-concurrency §1 — measure the
                // `spawn_blocking` queue wait (time from scheduling to
                // the closure's first instruction) and the executor's
                // own wall time separately, so a busy blocking-thread
                // pool shows up distinctly from slow query execution.
                let scheduled_at = std::time::Instant::now();
                let out = tokio::task::spawn_blocking(move || {
                    if nexus_core::perf_probe::enabled() {
                        nexus_core::perf_probe::SPAWN_BLOCKING_QUEUE.record(scheduled_at.elapsed());
                    }
                    let exec_start = std::time::Instant::now();
                    let result = lock_free_executor.execute(&q);
                    if nexus_core::perf_probe::enabled() {
                        nexus_core::perf_probe::EXECUTOR_EXECUTE.record(exec_start.elapsed());
                    }
                    result
                })
                .await;
                let elapsed_ms = started.elapsed().as_millis() as i64;
                return match out {
                    Ok(Ok(rs)) => Ok(result_set_to_nexus(rs, elapsed_ms)),
                    Ok(Err(e)) => Err(format!("Cypher error: {e}")),
                    Err(join_err) => Err(format!("ERR internal join error: {join_err}")),
                };
            }
            // Else: an explicit transaction is open on the "default"
            // session — fall through to the engine-locked path below.
        }

        // phase8_neo4j-concurrency-gaps §3 — `ast` was already parsed
        // above (outside this write lock) to decide routing; use the
        // pre-parsed-AST entry point so the exclusive lock's critical
        // section no longer pays for a second parse of the same query
        // text. See `Engine::execute_cypher_ast_with_params`'s doc
        // comment.
        let engine_arc = state.server.engine.clone();
        let result = {
            let mut engine = engine_arc.write().await;
            engine.execute_cypher_ast_with_params(&ast, &query, params)
        };
        let elapsed_ms = started.elapsed().as_millis() as i64;
        return match result {
            Ok(rs) => Ok(result_set_to_nexus(rs, elapsed_ms)),
            Err(e) => Err(format!("Cypher error: {e}")),
        };
    }

    let executor = state.server.executor.clone();
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
    async fn cypher_parameterized_write_persists_value() {
        // Regression test for bug B6: RPC's write-path branch
        // (`needs_engine_interception`) used to call the params-dropping
        // `engine.execute_cypher(&query)`, so a write query referencing
        // `$param` could never resolve it — `self.current_params` is only
        // populated by `execute_cypher_with_params`. Dynamic labels
        // (`CREATE (n:$lbl)`) resolve strictly against `current_params`
        // (see `dynamic_labels::resolve_labels`), so an unresolved `$lbl`
        // fails fast with `ERR_INVALID_LABEL` instead of silently
        // no-oping — a deterministic way to prove params actually reach
        // the engine over this transport.
        let s = session();
        let params = NexusValue::Map(vec![(
            NexusValue::Str("lbl".into()),
            NexusValue::Str("RpcParamLabel".into()),
        )]);
        run(
            &s,
            "CYPHER",
            &[NexusValue::Str("CREATE (n:$lbl)".into()), params],
        )
        .await
        .unwrap();

        let out = run(
            &s,
            "CYPHER",
            &[NexusValue::Str(
                "MATCH (n:RpcParamLabel) RETURN count(n) AS c".into(),
            )],
        )
        .await
        .unwrap();
        let pairs = expect_map(out);
        match lookup(&pairs, "rows") {
            NexusValue::Array(rows) => {
                assert_eq!(rows.len(), 1);
                match &rows[0] {
                    NexusValue::Array(cols) => {
                        assert_eq!(
                            cols[0].as_int(),
                            Some(1),
                            "expected exactly one node created under the parameterized label, got {:?}",
                            cols[0]
                        );
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
