//! Quantified Path Pattern executor — slice 2 of
//! `phase6_opencypher-quantified-path-patterns`.
//!
//! Handles the dedicated `Operator::QuantifiedExpand`, which the
//! planner emits when a `QuantifiedGroup` survives the slice-1
//! lowering (anything with named/labelled inner boundary nodes,
//! relationship-property filters that cannot live on a legacy
//! `*m..n` form, etc.). Slice 2 covers single-relationship
//! bodies — `(start)-[rel]->(end)` between the outer
//! `source_var` and `target_var`. Multi-hop bodies and inner
//! `WHERE` clauses still surface `ERR_QPP_NOT_IMPLEMENTED` from
//! the planner.
//!
//! ## Execution model
//!
//! For each input row carrying `source_var`, BFS hops along the
//! inner relationship. Each frame tracks:
//!
//! - `current_node`: the node we land on after `k` hops
//! - `iteration`: number of hops taken so far
//! - `path_nodes` / `path_rels`: ordered lists of intermediate
//!   nodes and relationships, used for list-promotion when the
//!   inner body declares boundary-node variables or a
//!   relationship variable
//!
//! When `iteration` is in `[min_length, max_length]`, the frame
//! satisfies the quantifier and the operator emits one row with
//! the outer `target_var` bound to `current_node`, every
//! list-promoted variable bound to its accumulated `Vec<Value>`,
//! and the source row's existing bindings carried through.
//!
//! Cycle policy is hard-coded to `NODES_CAN_REPEAT` (the MATCH
//! default; `shortestPath`/`allShortestPaths` over QPP are slice
//! 5 work).

use serde_json::Value;
use std::collections::{HashMap, VecDeque};

use crate::Result;
use crate::executor::Executor;
use crate::executor::context::{ExecutionContext, RelationshipInfo};
use crate::executor::parser::{self, PropertyMap};
use crate::executor::push_with_row_cap;
use crate::executor::types::Direction;

/// Per-query safety cap on iteration depth when the quantifier is
/// unbounded (`*` / `+` / `{m,}`). Picking `64` mirrors the
/// `max_qpp_depth` budget called out in the design doc and keeps
/// per-frame memory tractable: each frame costs
/// `O(|path_nodes| + |path_rels|)`, and 64 is well past any
/// real-world transitive-closure pattern (`REPORTS_TO`,
/// `BLOCKED_BY`, …).
const MAX_QPP_DEPTH: usize = 64;

