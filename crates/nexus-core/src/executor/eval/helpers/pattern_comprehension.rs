//! Full-enumeration walk for pattern comprehensions
//! (`[(a)-[:T]->(b) | b.name]`, `[p = (a)-->(b) | p]`) and any other
//! construct that needs every complete pattern binding rather than a
//! first-witness probe.
//!
//! Shares the `EXISTS` anchor-resolution, candidate-acceptance, and
//! variable-length-walk machinery in [`super::exists`] (widened to
//! `pub(super)` there) and diverges only at the recursion's base case:
//! instead of short-circuiting on the first witness, every complete
//! binding is projected through a caller-supplied closure and the
//! resulting `Value` alone is retained — the walk backtracks
//! immediately afterward. Relationship isomorphism, label/property
//! constraints, correlated-variable re-use, and the variable-length
//! hop's depth/clamp semantics are therefore identical to
//! `EXISTS { … }` by construction, not by parallel maintenance.
//!
//! # Streaming, not materialising
//!
//! [`Executor::collect_pattern_bindings`] does not build a
//! `Vec` of intermediate bindings before projecting them: the walk
//! calls the `project` closure exactly once per complete match, at the
//! moment the match is found, handing it the match's own binding map
//! (moved, not cloned — nothing after this point needs it) and its
//! traversal trail. Only the closure's *result* — typically a small
//! scalar or object, never the full row — is retained, one entry per
//! match, guarded by the same [`crate::executor::MAX_INTERMEDIATE_ROWS`]
//! ceiling every other row-collecting operator respects
//! ([`push_with_row_cap`]). An uncorrelated pattern comprehension over a
//! dense graph therefore surfaces a deterministic `Error::OutOfMemory`
//! instead of an unbounded allocation.
//!
//! A pattern anchored on a `NULL`-valued correlated outer variable — the
//! only place `EXISTS` would propagate three-valued `NULL` — instead
//! yields zero bindings here: a comprehension has no boolean result to
//! carry `NULL` through, and `[x IN NULL | …]` already yields `[]` for
//! Cypher's `ListComprehension`, so a pattern comprehension mirrors that
//! for consistency.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::operators::path::{MAX_VAR_LENGTH_PATH_DEPTH, Path};
use super::super::super::parser;
use super::super::super::push_with_row_cap;
use super::super::super::types::Direction;
use super::exists::{
    ExistsAcceptOutcome, ExistsAnchor, ExistsVarLengthWalk,
    var_length_rel_variable_not_implemented_error,
};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

impl Executor {
    /// Enumerate every complete binding of `pattern` against live graph
    /// state, starting from `row`'s already-bound variables (correlated
    /// subquery semantics — identical to `EXISTS`'s anchor resolution),
    /// streaming each match through `project` and collecting only the
    /// projected `Value`s (see the module doc comment for why this
    /// matters).
    ///
    /// `project` receives the match's own binding map (the pattern's
    /// variables plus everything already in `row`) by value and the
    /// match's traversal trail by reference; the latter is a real,
    /// freshly cloned [`Path`] only when `needs_trail` is `true`
    /// (typically because the comprehension carries a `p = pattern`
    /// path-binding variable the caller must materialise via
    /// [`Executor::path_to_value`]) — otherwise an empty, allocation-free
    /// [`Path`] is passed, since cloning the trail on every match only
    /// to have the caller ignore it would itself be an unbounded-work
    /// hazard on a long path.
    ///
    /// Unlike [`Self::evaluate_exists_pattern`], this does not
    /// short-circuit: every witness the depth-first walk finds is
    /// projected (subject to `where_clause`, evaluated per candidate
    /// binding — same per-candidate timing as `EXISTS`'s inner `WHERE`)
    /// and the walk backtracks to look for the next one. Relationship
    /// isomorphism holds within each individual binding but not across
    /// bindings — two different result elements may legitimately reuse
    /// the same relationship id.
    ///
    /// Returns an empty list — never an error — for an empty pattern, an
    /// unmatched pattern, or a pattern anchored on a `NULL`-valued
    /// correlated variable. Returns `Err` for a quantified (QPP) element
    /// anywhere in `pattern.elements` unconditionally — checked once,
    /// up front, so the error surfaces regardless of whether the graph
    /// data would ever let the walk actually reach that element (a
    /// zero-candidate anchor must not silently swallow an unsupported
    /// pattern shape into `[]`).
    pub(in crate::executor) fn collect_pattern_bindings<F>(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        pattern: &parser::Pattern,
        where_clause: Option<&parser::Expression>,
        needs_trail: bool,
        mut project: F,
    ) -> Result<Vec<Value>>
    where
        F: FnMut(HashMap<String, Value>, &Path) -> Result<Value>,
    {
        if pattern.elements.is_empty() {
            // Defensive-only: the parser never produces an empty pattern.
            return Ok(Vec::new());
        }
        if pattern
            .elements
            .iter()
            .any(|e| matches!(e, parser::PatternElement::QuantifiedGroup(_)))
        {
            return Err(Error::CypherExecution(
                "ERR_QPP_NOT_IMPLEMENTED: quantified path patterns inside pattern \
                 comprehensions need the QPP operator"
                    .to_string(),
            ));
        }
        let mut results = Vec::new();
        let mut bound_relationships: HashSet<u64> = HashSet::new();
        let mut trail_nodes: Vec<u64> = Vec::new();
        let mut trail_relationships: Vec<u64> = Vec::new();
        self.collect_pattern_bindings_walk(
            context,
            &pattern.elements,
            0,
            row.clone(),
            None,
            &mut bound_relationships,
            &mut trail_nodes,
            &mut trail_relationships,
            where_clause,
            needs_trail,
            &mut project,
            &mut results,
        )?;
        Ok(results)
    }

