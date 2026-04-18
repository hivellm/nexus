//! Bulk ingest handler: INGEST.
//!
//! Takes two arrays of Maps — nodes and relationships — and performs the
//! corresponding inserts under a single write lock so the engine observes
//! the batch atomically from the catalog's point of view (though each
//! record still goes through its own WAL entry).
//!
//! Node map shape:
//!     { "labels": Array<Str>, "properties": Map }
//!
//! Relationship map shape:
//!     { "src": Int, "dst": Int, "type": Str, "properties": Map }
//!
//! Return envelope:
//!     Map {
//!       nodes:        Map { created: Int, errors: Int },
//!       relationships: Map { created: Int, errors: Int },
//!     }

use crate::protocol::rpc::NexusValue;

use super::convert::pairs_to_json_object;
use super::{RpcSession, arg_array};

/// Dispatch the INGEST command.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "INGEST" => ingest(state, args).await,
        other => Err(format!("ERR unknown ingest command '{other}'")),
    }
}

async fn ingest(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "ERR wrong number of arguments for 'INGEST' ({})",
            args.len()
        ));
    }
    let nodes = arg_array(args, 0)?.to_vec();
    let rels = arg_array(args, 1)?.to_vec();

    // Normalise inputs up-front so a malformed map stops the batch with a
    // clear error instead of half-applying.
    let node_payload = prepare_nodes(&nodes)?;
    let rel_payload = prepare_rels(&rels)?;

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        let mut node_created: i64 = 0;
        let mut node_errors: i64 = 0;
        for (labels, props) in node_payload {
            match guard.create_node(labels, props) {
                Ok(_) => node_created += 1,
                Err(_) => node_errors += 1,
            }
        }
        let mut rel_created: i64 = 0;
        let mut rel_errors: i64 = 0;
        for (src, dst, ty, props) in rel_payload {
            match guard.create_relationship(src, dst, ty, props) {
                Ok(_) => rel_created += 1,
                Err(_) => rel_errors += 1,
            }
        }
        (node_created, node_errors, rel_created, rel_errors)
    })
    .await;

    match out {
        Ok((nc, ne, rc, re)) => Ok(NexusValue::Map(vec![
            (
                NexusValue::Str("nodes".into()),
                NexusValue::Map(vec![
                    (NexusValue::Str("created".into()), NexusValue::Int(nc)),
                    (NexusValue::Str("errors".into()), NexusValue::Int(ne)),
                ]),
            ),
            (
                NexusValue::Str("relationships".into()),
                NexusValue::Map(vec![
                    (NexusValue::Str("created".into()), NexusValue::Int(rc)),
                    (NexusValue::Str("errors".into()), NexusValue::Int(re)),
                ]),
            ),
        ])),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

fn prepare_nodes(items: &[NexusValue]) -> Result<Vec<(Vec<String>, serde_json::Value)>, String> {
    items
        .iter()
        .enumerate()
        .map(|(idx, v)| match v {
            NexusValue::Map(pairs) => {
                let labels = lookup_labels(pairs).map_err(|e| node_err(idx, &e))?;
                let props = lookup_properties(pairs).map_err(|e| node_err(idx, &e))?;
                Ok((labels, props))
            }
            _ => Err(format!("ERR node at index {idx} must be a map")),
        })
        .collect()
}

fn prepare_rels(
    items: &[NexusValue],
) -> Result<Vec<(u64, u64, String, serde_json::Value)>, String> {
    items
        .iter()
        .enumerate()
        .map(|(idx, v)| match v {
            NexusValue::Map(pairs) => {
                let src = lookup_i64(pairs, "src")
                    .ok_or_else(|| rel_err(idx, "missing 'src' (integer)"))?;
                let dst = lookup_i64(pairs, "dst")
                    .ok_or_else(|| rel_err(idx, "missing 'dst' (integer)"))?;
                let ty = lookup_str(pairs, "type")
                    .ok_or_else(|| rel_err(idx, "missing 'type' (string)"))?;
                if src < 0 || dst < 0 {
                    return Err(rel_err(idx, "src/dst must be non-negative"));
                }
                let props = lookup_properties(pairs).map_err(|e| rel_err(idx, &e))?;
                Ok((src as u64, dst as u64, ty, props))
            }
            _ => Err(format!("ERR relationship at index {idx} must be a map")),
        })
        .collect()
}

fn lookup_labels(pairs: &[(NexusValue, NexusValue)]) -> Result<Vec<String>, String> {
    let Some(v) = pairs.iter().find_map(|(k, v)| {
        if k.as_str() == Some("labels") {
            Some(v)
        } else {
            None
        }
    }) else {
        return Ok(Vec::new());
    };
    match v {
        NexusValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (idx, item) in items.iter().enumerate() {
                match item.as_str() {
                    Some(s) => out.push(s.to_owned()),
                    None => return Err(format!("label at position {idx} is not a string")),
                }
            }
            Ok(out)
        }
        _ => Err("'labels' must be an array of strings".into()),
    }
}

