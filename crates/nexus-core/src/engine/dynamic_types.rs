//! Resolver for read-side dynamic relationship types.
//!
//! The parser encodes a `:$param` relationship-type position as the
//! sentinel `"$ident"` string (leading `$` is not a valid identifier
//! character, so the mapping is unambiguous — mirrors
//! [`crate::engine::dynamic_labels`] for node labels). This module
//! resolves those sentinels against a runtime parameter map and returns
//! the expanded list of type names ready for the catalog.
//!
//! Rejection surface (`MATCH ()-[r:$type]->()`):
//!
//! - `ERR_INVALID_RELATIONSHIP_TYPE` — the parameter is NULL, missing, an
//!   empty STRING, an empty LIST, a LIST containing a non-STRING element, a
//!   STRING outside the valid identifier character set, or a
//!   map/number/boolean. Raising a typed error (rather than silently
//!   matching nothing) also avoids the "empty type set = match every type"
//!   trap in the Expand operator and never registers a garbage type in the
//!   catalog.
//!
//! A dynamic type that resolves to a valid-but-unregistered name is NOT an
//! error — it simply matches no relationships, exactly like a static
//! `-[r:Nope]->`.

use crate::{Error, Result};
use serde_json::Value;

/// Resolve `:$param` relationship-type sentinels against the supplied
/// parameter map.
///
/// Static entries (no leading `$`) pass through unchanged. A `$ident` entry
/// looks up `params.get(ident)`:
///
/// - STRING → single type;
/// - LIST<STRING> → one type per element, in order (a `:A|B` union);
/// - anything else → `ERR_INVALID_RELATIONSHIP_TYPE`.
///
/// The function never mutates `types_with_sentinels` in place and never
/// panics on empty input (an anonymous `-[r]->` has no types).
pub fn resolve_types(
    types_with_sentinels: &[String],
    params: &std::collections::HashMap<String, Value>,
) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(types_with_sentinels.len());
    for raw in types_with_sentinels {
        if let Some(stripped) = raw.strip_prefix('$') {
            let v = params.get(stripped).unwrap_or(&Value::Null);
            resolve_one(stripped, v, &mut out)?;
        } else {
            out.push(raw.clone());
        }
    }
    Ok(out)
}

fn resolve_one(param_name: &str, v: &Value, out: &mut Vec<String>) -> Result<()> {
    match v {
        Value::Null => Err(invalid_type(&format!(
            "parameter ${param_name} resolved to NULL"
        ))),
        Value::String(s) => {
            validate_type_string(s).map_err(|e| {
                invalid_type(&format!("parameter ${param_name}: {e}", e = short_err(&e)))
            })?;
            out.push(s.clone());
            Ok(())
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Err(invalid_type(&format!(
                    "parameter ${param_name} resolved to an empty LIST"
                )));
            }
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::String(s) => {
                        validate_type_string(s).map_err(|e| {
                            invalid_type(&format!(
                                "parameter ${param_name}[{i}]: {e}",
                                e = short_err(&e)
                            ))
                        })?;
                        out.push(s.clone());
                    }
                    other => {
                        return Err(invalid_type(&format!(
                            "parameter ${param_name}[{i}] is {ty}, expected STRING",
                            ty = json_type_name(other)
                        )));
                    }
                }
            }
            Ok(())
        }
        other => Err(invalid_type(&format!(
            "parameter ${param_name} is {ty}, expected STRING or LIST<STRING>",
            ty = json_type_name(other)
        ))),
    }
}

/// Relationship types follow the same identifier rules as labels: a
/// non-empty run of ASCII alphanumerics/underscore, starting with a letter
/// or underscore. (The parser never emits a static type outside this set,
/// so a dynamic type must not either.)
fn validate_type_string(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(invalid_type("relationship type is the empty string"));
    }
    if let Some(first) = s.chars().next() {
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(invalid_type(&format!(
                "relationship type {s:?} must start with ASCII letter or underscore"
            )));
        }
    }
    for c in s.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Err(invalid_type(&format!(
                "relationship type {s:?} contains invalid character {c:?}"
            )));
        }
    }
    Ok(())
}

fn invalid_type(reason: &str) -> Error {
    Error::CypherExecution(format!("ERR_INVALID_RELATIONSHIP_TYPE: {reason}"))
}

fn short_err(e: &Error) -> String {
    e.to_string().replace("ERR_INVALID_RELATIONSHIP_TYPE: ", "")
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
    fn static_types_pass_through_unchanged() {
        let out = resolve_types(
            &["KNOWS".to_string(), "FOLLOWS".to_string()],
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(out, vec!["KNOWS".to_string(), "FOLLOWS".to_string()]);
    }

    #[test]
    fn single_string_param_expands_to_one_type() {
        let out = resolve_types(&["$type".to_string()], &map(&[("type", json!("KNOWS"))])).unwrap();
        assert_eq!(out, vec!["KNOWS".to_string()]);
    }

    #[test]
    fn list_param_expands_in_order() {
        let out = resolve_types(
            &["$types".to_string()],
            &map(&[("types", json!(["KNOWS", "FOLLOWS"]))]),
        )
        .unwrap();
        assert_eq!(out, vec!["KNOWS".to_string(), "FOLLOWS".to_string()]);
    }

    #[test]
    fn static_and_dynamic_types_mix() {
        let out = resolve_types(
            &["KNOWS".to_string(), "$extra".to_string()],
            &map(&[("extra", json!("FOLLOWS"))]),
        )
        .unwrap();
        assert_eq!(out, vec!["KNOWS".to_string(), "FOLLOWS".to_string()]);
    }

    #[test]
    fn null_parameter_rejected() {
        let err = resolve_types(&["$t".to_string()], &map(&[("t", Value::Null)])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }

    #[test]
    fn missing_parameter_treated_as_null() {
        let err = resolve_types(&["$missing".to_string()], &HashMap::new()).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }

    #[test]
    fn empty_string_rejected() {
        let err = resolve_types(&["$t".to_string()], &map(&[("t", json!(""))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }

    #[test]
    fn empty_list_rejected() {
        let err =
            resolve_types(&["$types".to_string()], &map(&[("types", json!([]))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }

    #[test]
    fn non_string_list_element_rejected() {
        let err = resolve_types(
            &["$types".to_string()],
            &map(&[("types", json!(["KNOWS", 42]))]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }

    #[test]
    fn number_param_rejected() {
        let err = resolve_types(&["$t".to_string()], &map(&[("t", json!(42))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }

    #[test]
    fn invalid_type_characters_rejected() {
        let err =
            resolve_types(&["$t".to_string()], &map(&[("t", json!("has space"))])).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"));
    }
}