    /// Depth-first walk of a pattern comprehension's element list,
    /// starting at `elements[pos]`. Structurally identical to
    /// [`Self::exists_probe`] (see its doc comment for the anchor /
    /// isomorphism / comma-separated-component rules, which apply here
    /// unchanged) except every complete binding reached at the base
    /// case is handed to `project` and only its result pushed onto
    /// `results` (row-capped), instead of short-circuiting the search.
    ///
    /// `trail_nodes` / `trail_relationships` mirror `bound_relationships`:
    /// pushed immediately before recursing into a newly accepted
    /// node/hop and popped again on backtrack, so at the base case they
    /// hold exactly the ordered id sequence of the current witness.
    #[allow(clippy::too_many_arguments)]
    fn collect_pattern_bindings_walk<F>(
        &self,
        context: &ExecutionContext,
        elements: &[parser::PatternElement],
        pos: usize,
        binding: HashMap<String, Value>,
        anchor_id: Option<u64>,
        bound_relationships: &mut HashSet<u64>,
        trail_nodes: &mut Vec<u64>,
        trail_relationships: &mut Vec<u64>,
        where_clause: Option<&parser::Expression>,
        needs_trail: bool,
        project: &mut F,
        results: &mut Vec<Value>,
    ) -> Result<()>
    where
        F: FnMut(HashMap<String, Value>, &Path) -> Result<Value>,
    {
        let Some(element) = elements.get(pos) else {
            // Pattern fully walked — project this witness iff it also
            // satisfies the inner WHERE (vacuously true when there is
            // none), exactly like `exists_probe`'s base case.
            let include = match where_clause {
                Some(expr) => self.evaluate_predicate_on_row(&binding, context, expr)?,
                None => true,
            };
            if include {
                // Only pay for the O(depth) trail clone when the
                // caller actually needs it (a `p = pattern` binding
                // variable) — an unused clone on every match would
                // itself be unbounded work on a long path.
                let trail = if needs_trail {
                    Path {
                        nodes: trail_nodes.clone(),
                        relationships: trail_relationships.clone(),
                    }
                } else {
                    Path {
                        nodes: Vec::new(),
                        relationships: Vec::new(),
                    }
                };
                let value = project(binding, &trail)?;
                push_with_row_cap(results, value, "pattern comprehension")?;
            }
            return Ok(());
        };

        match element {
            parser::PatternElement::Node(node) => {
                let candidates = match self.exists_resolve_anchor(&binding, context, node)? {
                    ExistsAnchor::Null => return Ok(()),
                    ExistsAnchor::Ids(ids) => ids,
                };
                for candidate_id in candidates {
                    match self.exists_accept_node_candidate(
                        &binding,
                        context,
                        node,
                        candidate_id,
                    )? {
                        ExistsAcceptOutcome::Rejected | ExistsAcceptOutcome::Null => {}
                        ExistsAcceptOutcome::Accepted(next_binding) => {
                            trail_nodes.push(candidate_id);
                            self.collect_pattern_bindings_walk(
                                context,
                                elements,
                                pos + 1,
                                next_binding,
                                Some(candidate_id),
                                bound_relationships,
                                trail_nodes,
                                trail_relationships,
                                where_clause,
                                needs_trail,
                                project,
                                results,
                            )?;
                            trail_nodes.pop();
                        }
                    }
                }
                Ok(())
            }
            parser::PatternElement::Relationship(rel) => {
                let Some(anchor_id) = anchor_id else {
                    // A Relationship can only follow a Node in a
                    // well-formed pattern; defensive-only.
                    return Ok(());
                };
                let Some(parser::PatternElement::Node(next_node)) = elements.get(pos + 1) else {
                    // The grammar always pairs a relationship with a
                    // following node; defensive-only.
                    return Ok(());
                };

                // Read-only catalog lookup, never `get_or_create_type` —
                // same rationale as `exists_probe`'s Relationship arm:
                // this is a read path and must not intern new type ids.
                let mut type_ids: Vec<u32> = Vec::with_capacity(rel.types.len());
                for type_name in &rel.types {
                    if let Some(id) = self.catalog().get_type_id(type_name)? {
                        type_ids.push(id);
                    }
                }
                let direction = match rel.direction {
                    parser::RelationshipDirection::Outgoing => Direction::Outgoing,
                    parser::RelationshipDirection::Incoming => Direction::Incoming,
                    parser::RelationshipDirection::Both => Direction::Both,
                };

                if let Some(quantifier) = &rel.quantifier {
                    // Mirrors `exists_probe`'s identical restriction: a
                    // named variable on a variable-length hop binds a
                    // LIST<RELATIONSHIP> in full Cypher, which this walk
                    // (like the EXISTS probe it shares machinery with)
                    // does not materialise.
                    if rel.variable.is_some() {
                        return Err(var_length_rel_variable_not_implemented_error(
                            "a pattern comprehension",
                        ));
                    }
                    let (min_hops, max_hops) = match quantifier {
                        // openCypher defines a bare `*` as `*1..` (one or
                        // more) — see `exists_probe`'s identical comment
                        // for the full rationale.
                        parser::RelationshipQuantifier::ZeroOrMore => (1, usize::MAX),
                        parser::RelationshipQuantifier::OneOrMore => (1, usize::MAX),
                        parser::RelationshipQuantifier::ZeroOrOne => (0, 1),
                        parser::RelationshipQuantifier::Exact(n) => (*n, *n),
                        parser::RelationshipQuantifier::Range(min, max) => (*min, *max),
                    };
                    let max_hops = max_hops.min(MAX_VAR_LENGTH_PATH_DEPTH);
                    let can_extend = rel.types.is_empty() || !type_ids.is_empty();
                    let walk = ExistsVarLengthWalk {
                        context,
                        elements,
                        pos,
                        rel,
                        next_node,
                        min_hops,
                        max_hops,
                        can_extend,
                        type_ids: &type_ids,
                        direction,
                        where_clause,
                    };
                    return self.collect_pattern_bindings_var_length(
                        &walk,
                        binding,
                        anchor_id,
                        0,
                        bound_relationships,
                        trail_nodes,
                        trail_relationships,
                        needs_trail,
                        project,
                        results,
                    );
                }

                if !rel.types.is_empty() && type_ids.is_empty() {
                    return Ok(());
                }

                // Authoritative store adjacency — never
                // `relationship_index()`, which is not authoritative for
                // correctness.
                let relationships =
                    self.find_relationships(anchor_id, &type_ids, direction, None)?;
                for rel_info in &relationships {
                    if bound_relationships.contains(&rel_info.id) {
                        continue;
                    }
                    if !self.exists_relationship_properties_match(
                        &binding,
                        context,
                        rel.properties.as_ref(),
                        rel_info,
                    )? {
                        continue;
                    }

                    let mut hop_binding = binding.clone();
                    if let Some(var) = &rel.variable {
                        if let Some(existing) = hop_binding.get(var) {
                            match Self::extract_entity_id(existing) {
                                Some(existing_id) if existing_id == rel_info.id => {}
                                _ => continue,
                            }
                        } else {
                            hop_binding
                                .insert(var.clone(), self.read_relationship_as_value(rel_info)?);
                        }
                    }

                    let target_id = match direction {
                        Direction::Outgoing => rel_info.target_id,
                        Direction::Incoming => rel_info.source_id,
                        Direction::Both => {
                            if rel_info.source_id == anchor_id {
                                rel_info.target_id
                            } else {
                                rel_info.source_id
                            }
                        }
                    };

                    match self.exists_accept_node_candidate(
                        &hop_binding,
                        context,
                        next_node,
                        target_id,
                    )? {
                        ExistsAcceptOutcome::Rejected | ExistsAcceptOutcome::Null => {}
                        ExistsAcceptOutcome::Accepted(next_binding) => {
                            bound_relationships.insert(rel_info.id);
                            trail_relationships.push(rel_info.id);
                            trail_nodes.push(target_id);
                            self.collect_pattern_bindings_walk(
                                context,
                                elements,
                                pos + 2,
                                next_binding,
                                Some(target_id),
                                bound_relationships,
                                trail_nodes,
                                trail_relationships,
                                where_clause,
                                needs_trail,
                                project,
                                results,
                            )?;
                            trail_nodes.pop();
                            trail_relationships.pop();
                            bound_relationships.remove(&rel_info.id);
                        }
                    }
                }
                Ok(())
            }
            parser::PatternElement::QuantifiedGroup(_) => {
                // Unreachable in practice: `collect_pattern_bindings`
                // pre-scans `pattern.elements` for this variant and
                // errors unconditionally before the walk ever starts
                // (see its doc comment) — kept as a defensive fallback,
                // never a data-dependent path.
                Err(Error::CypherExecution(
                    "ERR_QPP_NOT_IMPLEMENTED: quantified path patterns inside pattern \
                     comprehensions need the QPP operator"
                        .to_string(),
                ))
            }
        }
    }

