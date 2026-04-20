//! KNN and bulk-ingest RESP3 commands.

use crate::protocol::resp3::parser::Resp3Value;

use super::{
    SessionState, arg_int_required, arg_str_required, err, expect_arity, expect_arity_min,
    expect_arity_range,
};

/// `KNN.SEARCH <label> <vector> <k>`
///
/// `<vector>` is either a comma-separated list of doubles
/// (`"0.1,0.2,0.3"`) or a BulkString of raw little-endian f32s. `k` is the
/// number of neighbours to return. Returns an Array of `{id, score}` Maps.
pub async fn knn_search(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_range(args, 4, 6, "KNN.SEARCH") {
        return e;
    }
    let label = match arg_str_required(args, 1, "KNN.SEARCH") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let vector = match parse_vector_arg(&args[2]) {
        Ok(v) => v,
        Err(e) => return err(format!("ERR KNN.SEARCH: {e}")),
    };
    let k = match arg_int_required(args, 3, "KNN.SEARCH") {
        Ok(n) if n > 0 => n as usize,
        Ok(_) => return err("ERR KNN.SEARCH: k must be > 0"),
        Err(e) => return e,
    };

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let guard = engine.blocking_read();
        guard.knn_search(&label, &vector, k)
    })
    .await;

    match out {
        Ok(Ok(results)) => Resp3Value::Array(
            results
                .into_iter()
                .map(|(id, score)| {
                    Resp3Value::Map(vec![
                        (Resp3Value::bulk("id"), Resp3Value::Integer(id as i64)),
                        (Resp3Value::bulk("score"), Resp3Value::Double(score as f64)),
                    ])
                })
                .collect(),
        ),
        Ok(Err(e)) => err(format!("ERR KNN.SEARCH failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `KNN.TRAVERSE <seeds-csv> <depth>` — breadth-first expansion from the
/// seed node ids out to `depth` hops. Returns the deduplicated set of
/// reachable node ids (the seeds themselves included).
///
/// Implemented via a generated Cypher query because the core `Engine` does
/// not expose a direct traversal API yet. We use a variable-length path so
/// the planner's own expansion logic is reused:
///
/// ```text
/// MATCH (s)-[*0..<depth>]->(n)
/// WHERE id(s) IN [<seeds>]
/// RETURN DISTINCT id(n) AS id
/// ```
///
/// The `*0..depth` allows depth = 0 to still return the seeds themselves,
/// which matches operator expectations when `redis-cli KNN.TRAVERSE <id> 0`
/// is used as "is this node live?".
pub async fn knn_traverse(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_min(args, 3, "KNN.TRAVERSE") {
        return e;
    }
    let seeds_raw = match arg_str_required(args, 1, "KNN.TRAVERSE") {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    let depth = match arg_int_required(args, 2, "KNN.TRAVERSE") {
        Ok(n) if (0..=32).contains(&n) => n,
        Ok(_) => return err("ERR KNN.TRAVERSE: depth must be between 0 and 32"),
        Err(e) => return e,
    };

    // Parse seeds.
    let seed_ids: Result<Vec<i64>, _> = seeds_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>())
        .collect();
    let seed_ids = match seed_ids {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => return err("ERR KNN.TRAVERSE: at least one seed id required"),
        Err(_) => return err("ERR KNN.TRAVERSE: seeds must be comma-separated integers"),
    };

    // Build the Cypher query. Hand-rolling the `[...]` list is safe because
    // every element of seed_ids is already an `i64`, so injection is not
    // possible — the only attacker-controlled bytes in the generated query
    // are the decimal digits of integers we parsed ourselves.
    let seed_list = seed_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        "MATCH (s)-[*0..{depth}]->(n) WHERE id(s) IN [{seed_list}] RETURN DISTINCT id(n) AS id"
    );

    let engine = state.server.engine.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut guard = engine.blocking_write();
        guard.execute_cypher(&query)
    })
    .await;

    match out {
        Ok(Ok(rs)) => {
            let ids: Vec<Resp3Value> = rs
                .rows
                .iter()
                .filter_map(|row| row.values.first().and_then(|v| v.as_i64()))
                .map(Resp3Value::Integer)
                .collect();
            Resp3Value::Array(ids)
        }
        Ok(Err(e)) => err(format!("ERR KNN.TRAVERSE failed: {e}")),
        Err(_) => err("ERR internal join error"),
    }
}

/// `INGEST.NODES <ndjson-bulk>` — newline-delimited JSON, one node per line.
/// Each line is `{"labels": [...], "properties": {...}}`. Returns a Map
/// `{created: Int, errors: Int}`.
pub async fn ingest_nodes(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "INGEST.NODES") {
        return e;
    }
    let payload = match args[1].as_str() {
        Some(s) => s.to_string(),
        None => return err("ERR INGEST.NODES: argument must be NDJSON text"),
    };
    let engine = state.server.engine.clone();
    let (created, errors) =
        tokio::task::spawn_blocking(move || ingest_nodes_sync(&engine, &payload))
            .await
            .unwrap_or((0, 1));
    Resp3Value::Map(vec![
        (Resp3Value::bulk("created"), Resp3Value::Integer(created)),
        (Resp3Value::bulk("errors"), Resp3Value::Integer(errors)),
    ])
}

