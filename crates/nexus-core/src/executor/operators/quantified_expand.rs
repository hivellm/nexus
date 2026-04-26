//! Quantified Path Pattern executor —
//! `phase6_opencypher-quantified-path-patterns`.
//!
//! Handles the dedicated `Operator::QuantifiedExpand`, which the
//! planner emits when a `QuantifiedGroup` survives the slice-1
//! lowering (anything with named/labelled inner boundary nodes,
//! relationship-property filters that cannot live on a legacy
//! `*m..n` form, multi-hop bodies, etc.). The body shape is
//! arbitrary-arity — `hops.len() == n` means each iteration walks
//! `n` relationships in order.
//!
//! ## Execution model
//!
//! For each input row carrying `source_var`, BFS hops the body
//! once per iteration. Each frame tracks:
//!
//! - `current_node`: the node we land on after `k` complete
//!   iterations
//! - `iteration`: number of body-walks taken so far
//! - per-position node lists and per-hop relationship lists,
//!   one entry per iteration, used for list-promotion when the
//!   body declares boundary-node or relationship variables
//!
//! When `iteration` is in `[min_length, max_length]` and every
//! per-iteration filter accepted, the operator emits one row with
//! the outer `target_var` bound to `current_node`, every
//! list-promoted variable bound to its accumulated `Vec<Value>`,
//! and the source row's existing bindings carried through.
//!
//! Cycle policy is hard-coded to `NODES_CAN_REPEAT` (the MATCH
//! default; `shortestPath`/`allShortestPaths` over QPP are slice
//! 5 work).

use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::Result;
use crate::executor::Executor;
use crate::executor::context::{ExecutionContext, RelationshipInfo};
use crate::executor::parser::{self, PropertyMap};
use crate::executor::push_with_row_cap;
use crate::executor::types::{Direction, QppHopSpec, QppNodeSpec};

/// Per-query safety cap on iteration depth when the quantifier is
/// unbounded (`*` / `+` / `{m,}`). Picking `64` mirrors the
/// `max_qpp_depth` budget called out in the design doc and keeps
/// per-frame memory tractable.
const MAX_QPP_DEPTH: usize = 64;