fn lookup_properties(pairs: &[(NexusValue, NexusValue)]) -> Result<serde_json::Value, String> {
    let Some(v) = pairs.iter().find_map(|(k, v)| {
        if k.as_str() == Some("properties") {
            Some(v)
        } else {
            None
        }
    }) else {
        return Ok(serde_json::json!({}));
    };
    match v {
        NexusValue::Map(inner) => pairs_to_json_object(inner),
        _ => Err("'properties' must be a map".into()),
    }
}

fn lookup_i64(pairs: &[(NexusValue, NexusValue)], key: &str) -> Option<i64> {
    pairs.iter().find_map(|(k, v)| {
        if k.as_str() == Some(key) {
            v.as_int()
        } else {
            None
        }
    })
}

fn lookup_str(pairs: &[(NexusValue, NexusValue)], key: &str) -> Option<String> {
    pairs.iter().find_map(|(k, v)| {
        if k.as_str() == Some(key) {
            v.as_str().map(str::to_owned)
        } else {
            None
        }
    })
}

fn node_err(idx: usize, reason: &str) -> String {
    format!("ERR node at index {idx}: {reason}")
}

fn rel_err(idx: usize, reason: &str) -> String {
    format!("ERR relationship at index {idx}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn session() -> RpcSession {
        let ctx = nexus_core::testing::TestContext::new();
        let engine =
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for ingest test");
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

    fn node_map(labels: &[&str], props: &[(&str, NexusValue)]) -> NexusValue {
        let mut entries = Vec::with_capacity(2);
        entries.push((
            NexusValue::Str("labels".into()),
            NexusValue::Array(
                labels
                    .iter()
                    .map(|l| NexusValue::Str((*l).into()))
                    .collect(),
            ),
        ));
        entries.push((
            NexusValue::Str("properties".into()),
            NexusValue::Map(
                props
                    .iter()
                    .map(|(k, v)| (NexusValue::Str((*k).into()), v.clone()))
                    .collect(),
            ),
        ));
        NexusValue::Map(entries)
    }

    fn rel_map(src: i64, dst: i64, ty: &str, props: &[(&str, NexusValue)]) -> NexusValue {
        NexusValue::Map(vec![
            (NexusValue::Str("src".into()), NexusValue::Int(src)),
            (NexusValue::Str("dst".into()), NexusValue::Int(dst)),
            (NexusValue::Str("type".into()), NexusValue::Str(ty.into())),
            (
                NexusValue::Str("properties".into()),
                NexusValue::Map(
                    props
                        .iter()
                        .map(|(k, v)| (NexusValue::Str((*k).into()), v.clone()))
                        .collect(),
                ),
            ),
        ])
    }

    fn extract_counts(v: &NexusValue, key: &str) -> (i64, i64) {
        let pairs = match v {
            NexusValue::Map(p) => p,
            other => panic!("expected Map, got {other:?}"),
        };
        let sub = pairs
            .iter()
            .find_map(|(k, v)| (k.as_str() == Some(key)).then_some(v))
            .expect("key missing");
        let sub_pairs = match sub {
            NexusValue::Map(p) => p,
            other => panic!("expected inner Map, got {other:?}"),
        };
        let get = |name: &str| {
            sub_pairs
                .iter()
                .find_map(|(k, v)| (k.as_str() == Some(name)).then_some(v))
                .and_then(|v| v.as_int())
                .unwrap_or(-1)
        };
        (get("created"), get("errors"))
    }

    #[tokio::test]
    async fn ingest_creates_nodes_and_relationships() {
        let s = session();
        let nodes = NexusValue::Array(vec![
            node_map(&["Person"], &[("name", NexusValue::Str("A".into()))]),
            node_map(&["Person"], &[("name", NexusValue::Str("B".into()))]),
        ]);

        // Phase 1 of the ingest batch — nodes only, so we can discover ids.
        let first = run(&s, "INGEST", &[nodes.clone(), NexusValue::Array(vec![])])
            .await
            .unwrap();
        let (nc, ne) = extract_counts(&first, "nodes");
        assert_eq!(nc, 2);
        assert_eq!(ne, 0);

        // Now ingest a relationship between the two nodes we just created.
        // They will have ids 0 and 1 on a fresh engine.
        let rels = NexusValue::Array(vec![rel_map(
            0,
            1,
            "KNOWS",
            &[("since", NexusValue::Int(2024))],
        )]);
        let second = run(&s, "INGEST", &[NexusValue::Array(vec![]), rels])
            .await
            .unwrap();
        let (rc, re) = extract_counts(&second, "relationships");
        assert_eq!(rc, 1);
        assert_eq!(re, 0);
    }

    #[tokio::test]
    async fn ingest_reports_errors_for_bad_relationship() {
        let s = session();
        let rels = NexusValue::Array(vec![rel_map(999_999, 888_888, "T", &[])]);
        let out = run(&s, "INGEST", &[NexusValue::Array(vec![]), rels])
            .await
            .unwrap();
        let (_rc, re) = extract_counts(&out, "relationships");
        assert_eq!(re, 1);
    }

    #[tokio::test]
    async fn ingest_rejects_non_map_node_entry() {
        let s = session();
        let err = run(
            &s,
            "INGEST",
            &[
                NexusValue::Array(vec![NexusValue::Int(1)]),
                NexusValue::Array(vec![]),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("must be a map"));
    }

    #[tokio::test]
    async fn ingest_rejects_relationship_missing_src() {
        let s = session();
        let bad = NexusValue::Map(vec![(
            NexusValue::Str("type".into()),
            NexusValue::Str("T".into()),
        )]);
        let err = run(
            &s,
            "INGEST",
            &[NexusValue::Array(vec![]), NexusValue::Array(vec![bad])],
        )
        .await
        .unwrap_err();
        assert!(err.contains("missing 'src'"));
    }

    #[tokio::test]
    async fn ingest_rejects_relationship_missing_type() {
        let s = session();
        let bad = NexusValue::Map(vec![
            (NexusValue::Str("src".into()), NexusValue::Int(0)),
            (NexusValue::Str("dst".into()), NexusValue::Int(1)),
        ]);
        let err = run(
            &s,
            "INGEST",
            &[NexusValue::Array(vec![]), NexusValue::Array(vec![bad])],
        )
        .await
        .unwrap_err();
        assert!(err.contains("missing 'type'"));
    }

    #[tokio::test]
    async fn ingest_rejects_wrong_arity() {
        let s = session();
        let err = run(&s, "INGEST", &[]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn ingest_empty_batches_succeed_with_zero_counts() {
        let s = session();
        let out = run(
            &s,
            "INGEST",
            &[NexusValue::Array(vec![]), NexusValue::Array(vec![])],
        )
        .await
        .unwrap();
        assert_eq!(extract_counts(&out, "nodes"), (0, 0));
        assert_eq!(extract_counts(&out, "relationships"), (0, 0));
    }
}
