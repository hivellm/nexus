//! `add_relationship_operators` — relationship traversal operator building.

use super::qpp::build_quantified_expand_operator;
use super::*;

impl<'a> QueryPlanner<'a> {
    /// `clause_scope` identifies the `MATCH` clause these patterns come from.
    /// It becomes `Operator::Expand::iso_scope` so relationship isomorphism is
    /// enforced across every hop of one clause — including its comma-separated
    /// parts, which the parser flattens into a single `Pattern` — and never
    /// across clauses. See `Operator::Expand`'s `iso_scope` doc for why the
    /// scope has to travel on the operator.
    pub(super) fn add_relationship_operators(
        &self,
        patterns: &[Pattern],
        is_optional: bool,
        operators: &mut Vec<Operator>,
        previously_bound_vars: &std::collections::HashSet<String>,
        clause_scope: u32,
    ) -> Result<()> {
        let mut tmp_var_counter = 0;

        for pattern in patterns {
            // Isomorphism is vacuous below two hops, and the bookkeeping it
            // needs is an extra row entry — so leave the row shape of a
            // single-hop pattern untouched. Only plain single-hop slots count:
            // a quantified hop is lowered to `VariableLengthPath` /
            // `QuantifiedExpand`, which carry their own trail semantics and
            // never read this scope.
            let single_hop_slots = pattern
                .elements
                .iter()
                .filter(|el| {
                    matches!(el, PatternElement::Relationship(rel) if rel.quantifier.is_none())
                })
                .count();
            let iso_scope = (single_hop_slots >= 2).then_some(clause_scope);

            // Track previous node variable for relationship expansion
            let mut prev_node_var: Option<String> = None;
            // Inline label(s) declared on the node currently tracked by
            // `prev_node_var` (the WRITTEN source of the next hop). Kept in
            // lock-step with `prev_node_var` — see the two update sites
            // below — so a chained hop's `target_labels` can be recovered
            // for the traversal-reversed case (target already bound, see
            // the swap logic further down).
            let mut prev_node_labels: Vec<String> = Vec::new();

            for (idx, element) in pattern.elements.iter().enumerate() {
                match element {
                    PatternElement::Node(node_pattern) => {
                        // Update previous node variable
                        // If node has explicit variable, use it
                        // Otherwise, keep the previous value (from last Expand's target_var)
                        if let Some(var) = &node_pattern.variable {
                            prev_node_var = Some(var.clone());
                            prev_node_labels = node_pattern.labels.clone();
                        }
                        // Don't update prev_node_var if no variable - it should already be set by previous Expand
                    }
                    PatternElement::Relationship(rel) => {
                        let direction = match rel.direction {
                            RelationshipDirection::Outgoing => Direction::Outgoing,
                            RelationshipDirection::Incoming => Direction::Incoming,
                            RelationshipDirection::Both => Direction::Both,
                        };

                        // Determine source and target variables
                        let source_var = prev_node_var.clone().unwrap_or_default();
                        let source_labels = prev_node_labels.clone();

                        // Target will be the next node in the pattern
                        let (target_var, target_labels) = if idx + 1 < pattern.elements.len() {
                            if let PatternElement::Node(next_node) = &pattern.elements[idx + 1] {
                                // If target node has explicit variable, use it
                                // Otherwise, generate temporary variable for chaining
                                let var = next_node.variable.clone().unwrap_or_else(|| {
                                    let tmp_var = format!("__tmp_{}", tmp_var_counter);
                                    tmp_var_counter += 1;
                                    tmp_var
                                });
                                (var, next_node.labels.clone())
                            } else {
                                ("".to_string(), Vec::new())
                            }
                        } else {
                            ("".to_string(), Vec::new())
                        };

                        // Update prev_node_var to the target for next relationship
                        // This ensures multi-hop patterns chain correctly
                        prev_node_var = Some(target_var.clone());
                        prev_node_labels = target_labels.clone();

                        // CRITICAL FIX: For OPTIONAL MATCH, if target is already bound but source is not,
                        // we need to reverse the traversal direction and swap source/target
                        //
                        // The label predicate travels with whichever WRITTEN
                        // node ends up occupying the operator's `target_var`
                        // slot: on the normal (non-reversed) path that is the
                        // pattern's written target; when reversed, the
                        // operator's target_var becomes the pattern's WRITTEN
                        // source var, so its label predicate (`source_labels`)
                        // must travel with it instead — otherwise a reversed
                        // `(a:A)-->(c)` where `c` is already bound would
                        // silently drop `a`'s label constraint.
                        let (
                            final_source_var,
                            final_target_var,
                            final_direction,
                            final_target_labels,
                        ) = if is_optional && !previously_bound_vars.is_empty() {
                            let source_bound = previously_bound_vars.contains(&source_var);
                            let target_bound = previously_bound_vars.contains(&target_var);

                            if target_bound && !source_bound {
                                // Target is bound, source is not - reverse the traversal
                                let reversed_direction = match direction {
                                    Direction::Outgoing => Direction::Incoming,
                                    Direction::Incoming => Direction::Outgoing,
                                    Direction::Both => Direction::Both,
                                };
                                (
                                    target_var.clone(),
                                    source_var.clone(),
                                    reversed_direction,
                                    source_labels,
                                )
                            } else {
                                (
                                    source_var.clone(),
                                    target_var.clone(),
                                    direction,
                                    target_labels,
                                )
                            }
                        } else {
                            (
                                source_var.clone(),
                                target_var.clone(),
                                direction,
                                target_labels,
                            )
                        };

                        // Get type_ids from relationship types (support multiple types like :TYPE1|TYPE2)
                        // CRITICAL FIX: Use get_or_create_type to ensure type exists even if not yet in catalog
                        // This handles cases where relationships are created but type lookup fails
                        let type_ids: Vec<u32> =
                            rel.types
                                .iter()
                                .filter_map(|type_name| {
                                    // Try get_type_id first (faster), fallback to get_or_create_type if not found
                                    self.catalog.get_type_id(type_name).ok().flatten().or_else(
                                        || {
                                            // Type might not exist yet - create it to ensure lookup works
                                            self.catalog.get_or_create_type(type_name).ok()
                                        },
                                    )
                                })
                                .collect();

                        // Check if this is a variable-length path (has quantifier)
                        if let Some(quantifier) = &rel.quantifier {
                            // Slice-3b §6.5 — opt-in rewrite of legacy
                            // `*m..n` to `QuantifiedExpand` so both
                            // operators share a single execution
                            // path. Off by default because the §9.4
                            // regression gate
                            // (`scripts/bench/qpp-regression-gate.sh`)
                            // has not yet certified perf parity on
                            // every workload — flipping the default
                            // before that bench passes would risk a
                            // regression on the legacy `*m..n`
                            // surface that has shipped since 1.0.
                            // Operators can opt in for testing by
                            // setting `NEXUS_QPP_REWRITE_LEGACY=1`.
                            //
                            // Once the gate is green, the env-var
                            // check flips to a config knob with the
                            // same name, and the legacy
                            // `Operator::VariableLengthPath` arm
                            // above gets retired.
                            let rewrite_to_qpp = qpp_legacy_rewrite_enabled();

                            if rewrite_to_qpp {
                                let (min_length, max_length) = match quantifier {
                                    RelationshipQuantifier::ZeroOrMore => (0, usize::MAX),
                                    RelationshipQuantifier::OneOrMore => (1, usize::MAX),
                                    RelationshipQuantifier::ZeroOrOne => (0, 1),
                                    RelationshipQuantifier::Exact(n) => (*n, *n),
                                    RelationshipQuantifier::Range(min, max) => (*min, *max),
                                };
                                let hop = crate::executor::types::QppHopSpec {
                                    type_ids: type_ids.clone(),
                                    direction,
                                    var: rel.variable.clone(),
                                    properties: rel.properties.clone(),
                                };
                                let glue_node = crate::executor::types::QppNodeSpec {
                                    var: None,
                                    labels: Vec::new(),
                                    properties: None,
                                };
                                operators.push(Operator::QuantifiedExpand {
                                    source_var: final_source_var,
                                    target_var: final_target_var,
                                    hops: vec![hop],
                                    inner_nodes: vec![glue_node.clone(), glue_node],
                                    inner_where: None,
                                    min_length,
                                    max_length,
                                    optional: is_optional,
                                    mode: crate::executor::types::QppMode::Walk,
                                });
                            } else {
                                // Use VariableLengthPath operator for
                                // variable-length paths. Default path
                                // until §9.4 certifies parity.
                                let path_var = pattern.path_variable.clone().unwrap_or_default();
                                operators.push(Operator::VariableLengthPath {
                                    type_ids: type_ids.clone(),
                                    direction,
                                    source_var,
                                    target_var,
                                    rel_var: rel.variable.clone().unwrap_or_default(),
                                    path_var,
                                    quantifier: quantifier.clone(),
                                });
                            }
                        } else {
                            // Use regular Expand operator for single-hop relationships
                            operators.push(Operator::Expand {
                                type_ids,
                                source_var: final_source_var,
                                target_var: final_target_var,
                                rel_var: rel.variable.clone().unwrap_or_default(),
                                direction: final_direction,
                                optional: is_optional,
                                target_labels: final_target_labels,
                                iso_scope,
                            });
                        }
                    }
                    PatternElement::QuantifiedGroup(group) => {
                        // Slice-2 entry point. The slice-1 lowering
                        // already handled groups with anonymous
                        // boundary nodes; whatever lands here has
                        // either a named/labelled boundary, an
                        // inner property filter the lowering cannot
                        // attach, or a multi-hop body the slice-2
                        // operator cannot drive yet.
                        let qpp = build_quantified_expand_operator(
                            group,
                            &mut prev_node_var,
                            &pattern.elements,
                            idx,
                            is_optional,
                            &mut tmp_var_counter,
                            &self.catalog,
                        )?;
                        operators.push(qpp);
                    }
                }
            }
        }

        Ok(())
    }
}
