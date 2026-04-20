//! KNN commands: KNN_SEARCH and KNN_TRAVERSE.
//!
//! Embeddings can arrive as either:
//!
//! - [`NexusValue::Bytes`] containing raw little-endian `f32` bytes (length
//!   must be a multiple of 4). This is the preferred SDK encoding since it
//!   avoids the per-query JSON tax.
//! - [`NexusValue::Array`] of [`NexusValue::Float`] / [`NexusValue::Int`]
//!   scalars, for parity with languages whose native numeric types serialise
//!   most naturally to a list.
//!
//! Both encodings decode to the same `Vec<f32>` before calling
//! `Engine::knn_search`.
//!
//! The optional `filter: Map?` argument applies per-property equality
//! predicates to the returned ids. It is enforced via a follow-up Cypher
//! query so the server-side planner performs the filtering — no hand-rolled
//! property reads.

use crate::protocol::rpc::NexusValue;

use super::{RpcSession, arg_array, arg_int, arg_map, arg_str};

/// Dispatch the KNN command family.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "KNN_SEARCH" => knn_search(state, args).await,
        "KNN_TRAVERSE" => knn_traverse(state, args).await,
        other => Err(format!("ERR unknown knn command '{other}'")),
    }
}

// ── KNN_SEARCH [label: Str] [embedding: Bytes|Array<Float>] [k: Int] [filter: Map?] ──

async fn knn_search(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if !(3..=4).contains(&args.len()) {
        return Err(format!(
            "ERR wrong number of arguments for 'KNN_SEARCH' ({})",
            args.len()
        ));
    }
    let label = arg_str(args, 0)?;
    let vector = parse_embedding(&args[1])?;
    let k = arg_int(args, 2)?;
    if k <= 0 {
        return Err("ERR KNN_SEARCH k must be positive".into());
    }
    let k = k as usize;
    let filter_pairs = match args.get(3) {
        Some(_) => Some(arg_map(args, 3)?.to_vec()),
        None => None,
    };

    let engine = state.server.engine.clone();
    let label_for_task = label.clone();
    let out = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        guard.knn_search(&label_for_task, &vector, k)
    })
    .await;

    let raw: Vec<(u64, f32)> = match out {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return Err(format!("ERR KNN_SEARCH failed: {e}")),
        Err(join) => return Err(format!("ERR internal join error: {join}")),
    };

    let kept: Vec<(u64, f32)> = match filter_pairs {
        Some(ref pairs) if !pairs.is_empty() => {
            let allowed = run_filter_check(state, &label, &raw, pairs).await?;
            raw.into_iter()
                .filter(|(id, _)| allowed.contains(id))
                .collect()
        }
        _ => raw,
    };

    Ok(NexusValue::Array(
        kept.into_iter()
            .map(|(id, score)| {
                NexusValue::Map(vec![
                    (NexusValue::Str("id".into()), NexusValue::Int(id as i64)),
                    (
                        NexusValue::Str("score".into()),
                        NexusValue::Float(score as f64),
                    ),
                ])
            })
            .collect(),
    ))
}

// ── KNN_TRAVERSE [seeds: Array<Int>] [depth: Int] [filter: Map?] ─────────────

async fn knn_traverse(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    if !(2..=3).contains(&args.len()) {
        return Err(format!(
            "ERR wrong number of arguments for 'KNN_TRAVERSE' ({})",
            args.len()
        ));
    }
    let seeds = seeds_from_array(arg_array(args, 0)?)?;
    if seeds.is_empty() {
        return Err("ERR KNN_TRAVERSE: at least one seed id required".into());
    }
    let depth = arg_int(args, 1)?;
    if !(0..=32).contains(&depth) {
        return Err("ERR KNN_TRAVERSE: depth must be between 0 and 32".into());
    }
    let filter_pairs = match args.get(2) {
        Some(_) => Some(arg_map(args, 2)?.to_vec()),
        None => None,
    };

    let seed_list = seeds
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let mut cypher = format!("MATCH (s)-[*0..{depth}]->(n) WHERE id(s) IN [{seed_list}]");
    if let Some(ref pairs) = filter_pairs {
        if !pairs.is_empty() {
            for (k, v) in pairs {
                let key = k
                    .as_str()
                    .ok_or_else(|| "ERR filter map keys must be strings".to_string())?;
                cypher.push_str(&format!(" AND n.{key} = {}", super_cypher_literal(v)?));
            }
        }
    }
    cypher.push_str(" RETURN DISTINCT id(n) AS id");

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
                .filter_map(|row| row.values.into_iter().next())
                .filter_map(|v| v.as_i64())
                .map(NexusValue::Int)
                .collect(),
        )),
        Ok(Err(e)) => Err(format!("ERR KNN_TRAVERSE failed: {e}")),
        Err(join) => Err(format!("ERR internal join error: {join}")),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Parse an embedding argument from either Bytes (raw LE `f32`s) or an
