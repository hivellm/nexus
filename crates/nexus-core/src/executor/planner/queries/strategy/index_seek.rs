//! Inline-property index-seek lowering: `composite_index_seek_for` (composite
//! B-tree / NODE KEY seeks against a fully-covered inline property map) and
//! `node_index_seek_for` (single-property seeks, including correlated
//! row-local keys), both called from the node-selector loops in
//! `plan_execution_strategy` before falling back to `NodeByLabel`.

use super::*;

impl<'a> QueryPlanner<'a> {
    /// Build a `CompositeBtreeSeek` when a registered composite index
    /// (or NODE KEY constraint, which registers a UNIQUE composite
    /// index under the hood) on `label_id` has its FULL declared key
    /// set covered by `node`'s inline property-map equalities
    /// (`MATCH (n:L {a: 1, b: 2})` against an index on `(a, b)`).
    ///
    /// Called BEFORE [`Self::node_index_seek_for`] at both node-loop
    /// call sites so a composite index wins over a single-property
    /// index when both could apply: a composite seek narrows straight
    /// to the exact tuple, where a single-property seek would still
    /// need a residual `Filter` for the other predicate(s).
    ///
    /// Returns `None` (caller falls through to `node_index_seek_for` /
    /// `where_equality_index_seek_for` / `NodeByLabel`) when no
    /// composite-index handle is installed, the property map is empty
    /// or all-non-literal, or — critically — only a SUBSET of a
    /// registered index's key set is present. A partial match is
    /// deliberately never turned into a prefix seek here: the planner
    /// has no residual-filter wiring for the un-seeked trailing
    /// columns on this path, so seeking on an incomplete key would
    /// silently return every node sharing the seeked prefix instead of
    /// the caller's intended (narrower) match.
    pub(super) fn composite_index_seek_for(
        &self,
        node: &NodePattern,
        label_id: u32,
        label_name: &str,
        variable: &str,
    ) -> Option<Operator> {
        let registry = self.composite_index?;
        let property_map = node.properties.as_ref()?;

        let mut literal_values: HashMap<&str, serde_json::Value> = HashMap::new();
        for (prop_name, expr) in &property_map.properties {
            let value = match expr {
                Expression::Literal(Literal::String(s)) => serde_json::Value::String(s.clone()),
                Expression::Literal(Literal::Integer(i)) => serde_json::Value::from(*i),
                Expression::Literal(Literal::Float(f)) => match serde_json::Number::from_f64(*f) {
                    Some(n) => serde_json::Value::Number(n),
                    None => continue,
                },
                Expression::Literal(Literal::Boolean(b)) => serde_json::Value::Bool(*b),
                // null / point / parameter / non-literal / correlated:
                // not indexable at plan time for a composite seek.
                _ => continue,
            };
            literal_values.insert(prop_name.as_str(), value);
        }
        if literal_values.is_empty() {
            return None;
        }

        for (lbl, keys, _unique, _name) in registry.list() {
            if lbl != label_id || keys.is_empty() {
                continue;
            }
            if !keys.iter().all(|k| literal_values.contains_key(k.as_str())) {
                continue;
            }
            let prefix: Vec<(String, serde_json::Value)> = keys
                .iter()
                .filter_map(|k| {
                    literal_values
                        .get(k.as_str())
                        .cloned()
                        .map(|v| (k.clone(), v))
                })
                .collect();
            return Some(Operator::CompositeBtreeSeek {
                label: label_name.to_string(),
                variable: variable.to_string(),
                prefix,
            });
        }
        None
    }

    /// Build a `NodeIndexSeek` for the first inline equality property of
    /// `node` whose `(label_id, key_id)` has a registered property index
    /// and whose value is either an indexable literal (constant seek) or a
    /// row-local expression (`a.prop` / bare variable — per-row correlated
    /// seek, evaluated at execution time by `execute_correlated_index_seek`).
    /// Returns `None` (caller falls back to `NodeByLabel`) when no
    /// PropertyIndex handle is installed, no property qualifies, or the
    /// value is null/point/parameter/non-literal and non-correlated.
    pub(super) fn node_index_seek_for(
        &self,
        node: &NodePattern,
        label_id: u32,
        variable: &str,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let property_map = node.properties.as_ref()?;
        for (prop_name, expr) in &property_map.properties {
            let Ok(key_id) = self.catalog.get_key_id(prop_name) else {
                continue;
            };
            if !prop_idx.has_index(label_id, key_id) {
                continue;
            }
            match expr {
                // Constant: value baked into the plan at plan time.
                Expression::Literal(Literal::String(s)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::String(s.clone()),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                Expression::Literal(Literal::Integer(i)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Integer(*i),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                Expression::Literal(Literal::Float(f)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Float(*f),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                Expression::Literal(Literal::Boolean(b)) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Boolean(*b),
                        key_expression: None,
                        variable: variable.to_string(),
                    });
                }
                // Row-local / correlated: e.g. `r.s` from
                // `UNWIND $rows AS r MATCH (a:P {id: r.s})`. The key is
                // evaluated per driving row at execution time, so the
                // plan-time `value` is a documented no-op placeholder —
                // `execute_correlated_index_seek` ignores it whenever
                // `key_expression` is `Some(_)`.
                Expression::PropertyAccess { .. } | Expression::Variable(_) => {
                    return Some(Operator::NodeIndexSeek {
                        label_id,
                        key_id,
                        value: crate::index::PropertyValue::Null,
                        key_expression: Some(expr.clone()),
                        variable: variable.to_string(),
                    });
                }
                // null / point / param / other non-literal: not indexable.
                _ => continue,
            }
        }
        None
    }
}