    /// Depth-first walk of a variable-length relationship hop
    /// (`-[:T*min..max]->`) inside a pattern comprehension. Structurally
    /// identical to [`Self::exists_probe_var_length`] (see its doc
    /// comment for the zero-length / clamp / isomorphism rules, which
    /// apply here unchanged) except every accepted target backtracks
    /// into [`Self::collect_pattern_bindings_walk`] instead of
    /// short-circuiting, and each taken edge pushes onto the shared
    /// trail for the duration of its recursive extension.
    #[allow(clippy::too_many_arguments)]
    fn collect_pattern_bindings_var_length<F>(
        &self,
        walk: &ExistsVarLengthWalk<'_>,
        binding: HashMap<String, Value>,
        current_id: u64,
        depth: usize,
        bound_relationships: &mut HashSet<u64>,
        trail_nodes: &mut Vec<u64>,
        trail_relationships: &mut Vec<u64>,
        needs_trail: bool,
        project: &mut F,
        results: &mut Vec<Value>,
    ) -> Result<()>
    where
        F: FnMut(HashMap<String, Value>, &Path) -> Result<Value>,
    {
        if depth >= walk.min_hops {
            match self.exists_accept_node_candidate(
                &binding,
                walk.context,
                walk.next_node,
                current_id,
            )? {
                ExistsAcceptOutcome::Rejected | ExistsAcceptOutcome::Null => {}
                ExistsAcceptOutcome::Accepted(next_binding) => {
                    self.collect_pattern_bindings_walk(
                        walk.context,
                        walk.elements,
                        walk.pos + 2,
                        next_binding,
                        Some(current_id),
                        bound_relationships,
                        trail_nodes,
                        trail_relationships,
                        walk.where_clause,
                        needs_trail,
                        project,
                        results,
                    )?;
                }
            }
        }

        if walk.can_extend && depth < walk.max_hops {
            // Authoritative store adjacency — never `relationship_index()`.
            let relationships =
                self.find_relationships(current_id, walk.type_ids, walk.direction, None)?;
            for rel_info in &relationships {
                if bound_relationships.contains(&rel_info.id) {
                    continue;
                }
                if !self.exists_relationship_properties_match(
                    &binding,
                    walk.context,
                    walk.rel.properties.as_ref(),
                    rel_info,
                )? {
                    continue;
                }

                let hop_target = match walk.direction {
                    Direction::Outgoing => rel_info.target_id,
                    Direction::Incoming => rel_info.source_id,
                    Direction::Both => {
                        if rel_info.source_id == current_id {
                            rel_info.target_id
                        } else {
                            rel_info.source_id
                        }
                    }
                };

                bound_relationships.insert(rel_info.id);
                trail_relationships.push(rel_info.id);
                trail_nodes.push(hop_target);
                self.collect_pattern_bindings_var_length(
                    walk,
                    binding.clone(),
                    hop_target,
                    depth + 1,
                    bound_relationships,
                    trail_nodes,
                    trail_relationships,
                    needs_trail,
                    project,
                    results,
                )?;
                trail_nodes.pop();
                trail_relationships.pop();
                bound_relationships.remove(&rel_info.id);
            }
        }

        Ok(())
    }
}
