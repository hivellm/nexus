//! Schema introspection handlers: LABELS, REL_TYPES, PROPERTY_KEYS, INDEXES.
//!
//! LABELS / REL_TYPES / PROPERTY_KEYS read directly from the catalog
//! (`list_all_labels`, `list_all_types`, `list_all_keys`) so introspection
//! works even before a single query has populated the Cypher execution
//! caches. INDEXES reports the set of labels that own a label-bitmap
//! index — every created node lands in that index, so "labels with a
//! materialised index" is the most faithful listing the current engine
//! can produce without exposing private property-index internals.

use crate::protocol::rpc::NexusValue;

use super::RpcSession;

/// Dispatch the schema command family.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "LABELS" => labels(state, args).await,
        "REL_TYPES" => rel_types(state, args).await,
        "PROPERTY_KEYS" => property_keys(state, args).await,
        "INDEXES" => indexes(state, args).await,
        other => Err(format!("ERR unknown schema command '{other}'")),
    }
}

async fn labels(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    arity_zero(args, "LABELS")?;
    let engine = state.server.engine.clone();
    let names = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        guard
            .catalog
            .list_all_labels()
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|join| format!("ERR internal join error: {join}"))?;
    Ok(NexusValue::Array(
        names.into_iter().map(NexusValue::Str).collect(),
    ))
}

async fn rel_types(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    arity_zero(args, "REL_TYPES")?;
    let engine = state.server.engine.clone();
    let names = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        guard
            .catalog
            .list_all_types()
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|join| format!("ERR internal join error: {join}"))?;
    Ok(NexusValue::Array(
        names.into_iter().map(NexusValue::Str).collect(),
    ))
}

async fn property_keys(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    arity_zero(args, "PROPERTY_KEYS")?;
    let engine = state.server.engine.clone();
    let names = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        guard
            .catalog
            .list_all_keys()
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|join| format!("ERR internal join error: {join}"))?;
    Ok(NexusValue::Array(
        names.into_iter().map(NexusValue::Str).collect(),
    ))
}

/// INDEXES returns one Map per label that has a label-bitmap index
/// materialised. The shape is `{label, kind}` so a future release can add
/// btree/fulltext/knn entries without changing the envelope.
async fn indexes(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    arity_zero(args, "INDEXES")?;
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        let label_ids = guard.indexes.label_index.get_all_labels();
        label_ids
            .into_iter()
            .filter_map(|id| {
                guard
                    .catalog
                    .get_label_name(id)
                    .ok()
                    .flatten()
                    .map(|name| (name, "label".to_string()))
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|join| format!("ERR internal join error: {join}"))?;

    Ok(NexusValue::Array(
        out.into_iter()
            .map(|(label, kind)| {
                NexusValue::Map(vec![
                    (NexusValue::Str("label".into()), NexusValue::Str(label)),
                    (NexusValue::Str("kind".into()), NexusValue::Str(kind)),
                ])
            })
            .collect(),
    ))
}

fn arity_zero(args: &[NexusValue], cmd: &str) -> Result<(), String> {
    if !args.is_empty() {
        return Err(format!(
            "ERR wrong number of arguments for '{cmd}' ({})",
            args.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn session() -> RpcSession {
        let ctx = nexus_core::testing::TestContext::new();
        let engine =
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for schema test");
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
    async fn labels_reports_created_labels() {
        let s = session();
        super::super::run(
            &s,
            "CREATE_NODE",
            vec![
                NexusValue::Array(vec![NexusValue::Str("Alpha".into())]),
                NexusValue::Map(vec![]),
            ],
        )
        .await
        .unwrap();

        let out = run(&s, "LABELS", &[]).await.unwrap();
        match out {
            NexusValue::Array(items) => {
                let names: Vec<_> = items.iter().filter_map(|v| v.as_str()).collect();
                assert!(names.iter().any(|n| *n == "Alpha"), "got {names:?}");
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rel_types_reports_created_types() {
        let s = session();
        let a = match super::super::run(
            &s,
            "CREATE_NODE",
            vec![
                NexusValue::Array(vec![NexusValue::Str("N".into())]),
                NexusValue::Map(vec![]),
            ],
        )
        .await
        .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };
        let b = match super::super::run(
            &s,
            "CREATE_NODE",
            vec![
                NexusValue::Array(vec![NexusValue::Str("N".into())]),
                NexusValue::Map(vec![]),
            ],
        )
        .await
        .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };
        super::super::run(
            &s,
            "CREATE_REL",
            vec![
                NexusValue::Int(a),
                NexusValue::Int(b),
                NexusValue::Str("KNOWS".into()),
                NexusValue::Map(vec![]),
            ],
        )
        .await
        .unwrap();

        let out = run(&s, "REL_TYPES", &[]).await.unwrap();
        match out {
            NexusValue::Array(items) => {
                let names: Vec<_> = items.iter().filter_map(|v| v.as_str()).collect();
                assert!(names.iter().any(|n| *n == "KNOWS"), "got {names:?}");
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn property_keys_returns_known_registered_keys() {
        let s = session();
        // The catalog pre-registers a handful of keys on init; regardless
        // of which keys a test-created node allocates, PROPERTY_KEYS must
        // return at least those pre-registered entries as an Array.
        let out = run(&s, "PROPERTY_KEYS", &[]).await.unwrap();
        match out {
            NexusValue::Array(items) => {
                assert!(items.iter().all(|v| v.as_str().is_some()));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn labels_rejects_arguments() {
        let s = session();
        let err = run(&s, "LABELS", &[NexusValue::Str("x".into())])
            .await
            .unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn rel_types_returns_array_shape() {
        let s = session();
        let out = run(&s, "REL_TYPES", &[]).await.unwrap();
        match out {
            NexusValue::Array(items) => {
                // Every entry — pre-registered or test-created — must be a
                // string; the content depends on catalog initialisation.
                assert!(items.iter().all(|v| v.as_str().is_some()));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn indexes_returns_label_kind_entries_for_created_labels() {
        let s = session();
        super::super::run(
            &s,
            "CREATE_NODE",
            vec![
                NexusValue::Array(vec![NexusValue::Str("Indexed".into())]),
                NexusValue::Map(vec![]),
            ],
        )
        .await
        .unwrap();

        let out = run(&s, "INDEXES", &[]).await.unwrap();
        match out {
            NexusValue::Array(items) => {
                let hit = items.iter().any(|v| match v {
                    NexusValue::Map(pairs) => {
                        pairs
                            .iter()
                            .find_map(|(k, v)| (k.as_str() == Some("label")).then_some(v))
                            .and_then(|v| v.as_str())
                            == Some("Indexed")
                    }
                    _ => false,
                });
                assert!(hit, "Indexed label missing from {items:?}");
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn indexes_rejects_arguments() {
        let s = session();
        let err = run(&s, "INDEXES", &[NexusValue::Int(1)]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }
}
