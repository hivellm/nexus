//! Geospatial built-in functions for the projection evaluator.
//!
//! Covers `distance`, `point.distance`, `point.withinbbox`,
//! `point.withindistance`, `point.azimuth`, and `point.nearest`.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use super::super::super::types::Direction;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    /// Evaluate geospatial built-in functions.
    ///
    /// Returns `None` if the function name is not handled here.
    pub(super) fn eval_builtin_geo(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        name: &str,
        args: &[parser::Expression],
    ) -> Option<Result<Value>> {
        match name {
            // Geospatial predicate functions
            // (phase6_opencypher-geospatial-predicates §4).
            // Each predicate extracts up to two points via
            // `Point::from_json_value` and reuses the
            // existing distance / CRS helpers. CRS or
            // dimensionality mismatches surface as a
            // CypherSyntax error with the `ERR_CRS_MISMATCH`
            // prefix so SDK assertions can pattern-match on
            // the error code.
            "point.withinbbox" => {
                if args.len() < 2 {
                    return Some(Ok(Value::Null));
                }
                let p_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let bbox_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if p_val.is_null() || bbox_val.is_null() {
                    return Some(Ok(Value::Null));
                }
                let Value::Object(_) = &p_val else {
                    return Some(Ok(Value::Null));
                };
                let p = match crate::geospatial::Point::from_json_value(&p_val) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(Error::CypherSyntax(format!("Invalid point: {e}")))),
                };
                let bbox_obj = match bbox_val.as_object() {
                    Some(o) => o,
                    None => {
                        return Some(Err(Error::CypherSyntax(
                            "ERR_BBOX_MALFORMED: bbox must be a map".to_string(),
                        )));
                    }
                };
                let bl_v = match bbox_obj.get("bottomLeft") {
                    Some(v) => v,
                    None => {
                        return Some(Err(Error::CypherSyntax(
                            "ERR_BBOX_MALFORMED: missing 'bottomLeft'".to_string(),
                        )));
                    }
                };
                let tr_v = match bbox_obj.get("topRight") {
                    Some(v) => v,
                    None => {
                        return Some(Err(Error::CypherSyntax(
                            "ERR_BBOX_MALFORMED: missing 'topRight'".to_string(),
                        )));
                    }
                };
                let bl = match crate::geospatial::Point::from_json_value(bl_v) {
                    Ok(p) => p,
                    Err(e) => {
                        return Some(Err(Error::CypherSyntax(format!(
                            "ERR_BBOX_MALFORMED: bottomLeft: {e}"
                        ))));
                    }
                };
                let tr = match crate::geospatial::Point::from_json_value(tr_v) {
                    Ok(p) => p,
                    Err(e) => {
                        return Some(Err(Error::CypherSyntax(format!(
                            "ERR_BBOX_MALFORMED: topRight: {e}"
                        ))));
                    }
                };
                if !p.same_crs(&bl) || !p.same_crs(&tr) {
                    return Some(Err(Error::CypherSyntax(format!(
                        "ERR_CRS_MISMATCH: point={}, bbox=({}, {})",
                        p.crs_name(),
                        bl.crs_name(),
                        tr.crs_name()
                    ))));
                }
                Some(Ok(Value::Bool(p.within_bbox(&bl, &tr))))
            }
            "point.withindistance" => {
                if args.len() < 3 {
                    return Some(Ok(Value::Null));
                }
                let a_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let b_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let d_val = match self.evaluate_projection_expression(row, context, &args[2]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if a_val.is_null() || b_val.is_null() || d_val.is_null() {
                    return Some(Ok(Value::Null));
                }
                let Value::Object(_) = &a_val else {
                    return Some(Ok(Value::Null));
                };
                let Value::Object(_) = &b_val else {
                    return Some(Ok(Value::Null));
                };
                let a = match crate::geospatial::Point::from_json_value(&a_val) {
                    Ok(p) => p,
                    Err(e) => {
                        return Some(Err(Error::CypherSyntax(format!("Invalid point a: {e}"))));
                    }
                };
                let b = match crate::geospatial::Point::from_json_value(&b_val) {
                    Ok(p) => p,
                    Err(e) => {
                        return Some(Err(Error::CypherSyntax(format!("Invalid point b: {e}"))));
                    }
                };
                if !a.same_crs(&b) {
                    return Some(Err(Error::CypherSyntax(format!(
                        "ERR_CRS_MISMATCH: a={}, b={}",
                        a.crs_name(),
                        b.crs_name()
                    ))));
                }
                let dist = match self.value_to_number(&d_val) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                Some(Ok(Value::Bool(a.distance_to(&b) <= dist)))
            }
            "point.azimuth" => {
                if args.len() < 2 {
                    return Some(Ok(Value::Null));
                }
                let a_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let b_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if a_val.is_null() || b_val.is_null() {
                    return Some(Ok(Value::Null));
                }
                let Value::Object(_) = &a_val else {
                    return Some(Ok(Value::Null));
                };
                let Value::Object(_) = &b_val else {
                    return Some(Ok(Value::Null));
                };
                let a = match crate::geospatial::Point::from_json_value(&a_val) {
                    Ok(p) => p,
                    Err(e) => {
                        return Some(Err(Error::CypherSyntax(format!("Invalid point a: {e}"))));
                    }
                };
                let b = match crate::geospatial::Point::from_json_value(&b_val) {
                    Ok(p) => p,
                    Err(e) => {
                        return Some(Err(Error::CypherSyntax(format!("Invalid point b: {e}"))));
                    }
                };
                if !a.same_crs(&b) {
                    return Some(Err(Error::CypherSyntax(format!(
                        "ERR_CRS_MISMATCH: a={}, b={}",
                        a.crs_name(),
                        b.crs_name()
                    ))));
                }
                Some(match a.azimuth_to(&b) {
                    Some(deg) => serde_json::Number::from_f64(deg)
                        .map(Value::Number)
                        .ok_or_else(|| Error::TypeMismatch {
                            expected: "number".to_string(),
                            actual: "non-finite".to_string(),
                        }),
                    None => Ok(Value::Null),
                })
            }
            "point.nearest" => {
                // phase6_spatial-planner-followups §1 —
                // function-style k-NN over a label's
                // points. Signature:
                //   point.nearest(<var>.<prop>, <pt>, <k>)
                // Returns LIST<NODE> ordered ascending by
                // distance. With an R-tree index on the
                // matching `(label, property)` pair the
                // function walks the registry directly;
                // without one it falls back to a label
                // scan + sort + truncate so the spec
                // scenario "same query with and without
                // index returns same LIST<NODE>" holds.
                if args.len() < 3 {
                    return Some(Err(Error::CypherExecution(
                        "ERR_MISSING_ARG: point.nearest requires \
                         (variable.property, point, k)"
                            .to_string(),
                    )));
                }
                // First arg must be `<var>.<prop>` so we
                // can resolve the variable's label at
                // call time.
                let (variable, property) = match &args[0] {
                    parser::Expression::PropertyAccess { variable, property } => {
                        (variable.clone(), property.clone())
                    }
                    _ => {
                        return Some(Err(Error::CypherExecution(
                            "ERR_INVALID_ARG_TYPE: point.nearest first arg \
                             must be a property access (e.g. n.loc)"
                                .to_string(),
                        )));
                    }
                };
                // Centre point.
                let pt_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let centre = match &pt_val {
                    Value::Object(_) => match crate::geospatial::Point::from_json_value(&pt_val) {
                        Ok(p) => p,
                        Err(e) => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_INVALID_ARG_TYPE: point.nearest centre is \
                                     not a Point: {e}"
                            ))));
                        }
                    },
                    _ => {
                        return Some(Err(Error::CypherExecution(
                            "ERR_INVALID_ARG_TYPE: point.nearest centre must \
                             be a Point"
                                .to_string(),
                        )));
                    }
                };
                // k.
                let k_val = match self.evaluate_projection_expression(row, context, &args[2]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let k = match &k_val {
                    Value::Number(n) => match n.as_i64() {
                        Some(i) if i >= 0 => i as usize,
                        _ => {
                            return Some(Err(Error::CypherExecution(
                                "ERR_INVALID_ARG_TYPE: point.nearest k must \
                                 be a non-negative INTEGER"
                                    .to_string(),
                            )));
                        }
                    },
                    _ => {
                        return Some(Err(Error::CypherExecution(
                            "ERR_INVALID_ARG_TYPE: point.nearest k must be \
                             a non-negative INTEGER"
                                .to_string(),
                        )));
                    }
                };
                if k == 0 {
                    return Some(Ok(Value::Array(Vec::new())));
                }
                // Resolve the variable's label via its
                // bound `_nexus_id`: read the node record,
                // decode `label_bits` against the catalog,
                // pick the first label. This avoids
                // threading a var→label map through the
                // projection evaluator and works with the
                // existing node-as-Value shape (which
                // `read_node_as_value` does not include
                // `_labels` for).
                let bound = row.get(&variable).cloned().unwrap_or_else(|| {
                    context
                        .get_variable(&variable)
                        .cloned()
                        .unwrap_or(Value::Null)
                });
                let bound_id = match Self::extract_entity_id(&bound) {
                    Some(id) => id,
                    None => {
                        return Some(Err(Error::CypherExecution(format!(
                            "ERR_INVALID_ARG_TYPE: point.nearest could not resolve \
                             a node for variable '{variable}' — bind it via \
                             MATCH (var:Label) so the index lookup can route to \
                             {{label}}.{{prop}}"
                        ))));
                    }
                };
                let label_name = {
                    let store = self.shared.store.read();
                    let node_record = match store.read_node(bound_id) {
                        Ok(r) => r,
                        Err(e) => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_NODE_READ: {bound_id}: {e}"
                            ))));
                        }
                    };
                    drop(store);
                    let labels = match self
                        .catalog()
                        .get_labels_from_bitmap(node_record.label_bits)
                    {
                        Ok(l) => l,
                        Err(e) => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_LABEL_DECODE: {bound_id}: {e}"
                            ))));
                        }
                    };
                    match labels.into_iter().next() {
                        Some(l) => l,
                        None => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_INVALID_ARG_TYPE: point.nearest variable \
                                 '{variable}' has no labels — index lookup \
                                 requires a label-bound binding"
                            ))));
                        }
                    }
                };
                let index_name = format!("{label_name}.{property}");

                // Index path — walk the R-tree directly.
                let registry = self.shared.rtree_registry.clone();
                let hits = if registry.contains(&index_name) {
                    match registry.nearest_with_filter(
                        &index_name,
                        centre.x,
                        centre.y,
                        k,
                        crate::index::rtree::search::Metric::Cartesian,
                        |_| true,
                    ) {
                        Ok(r) => r.into_iter().map(|h| h.node_id).collect::<Vec<u64>>(),
                        Err(e) => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_RTREE_SEEK: {e}"
                            ))));
                        }
                    }
                } else {
                    // Scan-fallback — read the label
                    // bitmap, compute distances, sort,
                    // truncate to k. The spec scenario
                    // requires identical LIST<NODE>
                    // shape with or without the index.
                    let label_id = match self.catalog().get_label_id(&label_name) {
                        Ok(id) => id,
                        Err(e) => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_LABEL_NOT_FOUND: {label_name}: {e}"
                            ))));
                        }
                    };
                    let bitmap = match self
                        .shared
                        .label_index
                        .read()
                        .get_nodes_with_labels(&[label_id])
                    {
                        Ok(b) => b,
                        Err(e) => {
                            return Some(Err(Error::CypherExecution(format!(
                                "ERR_LABEL_INDEX: {e}"
                            ))));
                        }
                    };
                    let mut scored: Vec<(f64, u64)> = Vec::new();
                    for raw_id in bitmap.iter() {
                        let node_id = raw_id as u64;
                        let props = match self.shared.store.read().load_node_properties(node_id) {
                            Ok(Some(Value::Object(m))) => m,
                            _ => continue,
                        };
                        let val = match props.get(&property) {
                            Some(v) => v,
                            None => continue,
                        };
                        let p = match crate::geospatial::Point::from_json_value(val) {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        if !p.same_crs(&centre) {
                            continue;
                        }
                        scored.push((centre.distance_to(&p), node_id));
                    }
                    scored.sort_by(|a, b| {
                        a.0.partial_cmp(&b.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| a.1.cmp(&b.1))
                    });
                    scored.truncate(k);
                    scored.into_iter().map(|(_, id)| id).collect()
                };

                let mut out: Vec<Value> = Vec::with_capacity(hits.len());
                for node_id in hits {
                    match self.read_node_as_value(node_id) {
                        Ok(v) if !v.is_null() => out.push(v),
                        _ => continue,
                    }
                }
                Some(Ok(Value::Array(out)))
            }
            "point.distance" => {
                // point.distance as a namespaced alias for
                // the bare `distance()` function. Keeps
                // parity with Neo4j's newer surface where
                // the `point.*` namespace is the idiomatic
                // one.
                if args.len() < 2 {
                    return Some(Ok(Value::Null));
                }
                let a_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let b_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if a_val.is_null() || b_val.is_null() {
                    return Some(Ok(Value::Null));
                }
                let a = if let Value::Object(_) = &a_val {
                    match crate::geospatial::Point::from_json_value(&a_val) {
                        Ok(p) => p,
                        Err(e) => {
                            return Some(Err(Error::CypherSyntax(format!("Invalid point a: {e}"))));
                        }
                    }
                } else {
                    return Some(Ok(Value::Null));
                };
                let b = if let Value::Object(_) = &b_val {
                    match crate::geospatial::Point::from_json_value(&b_val) {
                        Ok(p) => p,
                        Err(e) => {
                            return Some(Err(Error::CypherSyntax(format!("Invalid point b: {e}"))));
                        }
                    }
                } else {
                    return Some(Ok(Value::Null));
                };
                if !a.same_crs(&b) {
                    return Some(Err(Error::CypherSyntax(format!(
                        "ERR_CRS_MISMATCH: a={}, b={}",
                        a.crs_name(),
                        b.crs_name()
                    ))));
                }
                Some(
                    serde_json::Number::from_f64(a.distance_to(&b))
                        .map(Value::Number)
                        .ok_or_else(|| Error::TypeMismatch {
                            expected: "number".to_string(),
                            actual: "non-finite".to_string(),
                        }),
                )
            }
            // Geospatial functions
            "distance" => {
                // distance(point1, point2) - calculate distance between two points
                if args.len() >= 2 {
                    let p1_val = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let p2_val = match self.evaluate_projection_expression(row, context, &args[1]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };

                    // Try to parse points from JSON values
                    // Points can be:
                    // 1. Point literals (already converted to JSON objects via to_json_value)
                    // 2. JSON objects with x/y/z/crs fields
                    let p1 = if let Value::Object(_) = &p1_val {
                        match crate::geospatial::Point::from_json_value(&p1_val) {
                            Ok(p) => p,
                            Err(_) => {
                                return Some(Err(Error::CypherSyntax(
                                    "Invalid point 1".to_string(),
                                )));
                            }
                        }
                    } else {
                        return Some(Ok(Value::Null));
                    };

                    let p2 = if let Value::Object(_) = &p2_val {
                        match crate::geospatial::Point::from_json_value(&p2_val) {
                            Ok(p) => p,
                            Err(_) => {
                                return Some(Err(Error::CypherSyntax(
                                    "Invalid point 2".to_string(),
                                )));
                            }
                        }
                    } else {
                        return Some(Ok(Value::Null));
                    };

                    let distance = p1.distance_to(&p2);
                    return Some(
                        serde_json::Number::from_f64(distance)
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            }),
                    );
                }
                Some(Ok(Value::Null))
            }
            _ => None,
        }
    }
}

/// Helper used by `shortestpath` and `allshortestpaths` to extract a
/// traversal direction from a relationship pattern element.
pub(super) fn direction_from_rel(rel: &parser::RelationshipPattern) -> Direction {
    match rel.direction {
        parser::RelationshipDirection::Outgoing => Direction::Outgoing,
        parser::RelationshipDirection::Incoming => Direction::Incoming,
        parser::RelationshipDirection::Both => Direction::Both,
    }
}
