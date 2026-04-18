//! Schema, index, database, and introspection RESP3 commands.
//!
//! Most handlers in this module route through Cypher rather than reaching
//! into the lower-level catalog APIs — the planner already knows how to
//! enumerate labels, relationship types, and property keys, and going
//! through Cypher means RESP3 stays in lock-step with the HTTP/REST
//! surface without duplicating enumeration logic.

use crate::protocol::resp3::parser::Resp3Value;

use super::{SessionState, arg_str_required, err, expect_arity, expect_arity_range};

// ==========================================================================
// INDEX.*
// ==========================================================================

/// `INDEX.CREATE <label> <property> [UNIQUE]` — creates a B-tree index on
/// the `label.property` tuple. Routes through Cypher so the same index
/// lifecycle as the REST API is used.
pub async fn index_create(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_range(args, 3, 4, "INDEX.CREATE") {
        return e;
    }
    let label = match arg_str_required(args, 1, "INDEX.CREATE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let property = match arg_str_required(args, 2, "INDEX.CREATE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let unique = args
        .get(3)
        .and_then(Resp3Value::as_str)
        .map(str::to_ascii_uppercase)
        .as_deref()
        == Some("UNIQUE");

    // Validate the identifiers — the values are going to be substituted
    // into a Cypher query, so we reject anything that isn't a plain
    // identifier. This is belt-and-braces: the parser will also reject
    // weird identifiers, but a clear early error beats a Cypher syntax
    // error bubbling up through the RESP3 response.
    if !is_simple_identifier(&label) {
        return err("ERR INDEX.CREATE: label must be [A-Za-z_][A-Za-z0-9_]*");
    }
    if !is_simple_identifier(&property) {
        return err("ERR INDEX.CREATE: property must be [A-Za-z_][A-Za-z0-9_]*");
    }

    let query = if unique {
        format!("CREATE CONSTRAINT FOR (n:{label}) REQUIRE n.{property} IS UNIQUE")
    } else {
        format!("CREATE INDEX FOR (n:{label}) ON (n.{property})")
    };
    run_cypher_for_ok(state, query, "INDEX.CREATE").await
}

/// `INDEX.DROP <label> <property>` — drops the matching index.
pub async fn index_drop(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 3, "INDEX.DROP") {
        return e;
    }
    let label = match arg_str_required(args, 1, "INDEX.DROP") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let property = match arg_str_required(args, 2, "INDEX.DROP") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    if !is_simple_identifier(&label) {
        return err("ERR INDEX.DROP: label must be [A-Za-z_][A-Za-z0-9_]*");
    }
    if !is_simple_identifier(&property) {
        return err("ERR INDEX.DROP: property must be [A-Za-z_][A-Za-z0-9_]*");
    }
    let query = format!("DROP INDEX FOR (n:{label}) ON (n.{property})");
    run_cypher_for_ok(state, query, "INDEX.DROP").await
}

/// `INDEX.LIST` — returns the array of registered indexes as Maps.
pub async fn index_list(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "INDEX.LIST") {
        return e;
    }
    run_cypher_as_array(state, "SHOW INDEXES".to_string(), "INDEX.LIST").await
}

// ==========================================================================
// DB.*
// ==========================================================================

/// `DB.LIST` -> Array of database names (BulkString).
pub async fn db_list(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "DB.LIST") {
        return e;
    }
    let dbm = state.server.database_manager.clone();
    let names = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.list_databases()
            .into_iter()
            .map(|info| info.name)
            .collect::<Vec<_>>()
    })
    .await;
    match names {
        Ok(list) => Resp3Value::Array(list.into_iter().map(Resp3Value::bulk).collect()),
        Err(_) => err("ERR internal join error"),
    }
}

