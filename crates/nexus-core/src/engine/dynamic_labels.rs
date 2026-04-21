//! Resolver for write-side dynamic labels.
//!
//! The parser encodes a `:$param` label position as the sentinel
//! `"$ident"` string (leading `$` is not a valid identifier character,
//! so the mapping is unambiguous). This module resolves those
//! sentinels against a runtime parameter map and returns the expanded
//! list of label names ready for the catalog.
//!
//! Rejection surface (shared by `CREATE (n:$x)`, `SET n:$x`,
//! `REMOVE n:$x`):
//!
//! - `ERR_INVALID_LABEL` — the parameter is NULL, an empty STRING, an
//!   empty LIST, a LIST containing a non-STRING element, a STRING
//!   outside the valid label character set, or a map/number/boolean.
//!
//! The 64-label bitmap cap is enforced by the catalog on insert, not
//! by this resolver.

use crate::{Error, Result};
use serde_json::Value;

/// Resolve `:$param` label sentinels against the supplied parameter map.
///
/// Static entries (no leading `$`) pass through unchanged. A `$ident`
/// entry looks up `params.get(ident)`:
///
/// - STRING → single label;
/// - LIST<STRING> → one label per element, in order;
/// - anything else → `ERR_INVALID_LABEL`.
///
/// The function never panics on empty input (read: an empty label
/// list, valid for anonymous patterns) and never mutates
/// `static_labels` in place.
pub fn resolve_labels(
    labels_with_sentinels: &[String],
    params: &std::collections::HashMap<String, Value>,
) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(labels_with_sentinels.len());
    for raw in labels_with_sentinels {
        if let Some(stripped) = raw.strip_prefix('$') {
            let v = params.get(stripped).unwrap_or(&Value::Null);
            resolve_one(stripped, v, &mut out)?;
        } else {
            validate_label_string(raw)?;
            out.push(raw.clone());
        }
    }
    Ok(out)
}

fn resolve_one(param_name: &str, v: &Value, out: &mut Vec<String>) -> Result<()> {
    match v {
        Value::Null => Err(invalid_label(&format!(
            "parameter ${param_name} resolved to NULL"
        ))),
        Value::String(s) => {
            validate_label_string(s).map_err(|e| {
                invalid_label(&format!("parameter ${param_name}: {e}", e = short_err(&e)))
            })?;
            out.push(s.clone());
            Ok(())
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Err(invalid_label(&format!(
                    "parameter ${param_name} resolved to an empty LIST"
                )));
            }
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::String(s) => {
                        validate_label_string(s).map_err(|e| {
                            invalid_label(&format!(
                                "parameter ${param_name}[{i}]: {e}",
                                e = short_err(&e)
                            ))
                        })?;
                        out.push(s.clone());
                    }
                    other => {
                        return Err(invalid_label(&format!(
                            "parameter ${param_name}[{i}] is {ty}, expected STRING",
                            ty = json_type_name(other)
                        )));
                    }
                }
            }
            Ok(())
        }
        other => Err(invalid_label(&format!(
            "parameter ${param_name} is {ty}, expected STRING or LIST<STRING>",
            ty = json_type_name(other)
        ))),
    }
}

fn validate_label_string(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(invalid_label("label is the empty string"));
    }
    if let Some(first) = s.chars().next() {
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(invalid_label(&format!(
                "label {s:?} must start with ASCII letter or underscore"
            )));
        }
    }
    for c in s.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Err(invalid_label(&format!(
                "label {s:?} contains invalid character {c:?}"
            )));
        }
    }
    Ok(())
}

fn invalid_label(reason: &str) -> Error {
    Error::CypherExecution(format!("ERR_INVALID_LABEL: {reason}"))
}

fn short_err(e: &Error) -> String {
    e.to_string().replace("ERR_INVALID_LABEL: ", "")
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "NULL",
        Value::Bool(_) => "BOOLEAN",
        Value::Number(_) => "NUMBER",
        Value::String(_) => "STRING",
        Value::Array(_) => "LIST",
        Value::Object(_) => "MAP",
    }
}

/// Fast-path predicate: does any entry in `labels_with_sentinels`
/// require parameter resolution? Used by write-path operators to
/// skip the resolver entirely on a fully-static label set.
pub fn contains_dynamic(labels_with_sentinels: &[String]) -> bool {
    labels_with_sentinels.iter().any(|l| l.starts_with('$'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn map(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn static_labels_pass_through_unchanged() {
        let out =
            resolve_labels(&["Person".to_string(), "User".to_string()], &HashMap::new()).unwrap();
        assert_eq!(out, vec!["Person".to_string(), "User".to_string()]);
    }

    #[test]
    fn single_string_param_expands_to_one_label() {
        let out =
            resolve_labels(&["$label".to_string()], &map(&[("label", json!("Person"))])).unwrap();
        assert_eq!(out, vec!["Person".to_string()]);
    }

    #[test]
    fn list_param_expands_in_order() {
        let out = resolve_labels(
            &["$labels".to_string()],
            &map(&[("labels", json!(["Person", "User"]))]),
        )
        .unwrap();
        assert_eq!(out, vec!["Person".to_string(), "User".to_string()]);
    }

    #[test]
    fn static_and_dynamic_labels_mix() {
        let out = resolve_labels(
            &["Base".to_string(), "$role".to_string()],
            &map(&[("role", json!("Admin"))]),
        )
        .unwrap();
        assert_eq!(out, vec!["Base".to_string(), "Admin".to_string()]);
    }

    #[test]
    fn null_parameter_rejected() {
        let err = resolve_labels(&["$l".to_string()], &map(&[("l", Value::Null)])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn missing_parameter_treated_as_null() {
        let err = resolve_labels(&["$missing".to_string()], &HashMap::new()).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn empty_string_rejected() {
        let err = resolve_labels(&["$l".to_string()], &map(&[("l", json!(""))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn empty_list_rejected() {
        let err =
            resolve_labels(&["$labels".to_string()], &map(&[("labels", json!([]))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn non_string_list_element_rejected() {
        let err = resolve_labels(
            &["$labels".to_string()],
            &map(&[("labels", json!(["Person", 42]))]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn number_param_rejected() {
        let err = resolve_labels(&["$l".to_string()], &map(&[("l", json!(42))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn invalid_label_characters_rejected() {
        let err =
            resolve_labels(&["$l".to_string()], &map(&[("l", json!("has space"))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_LABEL"));
    }

    #[test]
    fn contains_dynamic_fast_path() {
        assert!(!contains_dynamic(&[
            "Person".to_string(),
            "User".to_string()
        ]));
        assert!(contains_dynamic(&[
            "Person".to_string(),
            "$role".to_string()
        ]));
    }
}
