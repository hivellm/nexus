//! Multi-database management: DB_LIST, DB_CREATE, DB_DROP, DB_USE.
//!
//! These route to `DatabaseManager` directly rather than through Cypher;
//! Cypher's database management clauses are rejected by `execute_cypher`
//! because the engine does not own the manager.

use crate::protocol::rpc::NexusValue;

use super::{RpcSession, arg_str};

/// Dispatch the database command family.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "DB_LIST" => db_list(state, args).await,
        "DB_CREATE" => db_create(state, args).await,
        "DB_DROP" => db_drop(state, args).await,
        "DB_USE" => db_use(state, args).await,
        other => Err(format!("ERR unknown database command '{other}'")),
    }
}

// ── DB_LIST ─────────────────────────────────────────────────────────────────

async fn db_list(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if !args.is_empty() {
        return Err(format!(
            "ERR wrong number of arguments for 'DB_LIST' ({})",
            args.len()
        ));
    }
    let dbm = state.server.database_manager.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.list_databases()
            .into_iter()
            .map(|info| info.name)
            .collect::<Vec<_>>()
    })
    .await;

    match out {
        Ok(names) => Ok(NexusValue::Array(
            names.into_iter().map(NexusValue::Str).collect(),
        )),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── DB_CREATE <name> ────────────────────────────────────────────────────────

async fn db_create(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "ERR wrong number of arguments for 'DB_CREATE' ({})",
            args.len()
        ));
    }
    let name = arg_str(args, 0)?;
    let dbm = state.server.database_manager.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.create_database(&name).map(|_| ())
    })
    .await;

    match out {
        Ok(Ok(())) => Ok(NexusValue::Str("OK".into())),
        Ok(Err(e)) => Err(format!("ERR DB_CREATE failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── DB_DROP <name> ──────────────────────────────────────────────────────────

async fn db_drop(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "ERR wrong number of arguments for 'DB_DROP' ({})",
            args.len()
        ));
    }
    let name = arg_str(args, 0)?;
    let dbm = state.server.database_manager.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.drop_database(&name, false).map(|_| ())
    })
    .await;

    match out {
        Ok(Ok(())) => Ok(NexusValue::Str("OK".into())),
        Ok(Err(e)) => Err(format!("ERR DB_DROP failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── DB_USE <name> ───────────────────────────────────────────────────────────

/// `DB_USE <name>` validates that the named database exists and returns
/// `"OK"`. Session-level database routing is REST-only in this release;
/// the command is still useful for ops as an existence check.
async fn db_use(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "ERR wrong number of arguments for 'DB_USE' ({})",
            args.len()
        ));
    }
    let name = arg_str(args, 0)?;
    let dbm = state.server.database_manager.clone();
    let exists = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.exists(&name)
    })
    .await
    .map_err(|join| format!("ERR internal join error: {join}"))?;

    if exists {
        Ok(NexusValue::Str("OK".into()))
    } else {
        Err("ERR DB_USE: database not found".into())
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
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for database test");
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

    #[tokio::test]
    async fn db_list_returns_array_of_names() {
        let s = session();
        let out = run(&s, "DB_LIST", &[]).await.unwrap();
        match out {
            NexusValue::Array(items) => {
                // A fresh manager contains the default `neo4j` database.
                assert!(
                    items
                        .iter()
                        .any(|v| matches!(v.as_str(), Some(n) if !n.is_empty()))
                );
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn db_create_and_list_and_use_round_trip() {
        let s = session();
        let create = run(&s, "DB_CREATE", &[NexusValue::Str("scratch".into())])
            .await
            .unwrap();
        assert_eq!(create, NexusValue::Str("OK".into()));

        let listed = run(&s, "DB_LIST", &[]).await.unwrap();
        match listed {
            NexusValue::Array(items) => {
                assert!(items.iter().any(|v| v.as_str() == Some("scratch")))
            }
            other => panic!("expected Array, got {other:?}"),
        }

        let used = run(&s, "DB_USE", &[NexusValue::Str("scratch".into())])
            .await
            .unwrap();
        assert_eq!(used, NexusValue::Str("OK".into()));
    }

    #[tokio::test]
    async fn db_drop_removes_the_database() {
        let s = session();
        run(&s, "DB_CREATE", &[NexusValue::Str("ephemeral".into())])
            .await
            .unwrap();
        let drop = run(&s, "DB_DROP", &[NexusValue::Str("ephemeral".into())])
            .await
            .unwrap();
        assert_eq!(drop, NexusValue::Str("OK".into()));

        let err = run(&s, "DB_USE", &[NexusValue::Str("ephemeral".into())])
            .await
            .unwrap_err();
        assert!(err.contains("database not found"));
    }

    #[tokio::test]
    async fn db_use_rejects_missing_database() {
        let s = session();
        let err = run(&s, "DB_USE", &[NexusValue::Str("nowhere".into())])
            .await
            .unwrap_err();
        assert!(err.contains("database not found"));
    }

    #[tokio::test]
    async fn db_create_rejects_wrong_arity() {
        let s = session();
        let err = run(&s, "DB_CREATE", &[]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }
}
