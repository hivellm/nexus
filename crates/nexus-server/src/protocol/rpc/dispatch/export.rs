//! `EXPORT` / `IMPORT` RPC handlers.
//!
//! These are the RPC counterparts of the REST `/export` and `/import`
//! endpoints. Both single-frame — bulk exports larger than the frame
//! cap (`DEFAULT_MAX_FRAME_BYTES` = 64 MiB) should still use the REST
//! streaming endpoint; the RPC path is optimised for the common case
//! of small-to-medium exports (schema dumps, small graphs, test
//! fixtures) where the round-trip beats the HTTP handshake cost.
//!
//! ## Wire format
//!
//! ```text
//! EXPORT <format: Str>                          -> Map { format, data: Str, records: Int }
//! EXPORT <format: Str> <query: Str>             -> same, but for a custom export query
//! IMPORT <format: Str> <payload: Str | Bytes>   -> Map { imported: Int, format }
//! ```
//!
//! `format` accepts `"json"` or `"csv"`. The `data` field in the
//! EXPORT response is always a UTF-8 string because both supported
//! formats are text; switching to bytes if a future format emits
//! binary is a wire-compatible addition (the client just checks the
//! variant).

use crate::protocol::rpc::NexusValue;

use super::{RpcSession, arg_str};

/// Default Cypher export query — matches the REST handler's default
/// so RPC and REST behave identically when no custom query is
/// supplied.
const DEFAULT_EXPORT_QUERY: &str = "MATCH (n) RETURN n";

pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "EXPORT" => export(state, args).await,
        "IMPORT" => import(state, args).await,
        other => Err(format!("ERR unknown export command '{other}'")),
    }
}

async fn export(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    let (format, query) = match args.len() {
        1 => (arg_str(args, 0)?, DEFAULT_EXPORT_QUERY.to_string()),
        2 => (arg_str(args, 0)?, arg_str(args, 1)?),
        n => return Err(format!("ERR wrong number of arguments for 'EXPORT' ({n})")),
    };

    let format_lower = format.to_ascii_lowercase();
    if !matches!(format_lower.as_str(), "json" | "csv") {
        return Err(format!(
            "ERR unsupported export format '{format}' (expected 'json' or 'csv')"
        ));
    }

    // Run the selection query through the engine.
    let engine = state.server.engine.clone();
    let query_for_exec = query.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query_for_exec)
    })
    .await;

    let result = match out {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(format!("ERR export query failed: {e}")),
        Err(join) => return Err(format!("ERR internal join error: {join}")),
    };

    let records = result.rows.len() as i64;
    let data = match format_lower.as_str() {
        "json" => serialise_json(&result).map_err(|e| format!("ERR json encode failed: {e}"))?,
        "csv" => serialise_csv(&result),
        _ => unreachable!("format validated above"),
    };

    Ok(NexusValue::Map(vec![
        (
            NexusValue::Str("format".into()),
            NexusValue::Str(format_lower),
        ),
        (NexusValue::Str("records".into()), NexusValue::Int(records)),
        (NexusValue::Str("data".into()), NexusValue::Str(data)),
    ]))
}

async fn import(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "ERR wrong number of arguments for 'IMPORT' ({})",
            args.len()
        ));
    }
    let format = arg_str(args, 0)?;
    let format_lower = format.to_ascii_lowercase();
    if format_lower != "json" {
        // CSV import needs header mapping and a per-label target —
        // REST handles it; RPC sticks to JSON until we design a proper
        // typed schema.
        return Err(format!(
            "ERR unsupported import format '{format}' (expected 'json'; use the REST endpoint for csv)"
        ));
    }

    // The payload is always a string on the wire (either NexusValue::Str
    // or UTF-8 bytes). `arg_str` accepts both.
    let payload = arg_str(args, 1)?;
    let parsed: serde_json::Value = serde_json::from_str(&payload)
        .map_err(|e| format!("ERR payload is not valid JSON: {e}"))?;

    let rows = match &parsed {
        serde_json::Value::Array(a) => a.clone(),
        _ => return Err("ERR JSON payload must be an array of node objects".into()),
    };

    let engine = state.server.engine.clone();
    let rows_clone = rows.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        let mut imported = 0u64;
        for row in rows_clone {
            // Accept either a bare object (treated as properties under
            // the default `Node` label) or a full `{labels:[...], properties:{...}}`
            // envelope.
            let (labels, props) = match row {
                serde_json::Value::Object(mut obj) => {
                    let labels: Vec<String> = obj
                        .remove("labels")
                        .and_then(|v| v.as_array().cloned())
                        .map(|a| {
                            a.into_iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_else(|| vec!["Node".to_string()]);
                    let props = obj
                        .remove("properties")
                        .unwrap_or(serde_json::Value::Object(obj));
                    (labels, props)
                }
                other => (vec!["Node".to_string()], other),
            };
            if guard.create_node(labels, props).is_ok() {
                imported += 1;
            }
        }
        imported
    })
    .await;

    let imported = match out {
        Ok(n) => n,
        Err(join) => return Err(format!("ERR internal join error: {join}")),
    };

    Ok(NexusValue::Map(vec![
        (
            NexusValue::Str("imported".into()),
            NexusValue::Int(imported as i64),
        ),
        (
            NexusValue::Str("format".into()),
            NexusValue::Str(format_lower),
        ),
    ]))
}