/// Array of numeric values.
pub(super) fn parse_embedding(arg: &NexusValue) -> Result<Vec<f32>, String> {
    match arg {
        NexusValue::Bytes(b) => {
            if b.len() % 4 != 0 {
                return Err(format!(
                    "ERR embedding Bytes length must be a multiple of 4 (got {} bytes)",
                    b.len()
                ));
            }
            let mut out = Vec::with_capacity(b.len() / 4);
            for chunk in b.chunks_exact(4) {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                out.push(f32::from_le_bytes(arr));
            }
            Ok(out)
        }
        NexusValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (idx, item) in items.iter().enumerate() {
                let f = item
                    .as_float()
                    .ok_or_else(|| format!("ERR embedding element {idx} must be a number"))?;
                if !f.is_finite() {
                    return Err(format!(
                        "ERR embedding element {idx} must be a finite number"
                    ));
                }
                out.push(f as f32);
            }
            Ok(out)
        }
        _ => Err("ERR embedding must be Bytes or Array<Float>".into()),
    }
}

fn seeds_from_array(items: &[NexusValue]) -> Result<Vec<i64>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let n = item
            .as_int()
            .ok_or_else(|| format!("ERR seed {idx} must be an integer"))?;
        if n < 0 {
            return Err(format!("ERR seed {idx} must be non-negative"));
        }
        out.push(n);
    }
    Ok(out)
}

/// Given the candidate (id, score) pairs from `knn_search`, return the set
/// of ids that pass the filter by re-running a narrow Cypher query.
async fn run_filter_check(
    state: &RpcSession,
    label: &str,
    candidates: &[(u64, f32)],
    filter_pairs: &[(NexusValue, NexusValue)],
) -> Result<std::collections::HashSet<u64>, String> {
    let id_list = candidates
        .iter()
        .map(|(id, _)| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let mut cypher = format!("MATCH (n:{label}) WHERE id(n) IN [{id_list}]");
    for (k, v) in filter_pairs {
        let key = k
            .as_str()
            .ok_or_else(|| "ERR filter map keys must be strings".to_string())?;
        cypher.push_str(&format!(" AND n.{key} = {}", super_cypher_literal(v)?));
    }
    cypher.push_str(" RETURN id(n) AS id");

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&cypher)
    })
    .await;

    let rs = match out {
        Ok(Ok(rs)) => rs,
        Ok(Err(e)) => return Err(format!("ERR KNN filter failed: {e}")),
        Err(join) => return Err(format!("ERR internal join error: {join}")),
    };

    Ok(rs
        .rows
        .into_iter()
        .filter_map(|row| row.values.into_iter().next())
        .filter_map(|v| v.as_i64())
        .map(|id| id as u64)
        .collect())
}