/// `DB.CREATE <name>` -> `+OK` or `-ERR`.
pub async fn db_create(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "DB.CREATE") {
        return e;
    }
    let name = match arg_str_required(args, 1, "DB.CREATE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let dbm = state.server.database_manager.clone();
    let res = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.create_database(&name).map(|_| ())
    })
    .await;
    match res {
        Ok(Ok(())) => Resp3Value::SimpleString("OK".into()),
        Ok(Err(e)) => err(format!("ERR DB.CREATE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `DB.DROP <name>` -> `+OK` or `-ERR`.
pub async fn db_drop(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "DB.DROP") {
        return e;
    }
    let name = match arg_str_required(args, 1, "DB.DROP") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let dbm = state.server.database_manager.clone();
    let res = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.drop_database(&name, false).map(|_| ())
    })
    .await;
    match res {
        Ok(Ok(())) => Resp3Value::SimpleString("OK".into()),
        Ok(Err(e)) => err(format!("ERR DB.DROP failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `DB.USE <name>` — validates the database exists and returns `+OK`.
///
/// Per-session database selection is the REST session layer's job and is
/// not plumbed through RESP3 in this release (tracked for a follow-up
/// ADR). The command is still useful for operator sanity-checking: it
/// returns `+OK` iff the named database exists today.
pub async fn db_use(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "DB.USE") {
        return e;
    }
    let name = match arg_str_required(args, 1, "DB.USE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let dbm = state.server.database_manager.clone();
    let exists = tokio::task::spawn_blocking(move || {
        let mgr = dbm.read();
        mgr.exists(&name)
    })
    .await
    .unwrap_or(false);
    if exists {
        Resp3Value::SimpleString("OK".into())
    } else {
        err("ERR DB.USE: database not found")
    }
}

// ==========================================================================
// LABELS / REL_TYPES / PROPERTY_KEYS
// ==========================================================================

/// `LABELS` -> Array of distinct node labels present in the graph.
pub async fn labels(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "LABELS") {
        return e;
    }
    run_cypher_flatten_strings(
        state,
        "MATCH (n) UNWIND labels(n) AS l RETURN DISTINCT l".to_string(),
        "LABELS",
    )
    .await
}

/// `REL_TYPES` -> Array of distinct relationship types in the graph.
pub async fn rel_types(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "REL_TYPES") {
        return e;
    }
    run_cypher_flatten_strings(
        state,
        "MATCH ()-[r]->() RETURN DISTINCT type(r) AS t".to_string(),
        "REL_TYPES",
    )
    .await
}

/// `PROPERTY_KEYS` -> Array of distinct property keys observed on nodes.
pub async fn property_keys(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "PROPERTY_KEYS") {
        return e;
    }
    run_cypher_flatten_strings(
        state,
        "MATCH (n) UNWIND keys(n) AS k RETURN DISTINCT k".to_string(),
        "PROPERTY_KEYS",
    )
    .await
}

// ==========================================================================
// STATS / HEALTH
// ==========================================================================

/// `STATS` -> Map of engine counters.
pub async fn stats(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "STATS") {
        return e;
    }
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.stats()
    })
    .await;
    match out {
        Ok(Ok(s)) => Resp3Value::Map(vec![
            (
                Resp3Value::bulk("nodes"),
                Resp3Value::Integer(s.nodes as i64),
            ),
            (
                Resp3Value::bulk("relationships"),
                Resp3Value::Integer(s.relationships as i64),
            ),
            (
                Resp3Value::bulk("labels"),
                Resp3Value::Integer(s.labels as i64),
            ),
            (
                Resp3Value::bulk("rel_types"),
                Resp3Value::Integer(s.rel_types as i64),
            ),
            (
                Resp3Value::bulk("page_cache_hits"),
                Resp3Value::Integer(s.page_cache_hits as i64),
            ),
        ]),
        Ok(Err(e)) => err(format!("ERR STATS failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `HEALTH` -> `+OK` if the engine reports healthy or degraded, `-ERR`
/// otherwise (matching what the REST `/health` endpoint signals).
pub async fn health(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "HEALTH") {
        return e;
    }
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        guard.health_check()
    })
    .await;
    match out {
        Ok(Ok(h)) => match h.overall {
            nexus_core::HealthState::Healthy | nexus_core::HealthState::Degraded => {
                Resp3Value::SimpleString("OK".into())
            }
            nexus_core::HealthState::Unhealthy => err("ERR engine unhealthy"),
        },
        Ok(Err(e)) => err(format!("ERR HEALTH failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

// ==========================================================================
// Internal helpers.
// ==========================================================================

/// Run `query` and collapse a Cypher result set of single-column string
/// rows into a flat `Resp3Value::Array` of BulkStrings.
async fn run_cypher_flatten_strings(state: &SessionState, query: String, cmd: &str) -> Resp3Value {
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;
    match out {
        Ok(Ok(rs)) => Resp3Value::Array(
            rs.rows
                .iter()
                .filter_map(|row| row.values.first())
                .filter_map(|v| v.as_str().map(str::to_string))
                .map(Resp3Value::bulk)
                .collect(),
        ),
        Ok(Err(e)) => err(format!("ERR {cmd} failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// Run `query` and produce a RESP3 Array of Maps — each Cypher result row
/// becomes a Map using the result-set column names as keys.
async fn run_cypher_as_array(state: &SessionState, query: String, cmd: &str) -> Resp3Value {
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;
    match out {
        Ok(Ok(rs)) => {
            let cols = rs.columns.clone();
            let rows: Vec<Resp3Value> = rs
                .rows
                .into_iter()
                .map(|row| {
                    let entries: Vec<(Resp3Value, Resp3Value)> = cols
                        .iter()
                        .zip(row.values)
                        .map(|(c, v)| (Resp3Value::bulk(c.clone()), json_value_to_resp3(&v)))
                        .collect();
                    Resp3Value::Map(entries)
                })
                .collect();
            Resp3Value::Array(rows)
        }
        Ok(Err(e)) => err(format!("ERR {cmd} failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// Run `query` purely for side effects, returning `+OK` on success.
async fn run_cypher_for_ok(state: &SessionState, query: String, cmd: &str) -> Resp3Value {
    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;
    match out {
        Ok(Ok(_)) => Resp3Value::SimpleString("OK".into()),
        Ok(Err(e)) => err(format!("ERR {cmd} failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
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

/// True if `s` matches `[A-Za-z_][A-Za-z0-9_]*`. Used to reject Cypher
/// fragments smuggled in through identifier arguments (cheap belt-and-
/// braces on top of the engine's own parser).
fn is_simple_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_identifier_accepts_valid_names() {
        assert!(is_simple_identifier("Person"));
        assert!(is_simple_identifier("snake_case"));
        assert!(is_simple_identifier("_private"));
        assert!(is_simple_identifier("Label123"));
    }

    #[test]
    fn simple_identifier_rejects_injections() {
        assert!(!is_simple_identifier(""));
        assert!(!is_simple_identifier("Person) MATCH"));
        assert!(!is_simple_identifier("a b"));
        assert!(!is_simple_identifier("9starts_with_digit"));
        assert!(!is_simple_identifier("has-dash"));
        assert!(!is_simple_identifier("has'quote"));
    }
}