/// `INGEST.RELS <ndjson-bulk>` — one relationship per line, shape
/// `{"src": id, "dst": id, "type": "TYPE", "properties": {...}}`.
pub async fn ingest_rels(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 2, "INGEST.RELS") {
        return e;
    }
    let payload = match args[1].as_str() {
        Some(s) => s.to_string(),
        None => return err("ERR INGEST.RELS: argument must be NDJSON text"),
    };
    let engine = state.server.engine.clone();
    let (created, errors) =
        tokio::task::spawn_blocking(move || ingest_rels_sync(&engine, &payload))
            .await
            .unwrap_or((0, 1));
    Resp3Value::Map(vec![
        (Resp3Value::bulk("created"), Resp3Value::Integer(created)),
        (Resp3Value::bulk("errors"), Resp3Value::Integer(errors)),
    ])
}

// --------------------------------------------------------------------------
// Helpers.
// --------------------------------------------------------------------------

fn parse_vector_arg(arg: &Resp3Value) -> Result<Vec<f32>, String> {
    // Two accepted shapes:
    //   1. Raw f32 LE bytes inside a BulkString. Length must be a multiple
    //      of 4.
    //   2. Comma-separated decimal text.
    if let Some(bytes) = arg.as_bytes() {
        // Heuristic: if every byte is ASCII printable AND contains a comma,
        // treat as text. Otherwise assume raw f32.
        let looks_like_text = bytes
            .iter()
            .all(|b| b.is_ascii_graphic() || *b == b' ' || *b == b',' || *b == b'.' || *b == b'-')
            && bytes.contains(&b',');
        if looks_like_text {
            if let Ok(s) = std::str::from_utf8(bytes) {
                return parse_vector_text(s);
            }
        }
        if bytes.len() % 4 != 0 {
            return Err(format!(
                "raw f32 vector must be a multiple of 4 bytes (got {} bytes)",
                bytes.len()
            ));
        }
        let mut out = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            out.push(f32::from_le_bytes(arr));
        }
        return Ok(out);
    }
    if let Some(s) = arg.as_str() {
        return parse_vector_text(s);
    }
    Err("vector argument must be a BulkString or comma-separated text".into())
}

fn parse_vector_text(s: &str) -> Result<Vec<f32>, String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|tok| {
            tok.parse::<f32>()
                .map_err(|_| format!("invalid float in vector: {tok}"))
        })
        .collect()
}

fn ingest_nodes_sync(
    engine: &std::sync::Arc<tokio::sync::RwLock<nexus_core::Engine>>,
    payload: &str,
) -> (i64, i64) {
    let mut created = 0i64;
    let mut errors = 0i64;
    let mut guard = engine.blocking_write();
    for line in payload.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => {
                let labels = v
                    .get("labels")
                    .and_then(|l| l.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let props = v
                    .get("properties")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                match guard.create_node(labels, props) {
                    Ok(_) => created += 1,
                    Err(_) => errors += 1,
                }
            }
            Err(_) => errors += 1,
        }
    }
    (created, errors)
}

fn ingest_rels_sync(
    engine: &std::sync::Arc<tokio::sync::RwLock<nexus_core::Engine>>,
    payload: &str,
) -> (i64, i64) {
    let mut created = 0i64;
    let mut errors = 0i64;
    let mut guard = engine.blocking_write();
    for line in payload.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        let src = v.get("src").and_then(|x| x.as_u64());
        let dst = v.get("dst").and_then(|x| x.as_u64());
        let ty = v.get("type").and_then(|x| x.as_str()).map(str::to_string);
        let props = v
            .get("properties")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        match (src, dst, ty) {
            (Some(src), Some(dst), Some(ty)) => {
                match guard.create_relationship(src, dst, ty, props) {
                    Ok(_) => created += 1,
                    Err(_) => errors += 1,
                }
            }
            _ => errors += 1,
        }
    }
    (created, errors)
}

// Focused unit tests for the vector-arg parser; INGEST.* and KNN.* flows
// are exercised end-to-end by the integration suite.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_text_parses_comma_separated() {
        let v = parse_vector_arg(&Resp3Value::bulk("1.0,2.5,-3.0")).unwrap();
        assert_eq!(v, vec![1.0, 2.5, -3.0]);
    }

    #[test]
    fn vector_raw_f32_parses() {
        let bytes: Vec<u8> = [1.0f32, 2.0, 3.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let v = parse_vector_arg(&Resp3Value::BulkString(bytes)).unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn vector_rejects_non_multiple_of_4() {
        let bytes = vec![1u8, 2, 3];
        assert!(parse_vector_arg(&Resp3Value::BulkString(bytes)).is_err());
    }
}