/// Render a [`NexusValue`] as a Cypher literal for inline use inside
/// generated filter clauses. Supports the common scalar types; Arrays and
/// Maps are rejected to keep the generated query well-formed.
fn super_cypher_literal(value: &NexusValue) -> Result<String, String> {
    match value {
        NexusValue::Null => Ok("null".to_string()),
        NexusValue::Bool(b) => Ok(b.to_string()),
        NexusValue::Int(i) => Ok(i.to_string()),
        NexusValue::Float(f) if f.is_finite() => Ok(f.to_string()),
        NexusValue::Float(_) => Err("ERR non-finite Float in filter".into()),
        NexusValue::Str(_) | NexusValue::Bytes(_) => {
            let s = value
                .as_bytes()
                .and_then(|b| std::str::from_utf8(b).ok())
                .ok_or_else(|| "ERR filter string value must be UTF-8".to_string())?;
            Ok(format!(
                "\"{}\"",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            ))
        }
        NexusValue::Array(_) | NexusValue::Map(_) => {
            Err("ERR nested filter values are not supported".into())
        }
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
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for knn test");
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

    #[test]
    fn parse_embedding_from_bytes_roundtrips_values() {
        let raw: Vec<u8> = [1.0_f32, 2.5, -3.25]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let got = parse_embedding(&NexusValue::Bytes(raw)).unwrap();
        assert_eq!(got, vec![1.0, 2.5, -3.25]);
    }

    #[test]
    fn parse_embedding_from_array_accepts_ints_and_floats() {
        let arg = NexusValue::Array(vec![
            NexusValue::Float(1.0),
            NexusValue::Int(2),
            NexusValue::Float(-0.5),
        ]);
        let got = parse_embedding(&arg).unwrap();
        assert_eq!(got, vec![1.0, 2.0, -0.5]);
    }

    #[test]
    fn parse_embedding_bytes_and_array_produce_same_vec() {
        let values = [0.25_f32, -1.5, 3.0];
        let raw: Vec<u8> = values.iter().flat_map(|f| f.to_le_bytes()).collect();
        let as_bytes = parse_embedding(&NexusValue::Bytes(raw)).unwrap();
        let as_array = parse_embedding(&NexusValue::Array(
            values
                .iter()
                .map(|v| NexusValue::Float(*v as f64))
                .collect(),
        ))
        .unwrap();
        assert_eq!(as_bytes, as_array);
    }

    #[test]
    fn parse_embedding_rejects_bytes_not_multiple_of_4() {
        let err = parse_embedding(&NexusValue::Bytes(vec![0, 1, 2])).unwrap_err();
        assert!(err.contains("multiple of 4"));
    }

    #[test]
    fn parse_embedding_rejects_non_finite_array_element() {
        let arg = NexusValue::Array(vec![NexusValue::Float(f64::NAN)]);
        let err = parse_embedding(&arg).unwrap_err();
        assert!(err.contains("finite"));
    }

    #[test]
    fn parse_embedding_rejects_non_numeric_array_element() {
        let arg = NexusValue::Array(vec![NexusValue::Str("x".into())]);
        let err = parse_embedding(&arg).unwrap_err();
        assert!(err.contains("must be a number"));
    }

    #[test]
    fn parse_embedding_rejects_wrong_type() {
        let err = parse_embedding(&NexusValue::Int(1)).unwrap_err();
        assert!(err.contains("must be Bytes or Array"));
    }

    #[tokio::test]
    async fn knn_search_rejects_non_positive_k() {
        let s = session();
        let err = run(
            &s,
            "KNN_SEARCH",
            &[
                NexusValue::Str("L".into()),
                NexusValue::Array(vec![NexusValue::Float(1.0)]),
                NexusValue::Int(0),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("k must be positive"));
    }

    #[tokio::test]
    async fn knn_search_rejects_wrong_arity() {
        let s = session();
        let err = run(&s, "KNN_SEARCH", &[NexusValue::Str("L".into())])
            .await
            .unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn knn_search_surfaces_engine_error_for_unknown_label() {
        let s = session();
        // An empty engine has no label index for "Missing"; the engine
        // returns an error rather than an empty result.
        let err = run(
            &s,
            "KNN_SEARCH",
            &[
                NexusValue::Str("Missing".into()),
                NexusValue::Array(vec![NexusValue::Float(1.0), NexusValue::Float(0.0)]),
                NexusValue::Int(3),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("KNN_SEARCH failed"));
    }

    #[tokio::test]
    async fn knn_traverse_returns_seed_at_depth_0() {
        let s = session();
        // Create a node so we have something to traverse.
        let create_out = super::super::run(
            &s,
            "CREATE_NODE",
            vec![
                NexusValue::Array(vec![NexusValue::Str("Seeded".into())]),
                NexusValue::Map(vec![]),
            ],
        )
        .await
        .unwrap();
        let id = match create_out {
            NexusValue::Int(id) => id,
            other => panic!("{other:?}"),
        };

        let out = run(
            &s,
            "KNN_TRAVERSE",
            &[
                NexusValue::Array(vec![NexusValue::Int(id)]),
                NexusValue::Int(0),
            ],
        )
        .await
        .unwrap();
        match out {
            NexusValue::Array(items) => {
                assert!(items.iter().any(|v| v.as_int() == Some(id)));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn knn_traverse_rejects_empty_seeds() {
        let s = session();
        let err = run(
            &s,
            "KNN_TRAVERSE",
            &[NexusValue::Array(vec![]), NexusValue::Int(1)],
        )
        .await
        .unwrap_err();
        assert!(err.contains("at least one seed"));
    }

    #[tokio::test]
    async fn knn_traverse_rejects_out_of_range_depth() {
        let s = session();
        let err = run(
            &s,
            "KNN_TRAVERSE",
            &[
                NexusValue::Array(vec![NexusValue::Int(0)]),
                NexusValue::Int(100),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("depth must be"));
    }

    #[tokio::test]
    async fn knn_traverse_rejects_non_integer_seed() {
        let s = session();
        let err = run(
            &s,
            "KNN_TRAVERSE",
            &[
                NexusValue::Array(vec![NexusValue::Str("x".into())]),
                NexusValue::Int(1),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("must be an integer"));
    }

    #[test]
    fn super_cypher_literal_escapes_special_characters() {
        assert_eq!(
            super_cypher_literal(&NexusValue::Str(r#"ab"c"#.into())).unwrap(),
            r#""ab\"c""#
        );
        assert_eq!(
            super_cypher_literal(&NexusValue::Str(r"a\b".into())).unwrap(),
            r#""a\\b""#
        );
    }

    #[test]
    fn super_cypher_literal_rejects_nested() {
        assert!(super_cypher_literal(&NexusValue::Array(vec![])).is_err());
        assert!(super_cypher_literal(&NexusValue::Map(vec![])).is_err());
    }
}
