//! Typed-collection type system (phase6_opencypher-advanced-types §4).
//!
//! Cypher 25 / GQL lets a property-type constraint declare that a
//! property must be a `LIST<T>` of a homogeneous element type. This
//! module owns:
//!
//! - The typed-collection type grammar: `LIST<INTEGER>`, `LIST<STRING>`,
//!   `LIST<FLOAT>`, `LIST<BOOLEAN>`, `LIST<BYTES>`, and the fallback
//!   `LIST<ANY>` (which degrades to the pre-existing untyped list).
//! - A simple `parse_type(&str) -> Result<TypedList>` used by the
//!   constraint-DDL parser.
//! - An enforcement helper `validate_list(value, expected)` that the
//!   write-path constraint engine plugs in to reject non-matching
//!   writes with `ERR_CONSTRAINT_VIOLATED`.
//!
//! Storage-side inline encoding (spec §4.2) is deliberately out of
//! scope for this module — the wire value still rides through
//! `serde_json::Value` as a regular `Array`, and the constraint
//! surface is the single source of type truth.

use crate::{Error, Result};
use serde_json::Value;

/// Element types supported in a typed list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListElemType {
    /// Any element; equivalent to the untyped list used before this
    /// phase (empty-list case in §4.4).
    Any,
    Integer,
    Float,
    String,
    Boolean,
    Bytes,
}

impl ListElemType {
    pub fn name(&self) -> &'static str {
        match self {
            ListElemType::Any => "ANY",
            ListElemType::Integer => "INTEGER",
            ListElemType::Float => "FLOAT",
            ListElemType::String => "STRING",
            ListElemType::Boolean => "BOOLEAN",
            ListElemType::Bytes => "BYTES",
        }
    }

    /// Parse one of the canonical names (case-insensitive). Unknown
    /// names bubble up a `CypherSyntax` error that the constraint-DDL
    /// parser surfaces verbatim.
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_uppercase().as_str() {
            "ANY" => Ok(ListElemType::Any),
            "INTEGER" | "INT" => Ok(ListElemType::Integer),
            "FLOAT" | "DOUBLE" => Ok(ListElemType::Float),
            "STRING" | "TEXT" => Ok(ListElemType::String),
            "BOOLEAN" | "BOOL" => Ok(ListElemType::Boolean),
            "BYTES" => Ok(ListElemType::Bytes),
            other => Err(Error::CypherSyntax(format!(
                "unsupported LIST<T> element type: {other}"
            ))),
        }
    }
}

/// Parse a `LIST<ELEM>` type string produced by the constraint
/// parser. Whitespace is tolerated inside the angle brackets.
/// Returns the element type (the outer `LIST<>` wrapper is
/// implicit — callers already know they are parsing a list type).
pub fn parse_typed_list(input: &str) -> Result<ListElemType> {
    // Whitespace-tolerant: strip every ASCII space/tab before structural
    // matching so `LIST < INTEGER >` parses identically to `LIST<INTEGER>`.
    let compact: String = input
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .to_uppercase();
    if !compact.starts_with("LIST<") || !compact.ends_with('>') || compact.len() <= 6 {
        return Err(Error::CypherSyntax(format!(
            "expected `LIST<TYPE>`, got: {input:?}"
        )));
    }
    let inner = &compact[5..compact.len() - 1];
    ListElemType::parse(inner)
}

/// Validate that `value` is a `LIST<expected>`. Empty lists always
/// pass (matches spec §4.4). The function returns
/// `ERR_CONSTRAINT_VIOLATED` on mismatch.
pub fn validate_list(value: &Value, expected: ListElemType) -> Result<()> {
    let arr = match value {
        Value::Array(a) => a,
        Value::Null => return Ok(()), // null is allowed by default
        other => {
            return Err(Error::CypherExecution(format!(
                "ERR_CONSTRAINT_VIOLATED: expected LIST<{}>, got {}",
                expected.name(),
                json_type_name(other)
            )));
        }
    };
    if expected == ListElemType::Any || arr.is_empty() {
        return Ok(());
    }
    for (i, elem) in arr.iter().enumerate() {
        if !elem_matches(elem, expected) {
            return Err(Error::CypherExecution(format!(
                "ERR_CONSTRAINT_VIOLATED: expected LIST<{}>, element [{i}] is {} ({elem})",
                expected.name(),
                json_type_name(elem)
            )));
        }
    }
    Ok(())
}

fn elem_matches(v: &Value, expected: ListElemType) -> bool {
    match expected {
        ListElemType::Any => true,
        ListElemType::Integer => v.as_i64().is_some() && !v.is_f64(),
        ListElemType::Float => v.as_f64().is_some(),
        ListElemType::String => v.is_string(),
        ListElemType::Boolean => v.is_boolean(),
        ListElemType::Bytes => crate::executor::eval::bytes::is_bytes_value(v),
    }
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "NULL",
        Value::Bool(_) => "BOOLEAN",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "INTEGER"
            } else {
                "FLOAT"
            }
        }
        Value::String(_) => "STRING",
        Value::Array(_) => "LIST",
        Value::Object(_) => {
            if crate::executor::eval::bytes::is_bytes_value(v) {
                "BYTES"
            } else {
                "MAP"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_typed_list_canonical_forms() {
        assert_eq!(
            parse_typed_list("LIST<INTEGER>").unwrap(),
            ListElemType::Integer
        );
        assert_eq!(
            parse_typed_list("list<string>").unwrap(),
            ListElemType::String
        );
        assert_eq!(
            parse_typed_list(" LIST < BOOL > ").unwrap(),
            ListElemType::Boolean
        );
    }

    #[test]
    fn parse_typed_list_rejects_bad_shapes() {
        assert!(parse_typed_list("LIST INTEGER").is_err());
        assert!(parse_typed_list("LIST<>").is_err());
        assert!(parse_typed_list("LIST<UNKNOWN>").is_err());
    }

    #[test]
    fn validate_list_accepts_matching_integers() {
        assert!(validate_list(&json!([1, 2, 3]), ListElemType::Integer).is_ok());
    }

    #[test]
    fn validate_list_rejects_mixed_types() {
        let err = validate_list(&json!([1, "two"]), ListElemType::Integer).unwrap_err();
        assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));
    }

    #[test]
    fn validate_list_empty_always_ok() {
        for et in [
            ListElemType::Integer,
            ListElemType::String,
            ListElemType::Boolean,
            ListElemType::Float,
            ListElemType::Bytes,
        ] {
            assert!(
                validate_list(&json!([]), et).is_ok(),
                "empty list should pass for {}",
                et.name()
            );
        }
    }

    #[test]
    fn validate_list_any_passes_everything() {
        assert!(validate_list(&json!([1, "two", true]), ListElemType::Any).is_ok());
    }

    #[test]
    fn validate_list_null_passes() {
        assert!(validate_list(&Value::Null, ListElemType::Integer).is_ok());
    }

    #[test]
    fn validate_list_bytes_element_type() {
        let b = json!({"_bytes": "AAH/"});
        assert!(validate_list(&json!([b.clone(), b.clone()]), ListElemType::Bytes).is_ok());
        let err = validate_list(&json!([b, "not-bytes"]), ListElemType::Bytes).unwrap_err();
        assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));
    }

    #[test]
    fn validate_list_non_list_rejected() {
        let err = validate_list(&json!(42), ListElemType::Integer).unwrap_err();
        assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));
    }
}