impl Executor {
    /// Execute one `QuantifiedExpand` operator. See module-level
    /// docs for the BFS shape.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor) fn execute_quantified_expand(
        &self,
        context: &mut ExecutionContext,
        source_var: &str,
        target_var: &str,
        hops: &[QppHopSpec],
        inner_nodes: &[QppNodeSpec],
        min_length: usize,
        max_length: usize,
        optional: bool,
    ) -> Result<()> {
        debug_assert_eq!(
            inner_nodes.len(),
            hops.len() + 1,
            "QuantifiedExpand invariant: inner_nodes.len() must be hops.len() + 1"
        );

        let effective_max = max_length.min(MAX_QPP_DEPTH);
        // True when the user wrote an unbounded quantifier (`*` /
        // `+` / `{m,}`) or a `{m,n}` whose `n` exceeds
        // `MAX_QPP_DEPTH`. Tracked so we can surface
        // `ERR_QPP_UNBOUND_UPPER` once we've actually reached the
        // cap with more candidates pending — silently truncating
        // those candidates is the failure mode the error code
        // exists to make visible.
        let user_unbounded = max_length > MAX_QPP_DEPTH;
        let mut cap_was_hit = false;

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
                        let mut new_row = row.clone();
                        new_row.insert(source_var.to_string(), source_value.clone());
                        new_row.insert(target_var.to_string(), Value::Null);
                        for spec in inner_nodes {
                            if let Some(var) = &spec.var {
                                new_row.insert(var.to_string(), Value::Array(Vec::new()));
                            }
                        }
                        for hop in hops {
                            if let Some(var) = &hop.var {
                                new_row.insert(var.to_string(), Value::Array(Vec::new()));
                            }
                        }
                        push_with_row_cap(&mut expanded_rows, new_row, "QuantifiedExpand")?;
                    }
                    continue;
                }
            };

            // BFS frame:
            //   (current_node, iteration_count, nodes_per_position, rels_per_hop)
            // - `nodes_per_position[p]` = list of nodes seen at body
            //   position p, one per iteration. Length always equals
            //   `iteration_count` for `p < hops.len()`, and
            //   `iteration_count` for the closing position too
            //   (every accepted iteration appends one entry to every
            //   position list).
            // - `rels_per_hop[h]` = same shape for each hop.
            type Frame = (u64, usize, Vec<Vec<u64>>, Vec<Vec<u64>>);
            let mut queue: VecDeque<Frame> = VecDeque::new();
            queue.push_back((
                source_id,
                0,
                vec![Vec::new(); inner_nodes.len()],
                vec![Vec::new(); hops.len()],
            ));

            // Wavefront dedup: `(node, iteration)`. Cycle policy is
            // NODES_CAN_REPEAT, so we only collapse paths that share
            // both endpoint and iteration count.
            let mut visited: HashSet<(u64, usize)> = HashSet::new();
            visited.insert((source_id, 0));

            while let Some((current_node, iteration, nodes_per_pos, rels_per_hop)) =
                queue.pop_front()
            {
                if iteration >= min_length
                    && iteration <= effective_max
                    && self.qpp_lists_satisfy_filters(&nodes_per_pos, inner_nodes)?
                {
                    let mut emit = row.clone();
                    emit.insert(source_var.to_string(), source_value.clone());

                    let target_value = match self.read_node_as_value(current_node) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    emit.insert(target_var.to_string(), target_value);

                    for (idx, spec) in inner_nodes.iter().enumerate() {
                        if let Some(var) = &spec.var {
                            let values: Vec<Value> = nodes_per_pos[idx]
                                .iter()
                                .filter_map(|nid| self.read_node_as_value(*nid).ok())
                                .collect();
                            emit.insert(var.to_string(), Value::Array(values));
                        }
                    }
                    for (idx, hop) in hops.iter().enumerate() {
                        if let Some(var) = &hop.var {
                            let values = self.qpp_path_rels_as_value_list(&rels_per_hop[idx])?;
                            emit.insert(var.to_string(), Value::Array(values));
                        }
                    }

                    push_with_row_cap(&mut expanded_rows, emit, "QuantifiedExpand")?;
                }

                if iteration >= effective_max {
                    continue;
                }

                // Walk one full iteration of the body starting from
                // `current_node`, collecting every reachable end
                // node together with the per-hop relationships and
                // intermediate nodes traversed. Each successful walk
                // becomes a new BFS frame at `iteration + 1`.
                let mut walks: Vec<(u64, Vec<u64>, Vec<u64>)> = Vec::new();
                self.qpp_walk_body(
                    current_node,
                    hops,
                    inner_nodes,
                    &mut Vec::new(),
                    &mut Vec::new(),
                    0,
                    &mut walks,
                )?;

                // The cap is reached when the next iteration would
                // be `MAX_QPP_DEPTH + 1` AND there are still walks
                // pending — i.e. we're about to silently drop
                // candidates the user told us to walk. Tracking
                // this here (rather than after the loop) keeps the
                // signal precise: we only flag truncation that
                // actually discarded data, not queries that simply
                // ran out of graph before the cap.
                if iteration + 1 == effective_max && !walks.is_empty() && user_unbounded {
                    cap_was_hit = true;
                }

                for (end_node, hop_nodes_intermediate, hop_rels) in walks {
                    let next_iteration = iteration + 1;
                    let mut new_nodes_per_pos = nodes_per_pos.clone();
                    // Position 0 is the START of this iteration.
                    new_nodes_per_pos[0].push(current_node);
                    // Positions 1..n are the intermediate nodes
                    // landed on after each hop except the last; the
                    // last position holds the iteration's end node.
                    for (i, intermediate_node) in hop_nodes_intermediate.iter().enumerate() {
                        new_nodes_per_pos[i + 1].push(*intermediate_node);
                    }

                    let mut new_rels_per_hop = rels_per_hop.clone();
                    for (i, rid) in hop_rels.iter().enumerate() {
                        new_rels_per_hop[i].push(*rid);
                    }

                    let visit_key = (end_node, next_iteration);
                    if visited.insert(visit_key) {
                        queue.push_back((
                            end_node,
                            next_iteration,
                            new_nodes_per_pos,
                            new_rels_per_hop,
                        ));
                    }
                }
            }
        }

        if cap_was_hit {
            // ERR_QPP_UNBOUND_UPPER — the user wrote an unbounded
            // quantifier and BFS hit the per-query safety cap with
            // candidates pending, so the result set is truncated.
            // Surface the situation via tracing instead of failing
            // the query: silently dropping candidates is worse for
            // observability, but failing a query that previously
            // succeeded is worse for users on the upgrade path.
            // Operators reading the warning know to either narrow
            // the quantifier or split the query.
            tracing::warn!(
                target: "nexus_core::executor::quantified_expand",
                code = "ERR_QPP_UNBOUND_UPPER",
                max_qpp_depth = MAX_QPP_DEPTH,
                "QPP traversal hit the {} iteration cap with candidates \
                 still pending; result set may be truncated. Bound the \
                 quantifier (`{{m,n}}` instead of `*` / `+` / `{{m,}}`) \
                 to silence this warning.",
                MAX_QPP_DEPTH,
            );
        }

        self.update_variables_from_rows(context, &expanded_rows);
        self.update_result_set_from_rows(context, &expanded_rows);

        Ok(())
    }

    /// Walk one body iteration. Recursive depth-first expansion
    /// from `current_node` through `hops[hop_idx..]`. Collects every
    /// successful walk into `out` as `(end_node, intermediate_nodes,
    /// hop_relationships)`. `intermediate_nodes` records the node
    /// landed on after each hop (so for a 2-hop body, it has 2
    /// entries: the node after hop 0 and after hop 1).
    #[allow(clippy::too_many_arguments)]
    fn qpp_walk_body(
        &self,
        current_node: u64,
        hops: &[QppHopSpec],
        inner_nodes: &[QppNodeSpec],
        cur_intermediate: &mut Vec<u64>,
        cur_rels: &mut Vec<u64>,
        hop_idx: usize,
        out: &mut Vec<(u64, Vec<u64>, Vec<u64>)>,
    ) -> Result<()> {
        if hop_idx == hops.len() {
            // Walked every hop; emit.
            out.push((current_node, cur_intermediate.clone(), cur_rels.clone()));
            return Ok(());
        }

        let hop = &hops[hop_idx];
        let neighbors =
            self.find_relationships(current_node, &hop.type_ids, hop.direction, None)?;
        for rel in neighbors {
            if let Some(props) = &hop.properties
                && !self.qpp_relationship_matches_properties(&rel, props)?
            {
                continue;
            }
            let next_node = match hop.direction {
                Direction::Outgoing => rel.target_id,
                Direction::Incoming => rel.source_id,
                Direction::Both => {
                    if rel.source_id == current_node {
                        rel.target_id
                    } else {
                        rel.source_id
                    }
                }
            };

            // Apply the per-position node filter for the node
            // we're about to land on. The slot index is
            // `hop_idx + 1` because position 0 is the START of the
            // iteration (filtered separately when we walked into
            // it). The closing position (`hops.len()`) is filtered
            // here on the last hop.
            let target_spec = &inner_nodes[hop_idx + 1];
            if !self.qpp_node_matches(
                next_node,
                &target_spec.labels,
                target_spec.properties.as_ref(),
            )? {
                continue;
            }

            cur_intermediate.push(next_node);
            cur_rels.push(rel.id);
            self.qpp_walk_body(
                next_node,
                hops,
                inner_nodes,
                cur_intermediate,
                cur_rels,
                hop_idx + 1,
                out,
            )?;
            cur_intermediate.pop();
            cur_rels.pop();
        }

        Ok(())
    }

    /// Apply the per-position label / property filter to every
    /// node accumulated for that position. A single iteration
    /// failing the filter rejects the whole frame.
    fn qpp_lists_satisfy_filters(
        &self,
        nodes_per_pos: &[Vec<u64>],
        specs: &[QppNodeSpec],
    ) -> Result<bool> {
        for (slot, spec) in specs.iter().enumerate() {
            if spec.labels.is_empty() && spec.properties.is_none() {
                continue;
            }
            for nid in &nodes_per_pos[slot] {
                if !self.qpp_node_matches(*nid, &spec.labels, spec.properties.as_ref())? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Build the JSON list of relationships for an inner-hop
    /// relationship variable. Mirrors the legacy `VariableLengthPath`
    /// rendering — same `read_relationship_as_value` shape SDKs
    /// already know how to deserialise.
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

    /// Single-node check: every required label must be present and
    /// every required property must compare equal. Property
    /// expressions that cannot be evaluated against a literal at
    /// plan time are accepted (the executor falls back to
    /// always-true) so we don't reject paths over predicates we
    /// cannot evaluate yet — slice 5 wires the expression evaluator
    /// in.
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

    /// Check that a relationship satisfies a body's inline property
    /// map (`-[:T {weight: 1}]->`).
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
/// `None` for anything the slice-3 filter can't evaluate against a
/// row (parameters, property accesses, function calls). Slice 5
/// will replace this with a real expression evaluator.
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

// `HashMap` import kept only for inline use elsewhere in the
// executor; quiet the unused-import lint without removing the
// path so a future helper can reach for it.
#[allow(dead_code)]
fn _hashmap_marker(_: HashMap<String, Value>) {}