impl Executor {
    /// Execute one `QuantifiedExpand` operator. See module-level
    /// docs for the BFS shape; the public surface here is just the
    /// dispatch glue.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor) fn execute_quantified_expand(
        &self,
        context: &mut ExecutionContext,
        source_var: &str,
        target_var: &str,
        inner_rel_type_ids: &[u32],
        inner_rel_direction: Direction,
        inner_rel_var: Option<&str>,
        inner_rel_properties: Option<&PropertyMap>,
        inner_start_node_var: Option<&str>,
        inner_start_node_labels: &[String],
        inner_start_node_properties: Option<&PropertyMap>,
        inner_end_node_var: Option<&str>,
        inner_end_node_labels: &[String],
        inner_end_node_properties: Option<&PropertyMap>,
        min_length: usize,
        max_length: usize,
        optional: bool,
    ) -> Result<()> {
        // Cap the upper bound so an unbounded quantifier (`*`, `+`,
        // `{m,}`) doesn't allow a runaway BFS. The user's intent
        // ("unbounded") still holds within the safety budget.
        let effective_max = max_length.min(MAX_QPP_DEPTH);

        // Pull the source rows from the current execution context.
        // Same pattern as `execute_variable_length_path` — prefer
        // the materialised result-set rows, fall back to the scalar
        // variable bag when the operator is the first in the plan.
        let rows = if !context.result_set.rows.is_empty() {
            self.result_set_as_rows(context)
        } else {
            self.materialize_rows_from_variables(context)
        };

        if rows.is_empty() {
            return Ok(());
        }

        let mut expanded_rows = Vec::new();

        for row in rows {
            let source_value = row
                .get(source_var)
                .cloned()
                .or_else(|| context.get_variable(source_var).cloned())
                .unwrap_or(Value::Null);

            let source_id = match Self::extract_entity_id(&source_value) {
                Some(id) => id,
                None => {
                    if optional {
                        // OPTIONAL MATCH: keep the source row with NULL
                        // bindings for everything the QPP would have
                        // produced.
                        let mut new_row = row.clone();
                        new_row.insert(source_var.to_string(), source_value.clone());
                        new_row.insert(target_var.to_string(), Value::Null);
                        if let Some(v) = inner_rel_var {
                            new_row.insert(v.to_string(), Value::Array(Vec::new()));
                        }
                        if let Some(v) = inner_start_node_var {
                            new_row.insert(v.to_string(), Value::Array(Vec::new()));
                        }
                        if let Some(v) = inner_end_node_var {
                            new_row.insert(v.to_string(), Value::Array(Vec::new()));
                        }
                        push_with_row_cap(&mut expanded_rows, new_row, "QuantifiedExpand")?;
                    }
                    continue;
                }
            };

            // Special case: `min_length == 0` accepts the empty
            // path — emit a row with `target_var = source_var` and
            // every list-promoted variable as an empty array.
            if min_length == 0 {
                if let Some(emit) = self.qpp_build_emission_row(
                    &row,
                    source_var,
                    &source_value,
                    target_var,
                    source_id,
                    &[],
                    &[],
                    inner_rel_var,
                    inner_start_node_var,
                    inner_end_node_var,
                )? {
                    push_with_row_cap(&mut expanded_rows, emit, "QuantifiedExpand")?;
                }
            }

            // BFS frame: (current_node, iteration_count, path_nodes,
            // path_rels). `path_nodes` records the START node of
            // each iteration so list-promotion of the inner-start
            // variable matches GQL ordering (`x[0]` is iteration 0's
            // start node). `path_rels` and end-node tracking
            // follow the same convention.
            let mut queue: VecDeque<(u64, usize, Vec<u64>, Vec<u64>, Vec<u64>)> = VecDeque::new();
            // Seed with iteration 0 from the source.
            queue.push_back((source_id, 0, Vec::new(), Vec::new(), Vec::new()));

            // Track (node, iteration) we have already expanded out
            // of so we don't re-walk the same wavefront. Cycle
            // policy is NODES_CAN_REPEAT — different paths reaching
            // the same node at the same depth are still candidates
            // and must be deduped only by the `(node, depth)`
            // wavefront key.
            let mut visited: std::collections::HashSet<(u64, usize)> =
                std::collections::HashSet::new();
            visited.insert((source_id, 0));

            while let Some((current_node, iteration, path_starts, path_ends, path_rels)) =
                queue.pop_front()
            {
                if iteration >= 1
                    && iteration >= min_length
                    && iteration <= effective_max
                    && self.qpp_path_satisfies_filters(
                        &path_starts,
                        inner_start_node_labels,
                        inner_start_node_properties,
                        &path_ends,
                        inner_end_node_labels,
                        inner_end_node_properties,
                    )?
                {
                    if let Some(emit) = self.qpp_build_emission_row(
                        &row,
                        source_var,
                        &source_value,
                        target_var,
                        current_node,
                        &path_starts,
                        &path_ends,
                        inner_rel_var,
                        inner_start_node_var,
                        inner_end_node_var,
                    )? {
                        // Re-insert the path-rels list when the
                        // body declares a relationship variable.
                        if let Some(rel_var) = inner_rel_var {
                            let rel_values = self.qpp_path_rels_as_value_list(&path_rels)?;
                            let mut emit = emit;
                            emit.insert(rel_var.to_string(), Value::Array(rel_values));
                            push_with_row_cap(&mut expanded_rows, emit, "QuantifiedExpand")?;
                        } else {
                            push_with_row_cap(&mut expanded_rows, emit, "QuantifiedExpand")?;
                        }
                    }
                }

                // Stop expanding once we have reached the cap.
                if iteration >= effective_max {
                    continue;
                }

                let neighbors = self.find_relationships(
                    current_node,
                    inner_rel_type_ids,
                    inner_rel_direction,
                    None,
                )?;

                for rel_info in neighbors {
                    // Inner-relationship property filter.
                    if let Some(props) = inner_rel_properties
                        && !self.qpp_relationship_matches_properties(&rel_info, props)?
                    {
                        continue;
                    }

                    let next_node = match inner_rel_direction {
                        Direction::Outgoing => rel_info.target_id,
                        Direction::Incoming => rel_info.source_id,
                        Direction::Both => {
                            if rel_info.source_id == current_node {
                                rel_info.target_id
                            } else {
                                rel_info.source_id
                            }
                        }
                    };

                    let next_iteration = iteration + 1;

                    // Per-frame copies: we record the start node
                    // (= `current_node`) and the end node
                    // (= `next_node`) for this hop so list-promoted
                    // variables get the correct value at index
                    // `iteration`.
                    let mut new_starts = path_starts.clone();
                    new_starts.push(current_node);
                    let mut new_ends = path_ends.clone();
                    new_ends.push(next_node);
                    let mut new_rels = path_rels.clone();
                    new_rels.push(rel_info.id);

                    let visit_key = (next_node, next_iteration);
                    if visited.insert(visit_key) {
                        queue.push_back((
                            next_node,
                            next_iteration,
                            new_starts,
                            new_ends,
                            new_rels,
                        ));
                    }
                }
            }
        }

        self.update_variables_from_rows(context, &expanded_rows);
        self.update_result_set_from_rows(context, &expanded_rows);

        Ok(())
    }

    /// Build the emission row for a satisfied iteration, applying
    /// list-promotion to whichever inner variables the body
    /// declared. Returns `None` if the target node fails to
    /// materialise (deleted between BFS and emission, etc.).
    #[allow(clippy::too_many_arguments)]
    fn qpp_build_emission_row(
        &self,
        source_row: &HashMap<String, Value>,
        source_var: &str,
        source_value: &Value,
        target_var: &str,
        target_node: u64,
        path_starts: &[u64],
        path_ends: &[u64],
        inner_rel_var: Option<&str>,
        inner_start_node_var: Option<&str>,
        inner_end_node_var: Option<&str>,
    ) -> Result<Option<HashMap<String, Value>>> {
        let target_value = match self.read_node_as_value(target_node) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        let mut emit = source_row.clone();
        emit.insert(source_var.to_string(), source_value.clone());
        emit.insert(target_var.to_string(), target_value);

        // List-promote the inner-start nodes (one per iteration —
        // GQL spec: `x[k]` is the start node of iteration `k`).
        if let Some(var) = inner_start_node_var {
            let values: Vec<Value> = path_starts
                .iter()
                .filter_map(|nid| self.read_node_as_value(*nid).ok())
                .collect();
            emit.insert(var.to_string(), Value::Array(values));
        }

        // List-promote the inner-end nodes.
        if let Some(var) = inner_end_node_var {
            let values: Vec<Value> = path_ends
                .iter()
                .filter_map(|nid| self.read_node_as_value(*nid).ok())
                .collect();
            emit.insert(var.to_string(), Value::Array(values));
        }

        // The relationship-variable list is built by the caller
        // since we don't carry rel_ids here — the caller has them
        // in scope.
        let _ = inner_rel_var;

        Ok(Some(emit))
    }

    /// Build the JSON list of relationships for the inner
    /// relationship variable. Mirrors the legacy
    /// `VariableLengthPath` rendering — each entry is the same
    /// `read_relationship_as_value` shape SDKs already know how to
    /// deserialise.
    fn qpp_path_rels_as_value_list(&self, path_rels: &[u64]) -> Result<Vec<Value>> {
        let mut out = Vec::with_capacity(path_rels.len());
        for rel_id in path_rels {
            if let Ok(rel_record) = self.store().read_rel(*rel_id) {
                let info = RelationshipInfo {
                    id: *rel_id,
                    source_id: rel_record.src_id,
                    target_id: rel_record.dst_id,
                    type_id: rel_record.type_id,
                };
                if let Ok(v) = self.read_relationship_as_value(&info) {
                    out.push(v);
                }
            }
        }
        Ok(out)
    }

    /// Inner-node label + property AND-filter. The slice-2 contract
    /// treats labels as set membership and properties as
    /// equality on every key in the filter map. Rejected paths
    /// are dropped from emission but BFS keeps walking past them
    /// — Cypher semantics: a per-iteration predicate prunes the
    /// row, not the traversal.
    fn qpp_path_satisfies_filters(
        &self,
        path_starts: &[u64],
        start_labels: &[String],
        start_properties: Option<&PropertyMap>,
        path_ends: &[u64],
        end_labels: &[String],
        end_properties: Option<&PropertyMap>,
    ) -> Result<bool> {
        for nid in path_starts {
            if !self.qpp_node_matches(*nid, start_labels, start_properties)? {
                return Ok(false);
            }
        }
        for nid in path_ends {
            if !self.qpp_node_matches(*nid, end_labels, end_properties)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Single-node check: every required label must be present and
    /// every required property must compare equal. Property
    /// expressions that cannot be evaluated against a literal at
    /// plan time are accepted (the executor falls back to
    /// always-true) so we don't reject paths over predicates we
    /// cannot evaluate yet — slice 2 keeps the surface narrow.
    fn qpp_node_matches(
        &self,
        node_id: u64,
        labels: &[String],
        properties: Option<&PropertyMap>,
    ) -> Result<bool> {
        if !labels.is_empty() {
            // `read_node_as_value` strips labels from the JSON shape
            // (Neo4j-style flat property object), so we read them
            // directly from the catalog instead.
            let node_record = match self.store().read_node(node_id) {
                Ok(r) => r,
                Err(_) => return Ok(false),
            };
            if node_record.is_deleted() {
                return Ok(false);
            }
            let node_labels = self
                .catalog()
                .get_labels_from_bitmap(node_record.label_bits)?;
            for required in labels {
                if !node_labels.iter().any(|l| l == required) {
                    return Ok(false);
                }
            }
        }
        if let Some(props) = properties {
            let node_value = match self.read_node_as_value(node_id) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            };
            for (key, expected_expr) in &props.properties {
                let Some(expected_literal) = expression_as_literal(expected_expr) else {
                    // Non-literal predicate (`{name: $param}` /
                    // `{name: other.x}`) — slice 2 cannot evaluate
                    // these against an unbound row, so we let the
                    // path through. Slice 5 wires the expression
                    // evaluator into the filter.
                    continue;
                };
                let actual = node_value.get(key).cloned().unwrap_or(Value::Null);
                if actual != expected_literal {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Check that the inner relationship satisfies the body's
    /// inline property map (`-[:T {weight: 1}]->`).
    fn qpp_relationship_matches_properties(
        &self,
        rel_info: &RelationshipInfo,
        properties: &PropertyMap,
    ) -> Result<bool> {
        let rel_value = match self.read_relationship_as_value(rel_info) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        for (key, expected_expr) in &properties.properties {
            let Some(expected_literal) = expression_as_literal(expected_expr) else {
                continue;
            };
            let actual = rel_value.get(key).cloned().unwrap_or(Value::Null);
            if actual != expected_literal {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Best-effort literal extraction for property-map filters. Returns
/// `None` for anything the slice-2 filter can't evaluate against a
/// row (parameters, property accesses, function calls). The caller
/// treats `None` as "skip this filter" — slice 5 will replace this
/// with a real expression evaluator that has access to the row.
fn expression_as_literal(expr: &parser::Expression) -> Option<Value> {
    match expr {
        parser::Expression::Literal(lit) => match lit {
            parser::Literal::Integer(n) => Some(Value::from(*n)),
            parser::Literal::Float(f) => Some(Value::from(*f)),
            parser::Literal::String(s) => Some(Value::String(s.clone())),
            parser::Literal::Boolean(b) => Some(Value::Bool(*b)),
            parser::Literal::Null => Some(Value::Null),
            _ => None,
        },
        _ => None,
    }
}
