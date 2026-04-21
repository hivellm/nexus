//! `apoc.schema.*` — schema introspection (phase6 apoc §9).
//!
//! These procedures need a live engine context to read the catalog
//! and constraint manager, so they delegate through the executor's
//! existing `db.labels` / `db.propertyKeys` / `db.constraints` path
//! at the call site. The functions here expose the **shape** of each
//! procedure (column layout + empty fall-through) so dispatch can
//! flow through a uniform APOC path, and the engine-dispatcher
//! overrides those that need live catalog access.

use super::{ApocResult, bad_arg, not_found};
use crate::Result;
use serde_json::{Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.schema.assert",
        "apoc.schema.nodes",
        "apoc.schema.relationships",
        "apoc.schema.properties.distinctCount",
        "apoc.schema.node.constraintExists",
        "apoc.schema.node.indexExists",
        "apoc.schema.relationship.constraintExists",
        "apoc.schema.relationship.indexExists",
        "apoc.schema.stats",
        "apoc.schema.info",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "assert" => assert_schema(args),
        "nodes" => empty_shape(&["name", "label", "properties", "type"]),
        "relationships" => empty_shape(&["name", "type", "properties", "relationshipType"]),
        "properties.distinctCount" => empty_shape(&["label", "key", "count"]),
        "node.constraintExists" => exists_false(args, "apoc.schema.node.constraintExists"),
        "node.indexExists" => exists_false(args, "apoc.schema.node.indexExists"),
        "relationship.constraintExists" => {
            exists_false(args, "apoc.schema.relationship.constraintExists")
        }
        "relationship.indexExists" => exists_false(args, "apoc.schema.relationship.indexExists"),
        "stats" => stats_shape(),
        "info" => info_shape(),
        _ => Err(not_found(&format!("apoc.schema.{proc}"))),
    }
}

fn empty_shape(columns: &[&str]) -> Result<ApocResult> {
    Ok(ApocResult {
        columns: columns.iter().map(|s| s.to_string()).collect(),
        rows: Vec::new(),
    })
}

fn exists_false(args: Vec<Value>, proc: &str) -> Result<ApocResult> {
    // Shape: (label, properties) → BOOLEAN. Engine overrides this
    // with a live check. The stateless fallback here assumes the
    // catalog has no such item, which is correct for a freshly-
    // constructed engine and keeps the APOC surface answering
    // consistently.
    if args.is_empty() {
        return Err(bad_arg(proc, "expected (label, properties) arguments"));
    }
    Ok(ApocResult::scalar(Value::Bool(false)))
}

fn assert_schema(args: Vec<Value>) -> Result<ApocResult> {
    // apoc.schema.assert(indexes, constraints, dropExisting=false).
    // Full DDL execution needs engine context; at this layer we
    // round-trip the requested definitions as the result set so
    // the engine can apply them and report what changed. The
    // returned rows mirror Neo4j's shape: (label, key, keys, unique,
    // action).
    let indexes = args.first().cloned().unwrap_or(Value::Null);
    let constraints = args.get(1).cloned().unwrap_or(Value::Null);
    let mut rows: Vec<Vec<Value>> = Vec::new();
    append_assert_rows(&indexes, false, &mut rows);
    append_assert_rows(&constraints, true, &mut rows);
    Ok(ApocResult {
        columns: vec![
            "label".to_string(),
            "key".to_string(),
            "keys".to_string(),
            "unique".to_string(),
            "action".to_string(),
        ],
        rows,
    })
}

fn append_assert_rows(v: &Value, unique: bool, out: &mut Vec<Vec<Value>>) {
    if let Value::Object(m) = v {
        for (label, props) in m {
            if let Value::Array(prop_lists) = props {
                for prop_list in prop_lists {
                    if let Value::Array(keys) = prop_list {
                        let first_key = keys
                            .first()
                            .and_then(|v| v.as_str())
                            .map(|s| Value::String(s.to_string()))
                            .unwrap_or(Value::Null);
                        out.push(vec![
                            Value::String(label.clone()),
                            first_key,
                            Value::Array(keys.clone()),
                            Value::Bool(unique),
                            Value::String("CREATED".to_string()),
                        ]);
                    }
                }
            }
        }
    }
}

fn stats_shape() -> Result<ApocResult> {
    Ok(ApocResult {
        columns: vec!["value".to_string()],
        rows: vec![vec![json!({
            "labels": 0,
            "relTypes": 0,
            "propertyKeys": 0,
            "nodes": 0,
            "relationships": 0
        })]],
    })
}

fn info_shape() -> Result<ApocResult> {
    Ok(ApocResult {
        columns: vec!["value".to_string()],
        rows: vec![vec![json!({
            "name": "nexus-apoc",
            "version": env!("CARGO_PKG_VERSION"),
            "procedures": super::list_procedures().len() as i64,
        })]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_returns_empty_with_fixed_columns() {
        let r = dispatch("nodes", vec![]).unwrap();
        assert_eq!(r.columns, vec!["name", "label", "properties", "type"]);
        assert!(r.rows.is_empty());
    }

    #[test]
    fn stats_returns_skeleton_map() {
        let r = dispatch("stats", vec![]).unwrap();
        assert_eq!(r.columns, vec!["value"]);
        assert_eq!(r.rows.len(), 1);
        assert!(r.rows[0][0].is_object());
    }

    #[test]
    fn info_reports_procedure_count() {
        let r = dispatch("info", vec![]).unwrap();
        let obj = r.rows[0][0].as_object().unwrap();
        let n = obj.get("procedures").and_then(|v| v.as_i64()).unwrap();
        assert!(n >= 90, "expected ≥90 APOC procedures, got {n}");
    }

    #[test]
    fn assert_flattens_index_and_constraint_inputs() {
        let r = dispatch(
            "assert",
            vec![
                json!({"Person": [["name"]]}),
                json!({"Person": [["email"]]}),
            ],
        )
        .unwrap();
        assert_eq!(r.columns.len(), 5);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0][0], json!("Person"));
        assert_eq!(r.rows[0][3], json!(false)); // index → not unique
        assert_eq!(r.rows[1][3], json!(true)); // constraint → unique
    }

    #[test]
    fn exists_returns_false_by_default() {
        let out = dispatch("node.indexExists", vec![json!("Person"), json!(["name"])]).unwrap();
        assert_eq!(out.rows[0][0], json!(false));
    }
}
