//! Extended constraint engine (phase6_opencypher-constraint-enforcement).
//!
//! Nexus's legacy constraint catalogue (`crate::catalog::constraints`)
//! persists UNIQUE and EXISTS constraints in LMDB and already
//! enforces them on `CREATE`. This module sits alongside the
//! catalogue and adds the constraint kinds that the original
//! schema does not model:
//!
//! - **NODE KEY** — composite uniqueness + per-component NOT NULL.
//!   Backed by the `crate::index::composite_btree::CompositeBtreeRegistry`
//!   shipped in phase6_opencypher-advanced-types §3.
//! - **Relationship NOT NULL** — same shape as the node EXISTS
//!   constraint but scoped to relationship types.
//! - **Property-type** — `REQUIRE n.p IS :: INTEGER` (and FLOAT /
//!   STRING / BOOLEAN / LIST / BYTES). For typed lists the check
//!   delegates to `engine::typed_collections::validate_list`, so
//!   the two surfaces stay in lock-step.
//!
//! Kept in-memory rather than persisted to LMDB for this release —
//! the on-disk migration is a follow-up so the LMDB schema change
//! can be reviewed independently of the enforcement logic. Engines
//! re-register their constraints at startup via the programmatic
//! API (`Engine::add_node_key_constraint` etc.).

use crate::{Error, Result};
use serde_json::Value;

/// Scalar type tokens accepted by `REQUIRE n.p IS :: <TYPE>`.
/// Matches the `typed_collections::ListElemType` code range plus an
/// `Any` escape hatch so "this property must be a list" can skip
/// element-type discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Integer,
    Float,
    String,
    Boolean,
    Bytes,
    List,
    Map,
}

impl ScalarType {
    pub fn name(&self) -> &'static str {
        match self {
            ScalarType::Integer => "INTEGER",
            ScalarType::Float => "FLOAT",
            ScalarType::String => "STRING",
            ScalarType::Boolean => "BOOLEAN",
            ScalarType::Bytes => "BYTES",
            ScalarType::List => "LIST",
            ScalarType::Map => "MAP",
        }
    }

    /// Canonical name parser (case-insensitive, `INT` alias for
    /// `INTEGER`, `BOOL` for `BOOLEAN`).
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_uppercase().as_str() {
            "INTEGER" | "INT" => Ok(ScalarType::Integer),
            "FLOAT" | "DOUBLE" => Ok(ScalarType::Float),
            "STRING" | "TEXT" => Ok(ScalarType::String),
            "BOOLEAN" | "BOOL" => Ok(ScalarType::Boolean),
            "BYTES" => Ok(ScalarType::Bytes),
            "LIST" => Ok(ScalarType::List),
            "MAP" => Ok(ScalarType::Map),
            other => Err(Error::CypherSyntax(format!(
                "unsupported property-type `{other}`"
            ))),
        }
    }

    /// True iff `v` matches this scalar type. Follows Neo4j's strict
    /// rule: INTEGER does not match FLOAT and vice-versa.
    pub fn accepts(&self, v: &Value) -> bool {
        match self {
            ScalarType::Integer => match v {
                Value::Number(n) => n.is_i64() || n.is_u64(),
                _ => false,
            },
            ScalarType::Float => match v {
                Value::Number(n) => n.is_f64(),
                _ => false,
            },
            ScalarType::String => v.is_string(),
            ScalarType::Boolean => v.is_boolean(),
            ScalarType::Bytes => crate::executor::eval::bytes::is_bytes_value(v),
            ScalarType::List => v.is_array(),
            ScalarType::Map => {
                // A BYTES wire-shape decodes as MAP in raw JSON — exclude
                // it so `IS :: MAP` doesn't swallow byte arrays.
                v.is_object() && !crate::executor::eval::bytes::is_bytes_value(v)
            }
        }
    }
}

/// NODE KEY constraint: the tuple `(p1, p2, ...)` is globally unique
/// across all nodes carrying the label, and every component is
/// non-null. Uses a composite B-tree with the `unique` flag set.
#[derive(Debug, Clone)]
pub struct NodeKeyConstraint {
    pub name: Option<String>,
    pub label_id: u32,
    pub property_keys: Vec<String>,
}

/// Relationship NOT NULL constraint — forbids NULL / missing on the
/// named property for every relationship of the given type.
#[derive(Debug, Clone)]
pub struct RelNotNullConstraint {
    pub name: Option<String>,
    pub rel_type_id: u32,
    pub property_key: String,
}

