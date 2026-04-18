//! Graph CRUD handlers: CREATE_NODE, CREATE_REL, UPDATE_NODE, DELETE_NODE,
//! MATCH_NODES.
//!
//! All of these are `&mut Engine` operations, so they run inside
//! [`tokio::task::spawn_blocking`] with a `blocking_write` guard so the
//! tokio reactor thread is never pinned on a parking_lot guard — same
//! policy the REST handlers follow.

use crate::protocol::rpc::NexusValue;

use super::convert::{json_to_nexus, pairs_to_json_object};
use super::{RpcSession, arg_array, arg_int, arg_map, arg_str};

/// Dispatch the graph CRUD command family.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "CREATE_NODE" => create_node(state, args).await,
        "CREATE_REL" => create_rel(state, args).await,
        "UPDATE_NODE" => update_node(state, args).await,
        "DELETE_NODE" => delete_node(state, args).await,
        "MATCH_NODES" => match_nodes(state, args).await,
        other => Err(format!("ERR unknown graph command '{other}'")),
    }
}

// ── CREATE_NODE [labels: Array<Str>] [props: Map] ────────────────────────────

async fn create_node(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "ERR wrong number of arguments for 'CREATE_NODE' ({})",
            args.len()
        ));
    }
    let labels = labels_from_array(arg_array(args, 0)?)?;
    let props = pairs_to_json_object(arg_map(args, 1)?)?;

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.create_node(labels, props)
    })
    .await;

    match out {
        Ok(Ok(id)) => Ok(NexusValue::Int(id as i64)),
        Ok(Err(e)) => Err(format!("ERR CREATE_NODE failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── CREATE_REL [src: Int] [dst: Int] [type: Str] [props: Map] ────────────────

async fn create_rel(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 4 {
        return Err(format!(
            "ERR wrong number of arguments for 'CREATE_REL' ({})",
            args.len()
        ));
    }
    let src = arg_int(args, 0)?;
    let dst = arg_int(args, 1)?;
    let ty = arg_str(args, 2)?;
    let props = pairs_to_json_object(arg_map(args, 3)?)?;

    if src < 0 || dst < 0 {
        return Err("ERR CREATE_REL node ids must be non-negative".into());
    }

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.create_relationship(src as u64, dst as u64, ty, props)
    })
    .await;

    match out {
        Ok(Ok(id)) => Ok(NexusValue::Int(id as i64)),
        Ok(Err(e)) => Err(format!("ERR CREATE_REL failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── UPDATE_NODE [id: Int] [props: Map] ───────────────────────────────────────

async fn update_node(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "ERR wrong number of arguments for 'UPDATE_NODE' ({})",
            args.len()
        ));
    }
    let id = arg_int(args, 0)?;
    if id < 0 {
        return Err("ERR UPDATE_NODE id must be non-negative".into());
    }
    let new_props = pairs_to_json_object(arg_map(args, 1)?)?;

    let engine = state.server.engine.clone();
    let uid = id as u64;
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        // Preserve the node's existing labels; UPDATE_NODE semantics only
        // replace the property map.
        let labels = guard
            .get_node(uid)
            .ok()
            .flatten()
            .and_then(|n| guard.catalog.get_labels_from_bitmap(n.label_bits).ok())
            .unwrap_or_default();
        guard
            .update_node(uid, labels.clone(), new_props.clone())
            .map(|_| (uid, labels, new_props))
    })
    .await;

    match out {
        Ok(Ok((uid, labels, props))) => Ok(node_reply(uid, labels, props)),
        Ok(Err(e)) => Err(format!("ERR UPDATE_NODE failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── DELETE_NODE [id: Int] [detach: Bool] ─────────────────────────────────────

async fn delete_node(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "ERR wrong number of arguments for 'DELETE_NODE' ({})",
            args.len()
        ));
    }
    let id = arg_int(args, 0)?;
    if id < 0 {
        return Err("ERR DELETE_NODE id must be non-negative".into());
    }
    let detach = match &args[1] {
        NexusValue::Bool(b) => *b,
        _ => return Err("ERR argument 1 must be a boolean".into()),
    };

    let engine = state.server.engine.clone();
    let uid = id as u64;
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        if detach {
            guard.delete_node_relationships(uid)?;
        }
        guard.delete_node(uid)
    })
    .await;

    match out {
        Ok(Ok(deleted)) => Ok(NexusValue::Bool(deleted)),
        Ok(Err(e)) => Err(format!("ERR DELETE_NODE failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── MATCH_NODES [label: Str] [props: Map] [limit: Int] ───────────────────────

async fn match_nodes(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 3 {
        return Err(format!(
            "ERR wrong number of arguments for 'MATCH_NODES' ({})",
            args.len()
        ));
    }
    let label = arg_str(args, 0)?;
    let filter_pairs = arg_map(args, 1)?.to_vec();
    let limit = arg_int(args, 2)?;
    if limit < 0 {
        return Err("ERR MATCH_NODES limit must be non-negative".into());
    }

    // Compose a Cypher query with inline filter properties: the Cypher
    // optimiser already knows how to combine label scan + property equality.
    let mut cypher = format!("MATCH (n:{label}");
    if !filter_pairs.is_empty() {
        let mut first = true;
        cypher.push_str(" {");
        for (k, v) in &filter_pairs {
            let key = k
                .as_str()
                .ok_or_else(|| "ERR filter map keys must be strings".to_string())?;
            if !first {
                cypher.push_str(", ");
            }
            first = false;
            cypher.push_str(key);
            cypher.push_str(": ");
            cypher.push_str(&cypher_literal(v)?);
        }
        cypher.push('}');
    }
    cypher.push_str(") RETURN n");
    if limit > 0 {
        cypher.push_str(&format!(" LIMIT {limit}"));
    }

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&cypher)
    })
    .await;

    match out {
        Ok(Ok(rs)) => Ok(NexusValue::Array(
            rs.rows
                .into_iter()
                .flat_map(|row| row.values.into_iter().map(json_to_nexus))
                .collect(),
        )),
        Ok(Err(e)) => Err(format!("ERR MATCH_NODES failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Convert an `Array<Str>` argument into an owned `Vec<String>`.
fn labels_from_array(items: &[NexusValue]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        match item.as_str() {
            Some(s) => out.push(s.to_owned()),
            None => return Err(format!("ERR label at index {idx} must be a string")),
        }
    }
    Ok(out)
}

/// Render a [`NexusValue`] as a Cypher literal for embedding in a query
/// string. Covers every variant that can sensibly filter a node match.
fn cypher_literal(value: &NexusValue) -> Result<String, String> {
    match value {
        NexusValue::Null => Ok("null".to_string()),
        NexusValue::Bool(b) => Ok(b.to_string()),
        NexusValue::Int(i) => Ok(i.to_string()),
        NexusValue::Float(f) => {
            if !f.is_finite() {
                return Err("ERR non-finite Float cannot appear in a filter".into());
            }
            Ok(f.to_string())
        }
        NexusValue::Str(_) | NexusValue::Bytes(_) => {
            let s = value
                .as_bytes()
                .and_then(|b| std::str::from_utf8(b).ok())
                .ok_or_else(|| "ERR filter value must be UTF-8".to_string())?;
            Ok(format!(
                "\"{}\"",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            ))
        }
        NexusValue::Array(_) | NexusValue::Map(_) => {
            Err("ERR nested Array/Map filter values are not supported".into())
        }
    }
}

fn node_reply(id: u64, labels: Vec<String>, props: serde_json::Value) -> NexusValue {
    NexusValue::Map(vec![
        (NexusValue::Str("id".into()), NexusValue::Int(id as i64)),
        (
            NexusValue::Str("labels".into()),
            NexusValue::Array(labels.into_iter().map(NexusValue::Str).collect()),
        ),
        (NexusValue::Str("properties".into()), json_to_nexus(props)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn session() -> RpcSession {
        let ctx = nexus_core::testing::TestContext::new();
        let engine =
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for graph test");
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

    fn labels(items: &[&str]) -> NexusValue {
        NexusValue::Array(items.iter().map(|s| NexusValue::Str((*s).into())).collect())
    }

    fn props(pairs: &[(&str, NexusValue)]) -> NexusValue {
        NexusValue::Map(
            pairs
                .iter()
                .map(|(k, v)| (NexusValue::Str((*k).into()), v.clone()))
                .collect(),
        )
    }

    #[tokio::test]
    async fn create_node_returns_id() {
        let s = session();
        let out = run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Person"]),
                props(&[("name", NexusValue::Str("Alice".into()))]),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Int(id) => assert!(id >= 0),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_node_rejects_wrong_arity() {
        let s = session();
        let err = run(&s, "CREATE_NODE", &[]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn create_node_rejects_non_array_labels() {
        let s = session();
        let err = run(
            &s,
            "CREATE_NODE",
            &[NexusValue::Str("not-array".into()), props(&[])],
        )
        .await
        .unwrap_err();
        assert!(err.contains("must be an array"));
    }

    #[tokio::test]
    async fn create_node_rejects_non_string_label_entry() {
        let s = session();
        let bad = NexusValue::Array(vec![NexusValue::Int(1)]);
        let err = run(&s, "CREATE_NODE", &[bad, props(&[])])
            .await
            .unwrap_err();
        assert!(err.contains("must be a string"));
    }

    #[tokio::test]
    async fn create_rel_links_two_nodes() {
        let s = session();
        let a = match run(&s, "CREATE_NODE", &[labels(&["Person"]), props(&[])])
            .await
            .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };
        let b = match run(&s, "CREATE_NODE", &[labels(&["Person"]), props(&[])])
            .await
            .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };

        let out = run(
            &s,
            "CREATE_REL",
            &[
                NexusValue::Int(a),
                NexusValue::Int(b),
                NexusValue::Str("KNOWS".into()),
                props(&[("since", NexusValue::Int(2024))]),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Int(id) => assert!(id >= 0),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_rel_rejects_negative_ids() {
        let s = session();
        let err = run(
            &s,
            "CREATE_REL",
            &[
                NexusValue::Int(-1),
                NexusValue::Int(0),
                NexusValue::Str("T".into()),
                props(&[]),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("non-negative"));
    }

    #[tokio::test]
    async fn update_node_replaces_properties() {
        let s = session();
        let id = match run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Person"]),
                props(&[("name", NexusValue::Str("A".into()))]),
            ],
        )
        .await
        .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };

        let out = run(
            &s,
            "UPDATE_NODE",
            &[
                NexusValue::Int(id),
                props(&[("name", NexusValue::Str("Alice".into()))]),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Map(entries) => {
                let id_val = entries
                    .iter()
                    .find_map(|(k, v)| (k.as_str() == Some("id")).then_some(v))
                    .expect("id missing");
                assert_eq!(id_val.as_int(), Some(id));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_node_rejects_negative_id() {
        let s = session();
        let err = run(&s, "UPDATE_NODE", &[NexusValue::Int(-1), props(&[])])
            .await
            .unwrap_err();
        assert!(err.contains("non-negative"));
    }

    #[tokio::test]
    async fn delete_node_returns_true_on_existing() {
        let s = session();
        let id = match run(&s, "CREATE_NODE", &[labels(&["X"]), props(&[])])
            .await
            .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };
        let out = run(
            &s,
            "DELETE_NODE",
            &[NexusValue::Int(id), NexusValue::Bool(true)],
        )
        .await
        .unwrap();
        assert_eq!(out, NexusValue::Bool(true));
    }

    #[tokio::test]
    async fn delete_node_returns_false_on_missing() {
        let s = session();
        let out = run(
            &s,
            "DELETE_NODE",
            &[NexusValue::Int(999_999), NexusValue::Bool(true)],
        )
        .await
        .unwrap();
        assert_eq!(out, NexusValue::Bool(false));
    }

    #[tokio::test]
    async fn delete_node_rejects_non_boolean_detach() {
        let s = session();
        let err = run(&s, "DELETE_NODE", &[NexusValue::Int(0), NexusValue::Int(1)])
            .await
            .unwrap_err();
        assert!(err.contains("must be a boolean"));
    }

    #[tokio::test]
    async fn match_nodes_returns_array() {
        let s = session();
        run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Thing"]),
                props(&[("tag", NexusValue::Str("a".into()))]),
            ],
        )
        .await
        .unwrap();
        run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Thing"]),
                props(&[("tag", NexusValue::Str("b".into()))]),
            ],
        )
        .await
        .unwrap();

        let out = run(
            &s,
            "MATCH_NODES",
            &[
                NexusValue::Str("Thing".into()),
                props(&[]),
                NexusValue::Int(0),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn match_nodes_filters_by_property() {
        let s = session();
        run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Thing"]),
                props(&[("tag", NexusValue::Str("a".into()))]),
            ],
        )
        .await
        .unwrap();
        run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Thing"]),
                props(&[("tag", NexusValue::Str("b".into()))]),
            ],
        )
        .await
        .unwrap();

        let out = run(
            &s,
            "MATCH_NODES",
            &[
                NexusValue::Str("Thing".into()),
                props(&[("tag", NexusValue::Str("a".into()))]),
                NexusValue::Int(0),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn match_nodes_honours_limit() {
        let s = session();
        for _ in 0..3 {
            run(&s, "CREATE_NODE", &[labels(&["Bulk"]), props(&[])])
                .await
                .unwrap();
        }
        let out = run(
            &s,
            "MATCH_NODES",
            &[
                NexusValue::Str("Bulk".into()),
                props(&[]),
                NexusValue::Int(2),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn match_nodes_rejects_negative_limit() {
        let s = session();
        let err = run(
            &s,
            "MATCH_NODES",
            &[NexusValue::Str("T".into()), props(&[]), NexusValue::Int(-1)],
        )
        .await
        .unwrap_err();
        assert!(err.contains("non-negative"));
    }

    #[tokio::test]
    async fn full_crud_round_trip() {
        let s = session();
        // Create
        let id = match run(
            &s,
            "CREATE_NODE",
            &[
                labels(&["Person"]),
                props(&[("name", NexusValue::Str("Alice".into()))]),
            ],
        )
        .await
        .unwrap()
        {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };

        // Update
        let _ = run(
            &s,
            "UPDATE_NODE",
            &[
                NexusValue::Int(id),
                props(&[("name", NexusValue::Str("Alice B".into()))]),
            ],
        )
        .await
        .unwrap();

        // Match
        let matched = run(
            &s,
            "MATCH_NODES",
            &[
                NexusValue::Str("Person".into()),
                props(&[]),
                NexusValue::Int(10),
            ],
        )
        .await
        .unwrap();
        match matched {
            NexusValue::Array(items) => assert!(!items.is_empty()),
            other => panic!("expected Array, got {other:?}"),
        }

        // Delete
        let deleted = run(
            &s,
            "DELETE_NODE",
            &[NexusValue::Int(id), NexusValue::Bool(true)],
        )
        .await
        .unwrap();
        assert_eq!(deleted, NexusValue::Bool(true));
    }

    #[test]
    fn cypher_literal_handles_basic_types() {
        assert_eq!(cypher_literal(&NexusValue::Null).unwrap(), "null");
        assert_eq!(cypher_literal(&NexusValue::Bool(true)).unwrap(), "true");
        assert_eq!(cypher_literal(&NexusValue::Int(42)).unwrap(), "42");
        assert_eq!(
            cypher_literal(&NexusValue::Str("hi".into())).unwrap(),
            "\"hi\""
        );
        // Escapes double quotes and backslashes.
        assert_eq!(
            cypher_literal(&NexusValue::Str(r#"a\"b"#.into())).unwrap(),
            r#""a\\\"b""#
        );
    }

    #[test]
    fn cypher_literal_rejects_nested() {
        let err = cypher_literal(&NexusValue::Array(vec![])).unwrap_err();
        assert!(err.contains("nested"));
    }

    #[test]
    fn cypher_literal_rejects_non_finite_float() {
        let err = cypher_literal(&NexusValue::Float(f64::NAN)).unwrap_err();
        assert!(err.contains("non-finite"));
    }
}
