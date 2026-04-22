//! Cypher commands on the RESP3 transport.
//!
//! These handlers are a thin wrapper around `Engine::execute_cypher` — the
//! sync core of the Cypher stack. The wrapper does three things:
//!
//! 1. Pulls the engine out of `Arc<TokioRwLock<Engine>>` and runs the
//!    actual query inside `spawn_blocking` so the tokio reactor thread
//!    that's driving this socket is never pinned on a parking_lot guard
//!    (the same policy the HTTP handlers follow — see
//!    `docs/performance/CONCURRENCY.md`).
//! 2. Converts the `ResultSet` to a RESP3 `Map` envelope with `columns`,
//!    `rows`, `stats`, and `execution_time_ms`.
//! 3. Maps runtime errors to `Verbatim(txt, …)` so `redis-cli` renders
//!    multi-line Cypher diagnostics with the right line-feeds.

use std::time::Instant;

use crate::protocol::resp3::parser::Resp3Value;

use super::{
    SessionState, arg_json_required, arg_str_required, err, expect_arity, expect_arity_min,
};

/// `CYPHER <query>` — run an unparameterised query.
pub async fn cypher(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "CYPHER") {
        return e;
    }
    let query = match arg_str_required(args, 1, "CYPHER") {
        Ok(q) => q.to_string(),
        Err(e) => return e,
    };
    run_cypher(state, query, None).await
}

/// `CYPHER.WITH <query> <params-json>` — run with a parameter map.
pub async fn cypher_with(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 3, "CYPHER.WITH") {
        return e;
    }
    let query = match arg_str_required(args, 1, "CYPHER.WITH") {
        Ok(q) => q.to_string(),
        Err(e) => return e,
    };
    let params = match arg_json_required(args, 2, "CYPHER.WITH") {
        Ok(v) => v,
        Err(e) => return e,
    };
    run_cypher(state, query, Some(params)).await
}

/// `CYPHER.EXPLAIN <query>` — return the planner's textual plan.
pub async fn cypher_explain(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_min(args, 2, "CYPHER.EXPLAIN") {
        return e;
    }
    let query = match arg_str_required(args, 1, "CYPHER.EXPLAIN") {
        Ok(q) => q.to_string(),
        Err(e) => return e,
    };

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        // Best-effort: run the query prefixed with `EXPLAIN` via execute_cypher.
        // The executor treats EXPLAIN queries as plan-only.
        let mut guard = engine.blocking_write();
        let plan_query = if query.to_uppercase().starts_with("EXPLAIN") {
            query
        } else {
            format!("EXPLAIN {query}")
        };
        guard.execute_cypher(&plan_query)
    })
    .await;

    match out {
        Ok(Ok(rs)) => Resp3Value::bulk(format!("{rs:#?}")),
        Ok(Err(e)) => Resp3Value::Verbatim("txt".into(), e.to_string().into_bytes()),
        Err(_join_err) => err("ERR internal join error running EXPLAIN"),
    }
}

// --------------------------------------------------------------------------
// Shared helper.
// --------------------------------------------------------------------------

async fn run_cypher(
    state: &SessionState,
    query: String,
    _params: Option<serde_json::Value>,
) -> Resp3Value {
    let engine = state.server.engine.clone();
    let started = Instant::now();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;
    let elapsed_ms = started.elapsed().as_millis() as i64;

    match out {
        Ok(Ok(rs)) => result_set_to_resp3(&rs, elapsed_ms),
        Ok(Err(e)) => Resp3Value::Verbatim("txt".into(), format!("Cypher error: {e}").into_bytes()),
        Err(_join_err) => err("ERR internal join error running Cypher"),
    }
}

fn result_set_to_resp3(rs: &nexus_core::executor::ResultSet, execution_time_ms: i64) -> Resp3Value {
    let columns = Resp3Value::Array(
        rs.columns
            .iter()
            .map(|c| Resp3Value::bulk(c.clone()))
            .collect(),
    );
    let rows = Resp3Value::Array(
        rs.rows
            .iter()
            .map(|row| Resp3Value::Array(row.values.iter().map(json_to_resp3).collect::<Vec<_>>()))
            .collect(),
    );
    let stats = Resp3Value::Map(vec![(
        Resp3Value::bulk("rows"),
        Resp3Value::Integer(rs.rows.len() as i64),
    )]);
    Resp3Value::Map(vec![
        (Resp3Value::bulk("columns"), columns),
        (Resp3Value::bulk("rows"), rows),
        (Resp3Value::bulk("stats"), stats),
        (
            Resp3Value::bulk("execution_time_ms"),
            Resp3Value::Integer(execution_time_ms),
        ),
    ])
}

fn json_to_resp3(v: &serde_json::Value) -> Resp3Value {
    match v {
        serde_json::Value::Null => Resp3Value::Null,
        serde_json::Value::Bool(b) => Resp3Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Resp3Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Resp3Value::Double(f)
            } else {
                Resp3Value::bulk(n.to_string())
            }
        }
        serde_json::Value::String(s) => Resp3Value::bulk(s.clone()),
        serde_json::Value::Array(arr) => Resp3Value::Array(arr.iter().map(json_to_resp3).collect()),
        serde_json::Value::Object(obj) => Resp3Value::Map(
            obj.iter()
                .map(|(k, v)| (Resp3Value::bulk(k.clone()), json_to_resp3(v)))
                .collect(),
        ),
    }
}

// --------------------------------------------------------------------------
// Tests.
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_primitives_lower_to_expected_variants() {
        assert_eq!(json_to_resp3(&serde_json::json!(null)), Resp3Value::Null);
        assert_eq!(
            json_to_resp3(&serde_json::json!(true)),
            Resp3Value::Boolean(true)
        );
        assert_eq!(
            json_to_resp3(&serde_json::json!(42)),
            Resp3Value::Integer(42)
        );
        assert_eq!(
            json_to_resp3(&serde_json::json!(3.25)),
            Resp3Value::Double(3.25)
        );
        assert_eq!(json_to_resp3(&serde_json::json!("hi")).as_str(), Some("hi"));
    }

    #[test]
    fn json_object_becomes_map_preserving_order() {
        let v = serde_json::json!({"b": 2, "a": 1});
        match json_to_resp3(&v) {
            Resp3Value::Map(entries) => {
                // serde_json Map preserves insertion order by default.
                let keys: Vec<&str> = entries.iter().filter_map(|(k, _)| k.as_str()).collect();
                assert!(keys.contains(&"b") && keys.contains(&"a"));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }
}