/// Property-type constraint — property value must match `ty` when set.
#[derive(Debug, Clone)]
pub struct PropertyTypeConstraint {
    pub name: Option<String>,
    /// Which label/type this applies to. `None` on the label side
    /// means it applies to every node (unlabelled constraints are
    /// not in the spec but we accept the shape for forward compat).
    pub label_id: Option<u32>,
    /// Which rel-type (when this is a relationship constraint).
    pub rel_type_id: Option<u32>,
    pub property_key: String,
    pub ty: ScalarType,
}

/// Structured payload attached to a constraint-violation error.
/// Mirrors the JSON shape documented in `docs/guides/CONSTRAINTS.md`.
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    pub constraint_name: Option<String>,
    pub kind: &'static str,
    pub entity_type: &'static str,
    pub labels_or_types: Vec<String>,
    pub properties: Vec<String>,
    pub offending_id: Option<u64>,
    pub offending_values: Vec<(String, Value)>,
}

impl ConstraintViolation {
    pub fn into_error(self) -> Error {
        let msg = format!(
            "ERR_CONSTRAINT_VIOLATED: kind={} entity={} labelsOrTypes={:?} properties={:?} \
             offending_id={:?}",
            self.kind, self.entity_type, self.labels_or_types, self.properties, self.offending_id,
        );
        Error::ConstraintViolation(msg)
    }
}

/// Report produced by the backfill validator when a constraint is
/// created on a non-empty dataset. Capped at 100 offending rows so
/// the error payload stays bounded.
#[derive(Debug, Clone, Default)]
pub struct BackfillReport {
    pub total_scanned: u64,
    pub offending: Vec<(u64, String)>,
}

impl BackfillReport {
    pub const MAX_OFFENDING: usize = 100;

    pub fn record(&mut self, id: u64, reason: String) {
        if self.offending.len() < Self::MAX_OFFENDING {
            self.offending.push((id, reason));
        }
    }

    pub fn has_violations(&self) -> bool {
        !self.offending.is_empty()
    }

    pub fn into_error(self, kind: &str) -> Error {
        let n = self.offending.len();
        let preview: Vec<String> = self
            .offending
            .into_iter()
            .take(5)
            .map(|(id, r)| format!("{id}:{r}"))
            .collect();
        Error::ConstraintViolation(format!(
            "ERR_CONSTRAINT_VIOLATED: backfill found {n} {kind} violation(s) (showing up to 5): \
             [{}]; total_scanned={}",
            preview.join(", "),
            self.total_scanned,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_integer_strict() {
        assert!(ScalarType::Integer.accepts(&json!(42)));
        assert!(!ScalarType::Integer.accepts(&json!(1.5)));
        assert!(!ScalarType::Integer.accepts(&json!("42")));
    }

    #[test]
    fn scalar_float_strict() {
        assert!(ScalarType::Float.accepts(&json!(1.5)));
        assert!(!ScalarType::Float.accepts(&json!(42)));
    }

    #[test]
    fn scalar_bytes_distinguished_from_map() {
        let bytes_value = json!({"_bytes": "AAH/"});
        let plain_map = json!({"a": 1});
        assert!(ScalarType::Bytes.accepts(&bytes_value));
        assert!(!ScalarType::Bytes.accepts(&plain_map));
        assert!(!ScalarType::Map.accepts(&bytes_value));
        assert!(ScalarType::Map.accepts(&plain_map));
    }

    #[test]
    fn parse_canonical_and_aliases() {
        assert_eq!(ScalarType::parse("INTEGER").unwrap(), ScalarType::Integer);
        assert_eq!(ScalarType::parse("int").unwrap(), ScalarType::Integer);
        assert_eq!(ScalarType::parse("bool").unwrap(), ScalarType::Boolean);
        assert!(ScalarType::parse("nonsense").is_err());
    }

    #[test]
    fn backfill_report_caps_at_100() {
        let mut r = BackfillReport::default();
        for i in 0..150u64 {
            r.record(i, format!("bad row {i}"));
        }
        assert_eq!(r.offending.len(), 100);
        assert!(r.has_violations());
    }

    #[test]
    fn violation_into_error_carries_shape() {
        let v = ConstraintViolation {
            constraint_name: Some("person_email_unique".to_string()),
            kind: "UNIQUENESS",
            entity_type: "NODE",
            labels_or_types: vec!["Person".to_string()],
            properties: vec!["email".to_string()],
            offending_id: Some(42),
            offending_values: vec![("email".to_string(), json!("a@b.c"))],
        };
        let msg = v.into_error().to_string();
        assert!(msg.contains("ERR_CONSTRAINT_VIOLATED"));
        assert!(msg.contains("UNIQUENESS"));
        assert!(msg.contains("Person"));
    }
}
