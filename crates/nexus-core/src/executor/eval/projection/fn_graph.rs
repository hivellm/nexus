//! Graph-entity, path, and graph-metadata built-in functions for the
//! projection evaluator.
//!
//! Covers `__label_predicate__`, `labels`, `type`, `keys`, `id`,
//! `database`, `db`, `nodes`, `relationships`, `length`,
//! `shortestpath`, `allshortestpaths`, and `exists`.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use super::super::super::types::Direction;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    /// Evaluate graph-entity and path built-in functions.
    ///
    /// Returns `None` if the function name is not handled here.
    pub(super) fn eval_builtin_graph(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        name: &str,
        args: &[parser::Expression],
    ) -> Option<Result<Value>> {
        match name {
            // phase6_opencypher-quickwins §8 — runtime evaluator
            // for the synthetic `__label_predicate__(var, 'Label')`
            // the parser emits for `var:Label` in WHERE /
            // RETURN position.
            "__label_predicate__" => {
                if args.len() == 2 {
                    let node_val = match self.evaluate_projection_expression(row, context, &args[0])
                    {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let label_val =
                        match self.evaluate_projection_expression(row, context, &args[1]) {
                            Ok(v) => v,
                            Err(e) => return Some(Err(e)),
                        };
                    let label_name = match label_val {
                        Value::String(s) => s,
                        Value::Null => return Some(Ok(Value::Bool(false))),
                        other => {
                            return Some(Err(Error::TypeMismatch {
                                expected: "STRING label".to_string(),
                                actual: format!("{other:?}"),
                            }));
                        }
                    };
                    if label_name.is_empty() {
                        return Some(Ok(Value::Bool(false)));
                    }
                    let node_id = match &node_val {
                        Value::Object(obj) => obj.get("_nexus_id").and_then(|v| v.as_u64()),
                        _ => None,
                    };
                    let Some(nid) = node_id else {
                        return Some(Ok(Value::Bool(false)));
                    };
                    let Ok(label_id) = self.catalog().get_label_id(&label_name) else {
                        return Some(Ok(Value::Bool(false)));
                    };
                    let Ok(node_record) = self.store().read_node(nid) else {
                        return Some(Ok(Value::Bool(false)));
                    };
                    let present = if label_id < 64 {
                        (node_record.label_bits & (1u64 << label_id)) != 0
                    } else {
                        false
                    };
                    return Some(Ok(Value::Bool(present)));
                }
                Some(Ok(Value::Null))
            }
            "labels" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let node_id = if let Value::Object(obj) = &value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            id.as_u64()
                        } else {
                            None
                        }
                    } else if let Value::String(id_str) = &value {
                        id_str.parse::<u64>().ok()
                    } else {
                        None
                    };

                    if let Some(nid) = node_id {
                        if let Ok(node_record) = self.store().read_node(nid) {
                            if let Ok(label_names) = self
                                .catalog()
                                .get_labels_from_bitmap(node_record.label_bits)
                            {
                                let labels: Vec<Value> =
                                    label_names.into_iter().map(Value::String).collect();
                                return Some(Ok(Value::Array(labels)));
                            }
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "type" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let rel_id = if let Value::Object(obj) = &value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            id.as_u64()
                        } else {
                            None
                        }
                    } else if let Value::String(id_str) = &value {
                        id_str.parse::<u64>().ok()
                    } else {
                        None
                    };

                    if let Some(rid) = rel_id {
                        if let Ok(rel_record) = self.store().read_rel(rid) {
                            if let Ok(Some(type_name)) =
                                self.catalog().get_type_name(rel_record.type_id)
                            {
                                return Some(Ok(Value::String(type_name)));
                            }
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "keys" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(obj) = &value {
                        let mut keys: Vec<String> = obj
                            .keys()
                            .filter(|k| {
                                // Exclude internal fields:
                                // - Fields starting with _ (like _nexus_id, _nexus_type)
                                // - "type" field (internal relationship type)
                                !k.starts_with('_') && *k != "type"
                            })
                            .map(|k| k.to_string())
                            .collect();
                        keys.sort();
                        let key_values: Vec<Value> = keys.into_iter().map(Value::String).collect();
                        return Some(Ok(Value::Array(key_values)));
                    }
                }
                Some(Ok(Value::Array(Vec::new())))
            }
            "id" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(obj) = &value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            return Some(Ok(Value::Number(id.clone())));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            // Database functions
            "database" => {
                // Return current database name
                if let Some(db_manager_arc) = self.shared.database_manager() {
                    let db_manager = db_manager_arc.read();
                    Some(Ok(Value::String(
                        db_manager.default_database_name().to_string(),
                    )))
                } else {
                    Some(Ok(Value::String("neo4j".to_string())))
                }
            }
            "db" => {
                // Alias for database()
                if let Some(db_manager_arc) = self.shared.database_manager() {
                    let db_manager = db_manager_arc.read();
                    Some(Ok(Value::String(
                        db_manager.default_database_name().to_string(),
                    )))
                } else {
                    Some(Ok(Value::String("neo4j".to_string())))
                }
            }
            // Path functions
            "nodes" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    // If value is already an array, treat it as a path of nodes
                    if let Value::Array(arr) = value {
                        // Filter only node objects (objects with _nexus_id)
                        let nodes: Vec<Value> = arr
                            .into_iter()
                            .filter(|v| {
                                if let Value::Object(obj) = v {
                                    obj.contains_key("_nexus_id")
                                } else {
                                    false
                                }
                            })
                            .collect();
                        return Some(Ok(Value::Array(nodes)));
                    }
                    // If it's a single node, return it as array
                    if let Value::Object(obj) = &value {
                        if obj.contains_key("_nexus_id") {
                            return Some(Ok(Value::Array(vec![value])));
                        }
                    }
                }
                Some(Ok(Value::Array(Vec::new())))
            }
            "relationships" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    // If value is already an array, extract relationships
                    if let Value::Array(arr) = value {
                        // Filter only relationship objects (objects with _nexus_type and source/target)
                        let rels: Vec<Value> = arr
                            .into_iter()
                            .filter(|v| {
                                if let Value::Object(obj) = v {
                                    obj.contains_key("_nexus_type")
                                        && (obj.contains_key("_source")
                                            || obj.contains_key("_target"))
                                } else {
                                    false
                                }
                            })
                            .collect();
                        return Some(Ok(Value::Array(rels)));
                    }
                    // If it's a single relationship, return it as array
                    if let Value::Object(obj) = &value {
                        if obj.contains_key("_nexus_type") {
                            return Some(Ok(Value::Array(vec![value])));
                        }
                    }
                }
                Some(Ok(Value::Array(Vec::new())))
            }
            "length" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    // For arrays representing paths, length is the number of relationships
                    // which is (number of nodes - 1) or number of relationship objects
                    if let Value::Array(arr) = value {
                        // Count relationship objects in the path
                        let rel_count = arr
                            .iter()
                            .filter(|v| {
                                if let Value::Object(obj) = v {
                                    obj.contains_key("_nexus_type")
                                } else {
                                    false
                                }
                            })
                            .count();
                        return Some(Ok(Value::Number((rel_count as i64).into())));
                    }
                    // For a single relationship, length is 1
                    if let Value::Object(obj) = &value {
                        if obj.contains_key("_nexus_type") {
                            return Some(Ok(Value::Number(1.into())));
                        }
                    }
                }
                Some(Ok(Value::Number(0.into())))
            }
            "shortestpath" => {
                // shortestPath((start)-[*]->(end))
                // Returns the shortest path between two nodes
                if !args.is_empty() {
                    if let parser::Expression::PatternComprehension { pattern, .. } = &args[0] {
                        if let (Some(start_node), Some(end_node)) =
                            (pattern.elements.first(), pattern.elements.last())
                        {
                            if let (
                                parser::PatternElement::Node(start),
                                parser::PatternElement::Node(end),
                            ) = (start_node, end_node)
                            {
                                let start_id = if let Some(var) = &start.variable {
                                    if let Some(Value::Object(obj)) = row.get(var) {
                                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                            id.as_u64()
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                let end_id = if let Some(var) = &end.variable {
                                    if let Some(Value::Object(obj)) = row.get(var) {
                                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                            id.as_u64()
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                if let (Some(start_id), Some(end_id)) = (start_id, end_id) {
                                    // Extract relationship type and direction from
                                    // pattern.
                                    fn extract_first_rel<'a>(
                                        elements: &'a [parser::PatternElement],
                                    ) -> Option<&'a parser::RelationshipPattern>
                                    {
                                        for el in elements {
                                            match el {
                                                parser::PatternElement::Relationship(r) => {
                                                    return Some(r);
                                                }
                                                parser::PatternElement::QuantifiedGroup(g) => {
                                                    if let Some(r) = extract_first_rel(&g.inner) {
                                                        return Some(r);
                                                    }
                                                }
                                                parser::PatternElement::Node(_) => {}
                                            }
                                        }
                                        None
                                    }
                                    let rel = extract_first_rel(&pattern.elements);
                                    let rel_type = rel.and_then(|r| r.types.first().cloned());
                                    let type_id = rel_type.and_then(|t| {
                                        self.catalog().get_type_id(&t).ok().flatten()
                                    });
                                    let direction = rel
                                        .map(|r| super::fn_geo::direction_from_rel(r))
                                        .unwrap_or(Direction::Both);

                                    if let Ok(Some(path)) = self
                                        .find_shortest_path(start_id, end_id, type_id, direction)
                                    {
                                        return Some(Ok(self.path_to_value(&path)));
                                    }
                                }
                            }
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "allshortestpaths" => {
                // allShortestPaths((start)-[*]->(end))
                if !args.is_empty() {
                    if let parser::Expression::PatternComprehension { pattern, .. } = &args[0] {
                        if let (Some(start_node), Some(end_node)) =
                            (pattern.elements.first(), pattern.elements.last())
                        {
                            if let (
                                parser::PatternElement::Node(start),
                                parser::PatternElement::Node(end),
                            ) = (start_node, end_node)
                            {
                                let start_id = if let Some(var) = &start.variable {
                                    if let Some(Value::Object(obj)) = row.get(var) {
                                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                            id.as_u64()
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                let end_id = if let Some(var) = &end.variable {
                                    if let Some(Value::Object(obj)) = row.get(var) {
                                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                            id.as_u64()
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                if let (Some(start_id), Some(end_id)) = (start_id, end_id) {
                                    let rel_type = pattern.elements.iter().find_map(|e| {
                                        if let parser::PatternElement::Relationship(rel) = e {
                                            rel.types.first().cloned()
                                        } else {
                                            None
                                        }
                                    });
                                    let type_id = rel_type.and_then(|t| {
                                        self.catalog().get_type_id(&t).ok().flatten()
                                    });
                                    let direction = pattern
                                        .elements
                                        .iter()
                                        .find_map(|e| {
                                            if let parser::PatternElement::Relationship(rel) = e {
                                                Some(super::fn_geo::direction_from_rel(rel))
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or(Direction::Both);

                                    if let Ok(paths) = self.find_all_shortest_paths(
                                        start_id, end_id, type_id, direction,
                                    ) {
                                        let path_values: Vec<Value> =
                                            paths.iter().map(|p| self.path_to_value(p)).collect();
                                        return Some(Ok(Value::Array(path_values)));
                                    }
                                }
                            }
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            _ => None,
        }
    }
}