fn serialise_json(rs: &nexus_core::executor::ResultSet) -> Result<String, serde_json::Error> {
    let rows: Vec<serde_json::Value> = rs
        .rows
        .iter()
        .map(|row| serde_json::Value::Array(row.values.clone()))
        .collect();
    serde_json::to_string(&rows)
}

fn serialise_csv(rs: &nexus_core::executor::ResultSet) -> String {
    // Minimal CSV: header from column names, each row as
    // comma-separated values with JSON-encoded non-string cells so
    // nested structures round-trip. Matches REST's simple-CSV shape.
    let mut out = String::new();
    out.push_str(&rs.columns.join(","));
    out.push('\n');
    for row in &rs.rows {
        let cells: Vec<String> = row
            .values
            .iter()
            .map(|v| match v {
                serde_json::Value::String(s) => csv_quote(s),
                other => csv_quote(&other.to_string()),
            })
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn csv_quote(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn session() -> RpcSession {
        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
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
            authenticated: Arc::new(AtomicBool::new(false)),
            auth_required: false,
            connection_id: 1,
        }
    }

    #[tokio::test]
    async fn export_rejects_unknown_format() {
        let s = session();
        let err = run(&s, "EXPORT", &[NexusValue::Str("xml".into())])
            .await
            .unwrap_err();
        assert!(err.contains("unsupported export format"));
    }

    #[tokio::test]
    async fn export_with_zero_args_rejected() {
        let s = session();
        let err = run(&s, "EXPORT", &[]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn export_default_query_on_empty_engine_yields_zero_records() {
        let s = session();
        let out = run(&s, "EXPORT", &[NexusValue::Str("json".into())])
            .await
            .expect("export ok on empty graph");
        match out {
            NexusValue::Map(entries) => {
                let records = entries
                    .iter()
                    .find_map(|(k, v)| (k.as_str() == Some("records")).then_some(v))
                    .and_then(|v| v.as_int())
                    .expect("records field");
                assert_eq!(records, 0);
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn import_rejects_non_array_payload() {
        let s = session();
        let err = run(
            &s,
            "IMPORT",
            &[
                NexusValue::Str("json".into()),
                NexusValue::Str("{\"not\":\"array\"}".into()),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("must be an array"));
    }

    #[tokio::test]
    async fn import_rejects_bad_format() {
        let s = session();
        let err = run(
            &s,
            "IMPORT",
            &[NexusValue::Str("yaml".into()), NexusValue::Str("[]".into())],
        )
        .await
        .unwrap_err();
        assert!(err.contains("unsupported import format"));
    }

    #[tokio::test]
    async fn import_empty_array_returns_zero_imported() {
        let s = session();
        let out = run(
            &s,
            "IMPORT",
            &[NexusValue::Str("json".into()), NexusValue::Str("[]".into())],
        )
        .await
        .expect("import of empty array");
        match out {
            NexusValue::Map(entries) => {
                let imported = entries
                    .iter()
                    .find_map(|(k, v)| (k.as_str() == Some("imported")).then_some(v))
                    .and_then(|v| v.as_int())
                    .expect("imported field");
                assert_eq!(imported, 0);
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn csv_quote_escapes_commas_and_quotes() {
        assert_eq!(csv_quote("plain"), "plain");
        assert_eq!(csv_quote("a,b"), "\"a,b\"");
        assert_eq!(csv_quote("a\"b"), "\"a\"\"b\"");
    }
}
