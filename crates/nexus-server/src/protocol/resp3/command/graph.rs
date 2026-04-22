//! Graph CRUD commands: NODE.*, REL.*.
//!
//! These route straight to `Engine::create_node` / `create_relationship` /
//! `get_node` / `update_node` / `delete_node` / `delete_relationship`, all
//! of which are sync methods — same `spawn_blocking` pattern as
//! [`super::cypher`].

use crate::protocol::resp3::parser::Resp3Value;

use super::{
    SessionState, arg_int_required, arg_json_required, arg_str_required, err, expect_arity,
    expect_arity_range,
};

/// `NODE.CREATE <labels-csv> <props-json>` — returns `:<id>`.
pub async fn node_create(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 3, "NODE.CREATE") {
        return e;
    }
    let labels_csv = match arg_str_required(args, 1, "NODE.CREATE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let props = match arg_json_required(args, 2, "NODE.CREATE") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let labels: Vec<String> = labels_csv
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.create_node(labels, props)
    })
    .await;

    match out {
        Ok(Ok(id)) => Resp3Value::Integer(id as i64),
        Ok(Err(e)) => err(format!("ERR NODE.CREATE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `NODE.GET <id>` -> Map{id, labels, props}.
pub async fn node_get(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "NODE.GET") {
        return e;
    }
    let id = match arg_int_required(args, 1, "NODE.GET") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        // `get_node` takes `&mut self` because it may refresh the executor
        // cache on miss, so we acquire the write guard from inside the
        // blocking task.
        let mut guard = engine.blocking_write();
        guard.get_node(id)
    })
    .await;
    match out {
        Ok(Ok(Some(node))) => node_to_resp3(id, &node),
        Ok(Ok(None)) => Resp3Value::Null,
        Ok(Err(e)) => err(format!("ERR NODE.GET failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `NODE.UPDATE <id> <props-json>` — replaces the stored property map.
pub async fn node_update(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 3, "NODE.UPDATE") {
        return e;
    }
    let id = match arg_int_required(args, 1, "NODE.UPDATE") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let props = match arg_json_required(args, 2, "NODE.UPDATE") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        // Re-use existing labels so UPDATE only changes props.
        let labels = guard
            .get_node(id)
            .ok()
            .flatten()
            .and_then(|n| guard.catalog.get_labels_from_bitmap(n.label_bits).ok())
            .unwrap_or_default();
        guard.update_node(id, labels, props)
    })
    .await;
    match out {
        Ok(Ok(())) => Resp3Value::SimpleString("OK".into()),
        Ok(Err(e)) => err(format!("ERR NODE.UPDATE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `NODE.DELETE <id> [DETACH]` -> Integer(count_deleted).
///
/// `DETACH` first clears every relationship attached to the node and then
/// deletes the node itself. Without `DETACH`, the delete will fail if the
/// node still has relationships — matching Neo4j semantics.
pub async fn node_delete(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_range(args, 2, 3, "NODE.DELETE") {
        return e;
    }
    let id = match arg_int_required(args, 1, "NODE.DELETE") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let detach = args
        .get(2)
        .and_then(Resp3Value::as_str)
        .map(str::to_ascii_uppercase)
        .as_deref()
        == Some("DETACH");
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        if detach {
            guard.delete_node_relationships(id)?;
        }
        guard.delete_node(id)
    })
    .await;
    match out {
        Ok(Ok(true)) => Resp3Value::Integer(1),
        Ok(Ok(false)) => Resp3Value::Integer(0),
        Ok(Err(e)) => err(format!("ERR NODE.DELETE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `NODE.MATCH <label> <props-json> [LIMIT <n>]`.
pub async fn node_match(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_range(args, 3, 5, "NODE.MATCH") {
        return e;
    }
    let label = match arg_str_required(args, 1, "NODE.MATCH") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let _filter = match arg_json_required(args, 2, "NODE.MATCH") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let limit: Option<i64> = if args.len() >= 5 {
        let kw = args
            .get(3)
            .and_then(Resp3Value::as_str)
            .map(str::to_ascii_uppercase);
        if kw.as_deref() == Some("LIMIT") {
            args.get(4).and_then(Resp3Value::as_int)
        } else {
            None
        }
    } else {
        None
    };

    // Fallback: run as Cypher so we don't duplicate filtering here.
    let limit_clause = limit.map(|n| format!(" LIMIT {n}")).unwrap_or_default();
    let query = format!("MATCH (n:{label}) RETURN n{limit_clause}");
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;
    match out {
        Ok(Ok(rs)) => {
            let rows: Vec<Resp3Value> = rs
                .rows
                .iter()
                .flat_map(|row| row.values.iter().cloned())
                .map(|v| json_value_to_resp3(&v))
                .collect();
            Resp3Value::Array(rows)
        }
        Ok(Err(e)) => err(format!("ERR NODE.MATCH failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `REL.CREATE <src> <dst> <type> <props-json>`.
pub async fn rel_create(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 5, "REL.CREATE") {
        return e;
    }
    let src = match arg_int_required(args, 1, "REL.CREATE") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let dst = match arg_int_required(args, 2, "REL.CREATE") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let ty = match arg_str_required(args, 3, "REL.CREATE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let props = match arg_json_required(args, 4, "REL.CREATE") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.create_relationship(src, dst, ty, props)
    })
    .await;
    match out {
        Ok(Ok(id)) => Resp3Value::Integer(id as i64),
        Ok(Err(e)) => err(format!("ERR REL.CREATE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `REL.GET <id>` -> Map.
pub async fn rel_get(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "REL.GET") {
        return e;
    }
    let id = match arg_int_required(args, 1, "REL.GET") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        // `get_relationship` takes `&mut self` for the same reason as
        // `get_node` — executor cache refresh on miss.
        let mut guard = engine.blocking_write();
        guard.get_relationship(id)
    })
    .await;
    match out {
        Ok(Ok(Some(rec))) => {
            let entries: Vec<(Resp3Value, Resp3Value)> = vec![
                (Resp3Value::bulk("id"), Resp3Value::Integer(id as i64)),
                (
                    Resp3Value::bulk("src"),
                    Resp3Value::Integer(rec.src_id as i64),
                ),
                (
                    Resp3Value::bulk("dst"),
                    Resp3Value::Integer(rec.dst_id as i64),
                ),
                (
                    Resp3Value::bulk("type_id"),
                    Resp3Value::Integer(rec.type_id as i64),
                ),
            ];
            Resp3Value::Map(entries)
        }
        Ok(Ok(None)) => Resp3Value::Null,
        Ok(Err(e)) => err(format!("ERR REL.GET failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `REL.DELETE <id>` -> Integer(1 on success, 0 on not-found).
///
/// The engine does not currently expose a standalone `delete_relationship`
/// API (only the bulk `delete_node_relationships(node_id)` path, which
/// clears every relationship of a node). To honour the obvious RESP3
/// contract we route through Cypher: `MATCH ()-[r]->() WHERE id(r) = $id
/// DELETE r` — the executor's write-pass then goes through the same
/// storage-layer path a full Cypher client would hit.
pub async fn rel_delete(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "REL.DELETE") {
        return e;
    }
    let id = match arg_int_required(args, 1, "REL.DELETE") {
        Ok(n) => n as u64,
        Err(e) => return e,
    };
    let query = format!("MATCH ()-[r]->() WHERE id(r) = {id} DELETE r RETURN count(r) AS deleted");
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;
    match out {
        Ok(Ok(rs)) => {
            let count = rs
                .rows
                .first()
                .and_then(|r| r.values.first())
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Resp3Value::Integer(count)
        }
        Ok(Err(e)) => err(format!("ERR REL.DELETE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

// --------------------------------------------------------------------------
// Helpers.
// --------------------------------------------------------------------------

fn node_to_resp3(id: u64, node: &nexus_core::storage::NodeRecord) -> Resp3Value {
    let entries: Vec<(Resp3Value, Resp3Value)> = vec![
        (Resp3Value::bulk("id"), Resp3Value::Integer(id as i64)),
        (
            Resp3Value::bulk("label_bits"),
            Resp3Value::Integer(node.label_bits as i64),
        ),
    ];
    Resp3Value::Map(entries)
}

fn json_value_to_resp3(v: &serde_json::Value) -> Resp3Value {
    match v {
        serde_json::Value::Null => Resp3Value::Null,
        serde_json::Value::Bool(b) => Resp3Value::Boolean(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Resp3Value::Integer)
            .or_else(|| n.as_f64().map(Resp3Value::Double))
            .unwrap_or_else(|| Resp3Value::bulk(n.to_string())),
        serde_json::Value::String(s) => Resp3Value::bulk(s.clone()),
        serde_json::Value::Array(arr) => {
            Resp3Value::Array(arr.iter().map(json_value_to_resp3).collect())
        }
        serde_json::Value::Object(obj) => Resp3Value::Map(
            obj.iter()
                .map(|(k, v)| (Resp3Value::bulk(k.clone()), json_value_to_resp3(v)))
                .collect(),
        ),
    }
}
