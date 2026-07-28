//! `EXISTS { … }` pattern-probe machinery: anchor resolution for the
//! pattern's first (or a freshly-started comma-separated) node,
//! candidate acceptance against label/property/identity constraints,
//! and the depth-first witness search — including its variable-length
//! relationship walk. Extracted from `eval/helpers.rs`.

use super::super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::super::engine::Executor;
use super::super::super::operators::path::MAX_VAR_LENGTH_PATH_DEPTH;
use super::super::super::parser;
use super::super::super::types::Direction;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Shared error for a named relationship variable on a variable-length
/// hop inside a construct (`EXISTS { … }`, a pattern comprehension)
/// that only ever materialises a single relationship value per hop —
/// binding it as `LIST<RELATIONSHIP>` (full Cypher's semantics for a
/// named var-length relationship variable) is not supported.
/// `context_word` names the construct in the message, e.g. `"EXISTS"`
/// or `"a pattern comprehension"`.
///
/// `pub(super)`: shared by [`Executor::exists_probe`] and the
/// pattern-comprehension walk (`eval::helpers::pattern_comprehension`)
/// instead of duplicating the message text.
pub(super) fn var_length_rel_variable_not_implemented_error(context_word: &str) -> Error {
    Error::CypherExecution(format!(
        "ERR_VAR_LENGTH_REL_VARIABLE_NOT_IMPLEMENTED: a named relationship variable on a \
         variable-length relationship inside {context_word} is not supported (it binds a \
         LIST<RELATIONSHIP> in full Cypher); use an anonymous variable-length relationship \
         instead"
    ))
}

/// Candidate-resolution outcome for the node that anchors a fresh
/// component of an `EXISTS { … }` pattern probe (the pattern's first
/// element, or a node that starts a new comma-separated pattern part).
/// `Null` signals that the anchor is correlated to an outer variable
/// that is bound but currently `NULL` — under Cypher's three-valued
/// logic this must propagate a `NULL` result rather than `false`.
///
/// `pub(super)` (not private) so the pattern-comprehension full
/// enumeration walk (`eval::helpers::pattern_comprehension`) shares
/// this exact anchor-resolution outcome instead of duplicating it.
pub(super) enum ExistsAnchor {
    Null,
    Ids(Vec<u64>),
}

/// Outcome of testing a single candidate node against a pattern node's
/// constraints (labels, properties, and — when the pattern reuses an
/// already-bound variable name — identity with the existing binding).
///
/// `pub(super)` for the same reason as [`ExistsAnchor`] — shared with
/// the pattern-comprehension full enumeration walk.
pub(super) enum ExistsAcceptOutcome {
    /// The candidate satisfies every constraint; carries the row
    /// extended with the node's variable (unchanged if it was already
    /// bound).
    Accepted(HashMap<String, Value>),
    /// The candidate fails a structural constraint (deleted, wrong
    /// label, property mismatch, or conflicts with an existing
    /// same-name binding).
    Rejected,
    /// The pattern reuses a variable name that is already bound in the
    /// row to `NULL` (e.g. correlated to an unmatched `OPTIONAL
    /// MATCH`) — under three-valued logic the candidate's fate is
    /// unknown, not rejected.
    Null,
}

/// Three-valued result of probing an `EXISTS` pattern (or a sub-step of
/// one): `True` once a witness binding is found, `False` when the
/// search is exhausted without one, and `Null` when a required
/// correlated variable was `NULL` along every path that was tried and
/// no path produced `True`. Mirrors Cypher's `NULL` propagation for
/// pattern predicates: `NULL` composes correctly under `NOT` and is
/// filtered out (treated as not-true) by `WHERE`, exactly like any
/// other `NULL` boolean expression.
enum ExistsOutcome {
    True,
    False,
    Null,
}

impl ExistsOutcome {
    fn into_value(self) -> Value {
        match self {
            Self::True => Value::Bool(true),
            Self::False => Value::Bool(false),
            Self::Null => Value::Null,
        }
    }
}

/// Loop-invariant arguments for [`Executor::exists_probe_var_length`]'s
/// depth-first walk of a variable-length relationship hop. Everything
/// here stays fixed across the whole recursive walk of one `-[:T*m..n]->`
/// segment; only the binding, frontier node id, depth, and the shared
/// `bound_relationships` set change per recursive call. Bundled into one
/// borrowed struct so the recursive function itself stays under
/// clippy's argument-count lint without duplicating any of these values.
///
/// `pub(super)` (struct and fields) so the pattern-comprehension full
/// enumeration walk (`eval::helpers::pattern_comprehension`) can build
/// the identical loop-invariant bundle and drive its own depth-first
/// walk of the same variable-length segment shape, instead of
/// duplicating the quantifier/type-id resolution that produces it.
pub(super) struct ExistsVarLengthWalk<'a> {
    pub(super) context: &'a ExecutionContext,
    pub(super) elements: &'a [parser::PatternElement],
    /// Index of the `Relationship` element itself within `elements` —
    /// the following node lives at `elements[pos + 1]`
    /// (`next_node`, cached separately below) and the rest of the
    /// pattern resumes at `pos + 2` once a witness for this segment is
    /// found.
    pub(super) pos: usize,
    pub(super) rel: &'a parser::RelationshipPattern,
    pub(super) next_node: &'a parser::NodePattern,
    pub(super) min_hops: usize,
    pub(super) max_hops: usize,
    /// `false` when the declared relationship type(s) never resolved
    /// to a real catalog type id — see the doc comment on
    /// [`Executor::exists_probe_var_length`].
    pub(super) can_extend: bool,
    pub(super) type_ids: &'a [u32],
    pub(super) direction: Direction,
    pub(super) where_clause: Option<&'a parser::Expression>,
}

impl Executor {
    /// Evaluate `EXISTS { pattern [WHERE expr] }` (and the bare
    /// pattern-predicate form the parser lowers to the same AST node)
    /// against live graph state.
    ///
    /// The pattern is walked from `row`'s already-bound variables via
    /// the authoritative store adjacency (`find_relationships`, never
    /// `relationship_index()` — that index is non-authoritative for
    /// correctness). The anchor of each connected component (the
    /// pattern's first element, and every node that starts a fresh
    /// comma-separated pattern part) is resolved from the outer
    /// binding when its variable is already in scope (correlated
    /// subquery semantics); otherwise it is enumerated from the store.
    /// Every relationship hop binds its target node fresh per
    /// candidate, checking type(s), direction, and the target node's
    /// label/property constraints, and observes Cypher's relationship-
    /// isomorphism rule: no single witness binding reuses the same
    /// relationship id in two different hops. `EXISTS` is true iff at
    /// least one complete binding satisfies both the pattern's
    /// structural constraints and the optional inner `WHERE`, which is
    /// evaluated per candidate binding — a candidate that fails
    /// `WHERE` does not abort the probe, the walk simply continues to
    /// the next candidate. The search is depth-first and short-
    /// circuits on the first witness.
    ///
    /// Returns `Value::Null` (never plain `Value::Bool(false)`) when a
    /// correlated outer variable the pattern depends on is bound to
    /// `NULL` along every path tried, and no path produced a witness:
    /// three-valued logic — a pattern predicate over unknown input is
    /// itself unknown, not false. `Value::Null` composes correctly
    /// under `NOT` and is filtered out by `WHERE` exactly like any
    /// other `NULL` boolean expression.
    pub(in crate::executor) fn evaluate_exists_pattern(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        pattern: &parser::Pattern,
        where_clause: Option<&parser::Expression>,
    ) -> Result<Value> {
        if pattern.elements.is_empty() {
            // Defensive-only: the parser never produces an empty
            // pattern.
            return Ok(Value::Bool(false));
        }
        let mut bound_relationships: HashSet<u64> = HashSet::new();
        let outcome = self.exists_probe(
            context,
            &pattern.elements,
            0,
            row.clone(),
            None,
            &mut bound_relationships,
            where_clause,
        )?;
        Ok(outcome.into_value())
    }

    /// Resolve the candidate node id(s) that anchor a fresh component
    /// of an `EXISTS` pattern (the very first element, or a node that
    /// starts a new comma-separated pattern part).
    ///
    /// `pub(super)`: also drives the pattern-comprehension full
    /// enumeration walk's anchor resolution.
    pub(super) fn exists_resolve_anchor(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        node: &parser::NodePattern,
    ) -> Result<ExistsAnchor> {
        if let Some(var) = &node.variable {
            if let Some(value) = binding.get(var) {
                return Ok(Self::exists_anchor_from_single_value(value));
            }
            if let Some(value) = context.get_variable(var) {
                // `update_variables_from_rows` stores every variable as
                // `Value::Array(values)` — one entry per materialised
                // row — not a single scalar. Expand it into the full
                // candidate id list instead of feeding the array
                // itself to `extract_entity_id` (which only recognises
                // a single node/relationship object and would silently
                // resolve to zero candidates).
                return Ok(match value {
                    Value::Null => ExistsAnchor::Null,
                    Value::Array(values) => ExistsAnchor::Ids(
                        values.iter().filter_map(Self::extract_entity_id).collect(),
                    ),
                    other => Self::exists_anchor_from_single_value(other),
                });
            }
        }
        // Fresh variable (or anonymous node): enumerate candidate ids
        // directly from the store/label index — never through
        // `execute_node_by_label` / `execute_all_nodes_scan`, which
        // materialise a full JSON `Value` per node and hard-error via
        // `Error::OutOfMemory` above `MAX_INTERMEDIATE_ROWS`. The
        // remaining labels/properties are re-checked per candidate in
        // `exists_accept_node_candidate`.
        let ids = self.exists_enumerate_candidate_ids(node.labels.first().map(String::as_str))?;
        Ok(ExistsAnchor::Ids(ids))
    }

    /// Resolve a single already-bound row value to an anchor: `NULL`
    /// propagates as `ExistsAnchor::Null`; a node/relationship object
    /// resolves to its id; anything else (bound to a non-entity value)
    /// yields zero candidates.
    fn exists_anchor_from_single_value(value: &Value) -> ExistsAnchor {
        if value.is_null() {
            ExistsAnchor::Null
        } else {
            match Self::extract_entity_id(value) {
                Some(id) => ExistsAnchor::Ids(vec![id]),
                None => ExistsAnchor::Ids(Vec::new()),
            }
        }
    }

    /// Enumerate live node ids for a fresh `EXISTS` pattern anchor:
    /// the label bitmap when `label` is declared, otherwise every live
    /// node id in the store. Returns raw ids only — no per-node JSON
    /// materialisation — so a large unlabeled anchor cannot blow the
    /// `MAX_INTERMEDIATE_ROWS` ceiling the way `execute_all_nodes_scan`
    /// would.
    fn exists_enumerate_candidate_ids(&self, label: Option<&str>) -> Result<Vec<u64>> {
        if let Some(label) = label {
            return match self.catalog().get_label_id(label) {
                Ok(label_id) => {
                    let bitmap = self.label_index().get_nodes(label_id)?;
                    Ok(bitmap.iter().map(u64::from).collect())
                }
                Err(_) => Ok(Vec::new()), // label never assigned to any node
            };
        }
        let store = self.store();
        let total_nodes = store.node_count();
        let mut ids = Vec::new();
        for node_id in 0..total_nodes {
            if let Ok(record) = store.read_node(node_id) {
                if !record.is_deleted() {
                    ids.push(node_id);
                }
            }
        }
        Ok(ids)
    }

    /// Verify `candidate_id` satisfies a pattern node's label/property
    /// constraints and, when the node's variable is already present in
    /// `binding` (correlated re-use — e.g. a closed triangle
    /// `(a)-->(b)-->(a)`, or a hop target that reuses an outer
    /// variable name), that the existing binding agrees with
    /// `candidate_id`.
    ///
    /// Reads the candidate exactly once: a single `store.read_node`
    /// backs the liveness check, the label check, and (when accepted)
    /// the property load reused for both the inline property-map match
    /// and the binding's materialised node value.
    ///
    /// `pub(super)`: also drives the pattern-comprehension full
    /// enumeration walk's per-candidate acceptance test.
    pub(super) fn exists_accept_node_candidate(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        node: &parser::NodePattern,
        candidate_id: u64,
    ) -> Result<ExistsAcceptOutcome> {
        let store = self.store();
        let record = match store.read_node(candidate_id) {
            Ok(r) => r,
            Err(_) => return Ok(ExistsAcceptOutcome::Rejected),
        };
        if record.is_deleted() {
            return Ok(ExistsAcceptOutcome::Rejected);
        }

        let label_names = self.catalog().get_labels_from_bitmap(record.label_bits)?;
        if !node.labels.is_empty() {
            for required in &node.labels {
                if !label_names.iter().any(|l| l == required) {
                    return Ok(ExistsAcceptOutcome::Rejected);
                }
            }
        }

        if let Some(var) = &node.variable {
            if let Some(existing) = binding.get(var) {
                if existing.is_null() {
                    return Ok(ExistsAcceptOutcome::Null);
                }
                match Self::extract_entity_id(existing) {
                    Some(existing_id) if existing_id == candidate_id => {}
                    _ => return Ok(ExistsAcceptOutcome::Rejected),
                }
            }
        }

        let properties_value = store
            .load_node_properties_with_ptr(candidate_id, record.prop_ptr)?
            .unwrap_or_else(|| Value::Object(Map::new()));
        let mut node_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };
        node_map.insert("_nexus_id".to_string(), Value::Number(candidate_id.into()));
        node_map.insert(
            "_nexus_labels".to_string(),
            Value::Array(label_names.into_iter().map(Value::String).collect()),
        );
        let node_value = Value::Object(node_map);
        drop(store);

        if !self.exists_node_properties_match(
            binding,
            context,
            node.properties.as_ref(),
            &node_value,
        )? {
            return Ok(ExistsAcceptOutcome::Rejected);
        }

        let mut next = binding.clone();
        if let Some(var) = &node.variable {
            next.entry(var.clone()).or_insert(node_value);
        }
        Ok(ExistsAcceptOutcome::Accepted(next))
    }

    /// Evaluate a pattern node's inline property map (`{prop: expr}`)
    /// against an already-materialised candidate node value. Each
    /// expected value is evaluated with the full projection evaluator
    /// (not just literals), so a property constraint may reference
    /// outer-row/correlated variables, e.g.
    /// `EXISTS { (a)-->(b {id: a.id}) }`. A `NULL` on either side never
    /// matches (mirrors Cypher's `NULL = x` → `NULL` under WHERE
    /// truthiness).
    fn exists_node_properties_match(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        properties: Option<&parser::PropertyMap>,
        node_value: &Value,
    ) -> Result<bool> {
        let Some(props) = properties else {
            return Ok(true);
        };
        if props.properties.is_empty() {
            return Ok(true);
        }
        for (key, expected_expr) in &props.properties {
            let expected = self.evaluate_projection_expression(binding, context, expected_expr)?;
            let actual = Self::extract_property(node_value, key);
            if actual.is_null()
                || expected.is_null()
                || !self.values_equal_for_comparison(&actual, &expected)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a pattern relationship's inline property map against a
    /// candidate relationship. Mirrors `exists_node_properties_match`.
    ///
    /// `pub(super)`: also used by the pattern-comprehension full
    /// enumeration walk's relationship-hop filtering.
    pub(super) fn exists_relationship_properties_match(
        &self,
        binding: &HashMap<String, Value>,
        context: &ExecutionContext,
        properties: Option<&parser::PropertyMap>,
        rel_info: &RelationshipInfo,
    ) -> Result<bool> {
        let Some(props) = properties else {
            return Ok(true);
        };
        if props.properties.is_empty() {
            return Ok(true);
        }
        let rel_value = self.read_relationship_as_value(rel_info)?;
        for (key, expected_expr) in &props.properties {
            let expected = self.evaluate_projection_expression(binding, context, expected_expr)?;
            let actual = Self::extract_property(&rel_value, key);
            if actual.is_null()
                || expected.is_null()
                || !self.values_equal_for_comparison(&actual, &expected)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Depth-first walk of an `EXISTS` pattern's element list, starting
    /// at `elements[pos]`.
    ///
    /// A `Node` reached here always starts a fresh component: by
    /// construction of `parse_pattern_until_where_or_brace`, the only
    /// element positions that dispatch to this arm are `pos == 0` and
    /// a `Node` immediately following another `Node` — the flat
    /// encoding of a comma-separated independent pattern part, e.g.
    /// `EXISTS { (a)-->(b), (c)-->(d) }`. Every `Node` that is the
    /// target of a relationship hop is consumed inline by the
    /// `Relationship` arm below via `elements.get(pos + 1)`, so it
    /// never reaches this dispatch.
    ///
    /// `anchor_id` is the id of the most recently bound node, used by
    /// the `Relationship` arm to expand from; it is `None` exactly
    /// when `pos` lands on a fresh-component `Node` (the arm resolves
    /// its own anchor and ignores the parameter). `bound_relationships`
    /// carries the set of relationship ids already consumed by earlier
    /// hops in the CURRENT witness candidate (across every component,
    /// not just the current one) — Cypher's relationship-isomorphism
    /// rule: a single pattern match never traverses the same edge
    /// twice. Entries are inserted before recursing into a hop and
    /// removed again on backtrack, so sibling candidates at the same
    /// or an earlier position see the set as it was before that hop
    /// was tried.
    ///
    /// Returns `ExistsOutcome::True` on the first witness found
    /// (short-circuiting); otherwise `ExistsOutcome::Null` if any
    /// explored path hit a correlated `NULL` and no witness was found,
    /// else `ExistsOutcome::False`.
    fn exists_probe(
        &self,
        context: &ExecutionContext,
        elements: &[parser::PatternElement],
        pos: usize,
        binding: HashMap<String, Value>,
        anchor_id: Option<u64>,
        bound_relationships: &mut HashSet<u64>,
        where_clause: Option<&parser::Expression>,
    ) -> Result<ExistsOutcome> {
        let Some(element) = elements.get(pos) else {
            // Pattern fully walked — the accumulated binding is a
            // complete match. It is a witness iff it also satisfies
            // the inner WHERE (vacuously true when there is none).
            // The inner WHERE is a subquery filter: a NULL result
            // excludes the candidate row exactly like false does, so
            // it yields False here — unlike a NULL correlated pattern
            // variable, which makes the whole predicate Null.
            return match where_clause {
                Some(expr) => Ok(
                    if self.evaluate_predicate_on_row(&binding, context, expr)? {
                        ExistsOutcome::True
                    } else {
                        ExistsOutcome::False
                    },
                ),
                None => Ok(ExistsOutcome::True),
            };
        };

        match element {
            parser::PatternElement::Node(node) => {
                let candidates = match self.exists_resolve_anchor(&binding, context, node)? {
                    ExistsAnchor::Null => return Ok(ExistsOutcome::Null),
                    ExistsAnchor::Ids(ids) => ids,
                };
                let mut saw_null = false;
                for candidate_id in candidates {
                    match self.exists_accept_node_candidate(
                        &binding,
                        context,
                        node,
                        candidate_id,
                    )? {
                        ExistsAcceptOutcome::Rejected => {}
                        ExistsAcceptOutcome::Null => saw_null = true,
                        ExistsAcceptOutcome::Accepted(next_binding) => {
                            match self.exists_probe(
                                context,
                                elements,
                                pos + 1,
                                next_binding,
                                Some(candidate_id),
                                bound_relationships,
                                where_clause,
                            )? {
                                ExistsOutcome::True => return Ok(ExistsOutcome::True),
                                ExistsOutcome::Null => saw_null = true,
                                ExistsOutcome::False => {}
                            }
                        }
                    }
                }
                Ok(if saw_null {
                    ExistsOutcome::Null
                } else {
                    ExistsOutcome::False
                })
            }
            parser::PatternElement::Relationship(rel) => {
                let Some(anchor_id) = anchor_id else {
                    // A Relationship can only follow a Node in a
                    // well-formed pattern; defensive-only.
                    return Ok(ExistsOutcome::False);
                };
                let Some(parser::PatternElement::Node(next_node)) = elements.get(pos + 1) else {
                    // The grammar always pairs a relationship with a
                    // following node; defensive-only.
                    return Ok(ExistsOutcome::False);
                };

                // Resolve each declared type via a plain (read-only)
                // catalog lookup — NEVER `get_or_create_type` here:
                // this is a read path (WHERE-clause evaluation), and
                // interning a brand-new type id into the LMDB catalog
                // as a side effect of probing a pattern would corrupt
                // catalog state for a query that creates nothing. A
                // type name that has never been assigned to any
                // relationship simply contributes no id; if NONE of
                // the declared names resolve, the hop cannot match
                // anything (an empty `type_ids` list would instead be
                // reinterpreted by `find_relationships` as "match
                // every type", a false positive), so the probe fails
                // this hop without touching the catalog.
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
                    // A named relationship variable on a variable-length
                    // hop binds a LIST<RELATIONSHIP> in full Cypher; the
                    // probe only ever materialises a single relationship
                    // value per hop variable. Reject clearly rather than
                    // bind something wrong.
                    if rel.variable.is_some() {
                        return Err(var_length_rel_variable_not_implemented_error("EXISTS"));
                    }
                    let (min_hops, max_hops) = match quantifier {
                        // openCypher defines a bare `*` as `*1..` (one
                        // or more), NOT `*0..` — deliberately diverging
                        // here from `execute_variable_length_path`
                        // (path.rs:489), whose `ZeroOrMore => (0,
                        // usize::MAX)` is a pre-existing MATCH-side
                        // off-by-one left untouched (out of scope for
                        // this probe). Getting this right at THIS call
                        // site matters more than bug-for-bug parity:
                        // without it, `EXISTS { (a)-[:T*]->() }` would
                        // be unconditionally true for every node (the
                        // zero-length case accepts the anchor itself
                        // against an unconstrained target). Explicit
                        // zero-length ranges (`*0..1`, `*0..`) are
                        // untouched — they already carry their own
                        // literal `0` lower bound via `Range`/parsed
                        // quantifiers, not this arm.
                        parser::RelationshipQuantifier::ZeroOrMore => (1, usize::MAX),
                        parser::RelationshipQuantifier::OneOrMore => (1, usize::MAX),
                        parser::RelationshipQuantifier::ZeroOrOne => (0, 1),
                        parser::RelationshipQuantifier::Exact(n) => (*n, *n),
                        parser::RelationshipQuantifier::Range(min, max) => (*min, *max),
                    };
                    // Mirrors `execute_variable_length_path`'s own
                    // clamp (path.rs) — an unbounded `*`/`+` quantifier
                    // faces the identical exponential-trail-count
                    // hazard that constant exists to cap; without it a
                    // no-witness probe over a dense, cyclic graph must
                    // exhaust every isomorphic trail before returning
                    // `False`, and an uncapped depth could also make
                    // `EXISTS` witness a path longer than `MATCH`'s own
                    // variable-length operator would ever allow.
                    let max_hops = max_hops.min(MAX_VAR_LENGTH_PATH_DEPTH);
                    // A declared type list that resolved to zero ids
                    // means the type has never been assigned to any
                    // relationship: no hop of length >= 1 can ever be
                    // taken, but a zero-length match (`min_hops == 0`)
                    // is still evaluated on its own merits below.
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
                    return self.exists_probe_var_length(
                        &walk,
                        binding,
                        anchor_id,
                        0,
                        bound_relationships,
                    );
                }

                if !rel.types.is_empty() && type_ids.is_empty() {
                    return Ok(ExistsOutcome::False);
                }

                // Authoritative store adjacency — never
                // `relationship_index()`, which is not authoritative
                // for correctness.
                let relationships = self.find_relationships(
                    anchor_id, &type_ids, direction, None, // No cache for EXISTS probes
                )?;

                let mut saw_null = false;
                for rel_info in &relationships {
                    // Cypher relationship-isomorphism: a witness
                    // binding never traverses the same edge twice
                    // (e.g. a self-loop can't satisfy two consecutive
                    // hops, and an undirected `--` re-scan of the same
                    // edge from the other side doesn't count as a
                    // second hop).
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
                        ExistsAcceptOutcome::Rejected => {}
                        ExistsAcceptOutcome::Null => saw_null = true,
                        ExistsAcceptOutcome::Accepted(next_binding) => {
                            bound_relationships.insert(rel_info.id);
                            let outcome = self.exists_probe(
                                context,
                                elements,
                                pos + 2,
                                next_binding,
                                Some(target_id),
                                bound_relationships,
                                where_clause,
                            )?;
                            bound_relationships.remove(&rel_info.id);
                            match outcome {
                                ExistsOutcome::True => return Ok(ExistsOutcome::True),
                                ExistsOutcome::Null => saw_null = true,
                                ExistsOutcome::False => {}
                            }
                        }
                    }
                }
                Ok(if saw_null {
                    ExistsOutcome::Null
                } else {
                    ExistsOutcome::False
                })
            }
            parser::PatternElement::QuantifiedGroup(_) => Err(Error::CypherExecution(
                "ERR_QPP_NOT_IMPLEMENTED: quantified path patterns inside EXISTS subqueries \
                 need the QPP operator"
                    .to_string(),
            )),
        }
    }

    /// Depth-first walk of a variable-length relationship hop
    /// (`-[:T*min..max]->`) inside an `EXISTS` pattern.
    ///
    /// `current_id` is the frontier node reached after `depth` hops
    /// from the segment's starting anchor. At every depth within
    /// `[walk.min_hops, walk.max_hops]` the walk first tries accepting
    /// `current_id` itself against `walk.next_node`'s constraints —
    /// the zero-length case (`depth == min_hops == 0`) binds the
    /// pattern's target straight to the segment's anchor, consuming no
    /// edge — then, if `depth < walk.max_hops`, extends by one more
    /// hop. An unbounded `max_hops` (`usize::MAX` for bare `*` / `+`,
    /// clamped to `MAX_VAR_LENGTH_PATH_DEPTH` by the caller — see
    /// [`Self::exists_probe`]) needs no further artificial cap here:
    /// `bound_relationships` below forbids re-entering an
    /// already-consumed edge, so no DFS branch can exceed the graph's
    /// distinct live-edge count within that ceiling.
    ///
    /// `walk.can_extend` is `false` when the declared relationship
    /// type(s) never resolved to a real catalog type id — no hop of
    /// length >= 1 is possible in that case, but the zero-length case
    /// at `depth == 0` is still tried when `min_hops == 0` (an
    /// unmatched type still allows `*0..n` to degrade to "target is
    /// the anchor itself").
    ///
    /// Isomorphism (`bound_relationships`) is enforced exactly as for
    /// a fixed-length hop: an edge id is inserted before recursing
    /// into the extension that consumes it and removed again on
    /// backtrack, so it becomes unavailable to every later hop in the
    /// CURRENT witness candidate — including hops belonging to a
    /// different pattern component — without leaking across sibling
    /// candidates.
    fn exists_probe_var_length(
        &self,
        walk: &ExistsVarLengthWalk<'_>,
        binding: HashMap<String, Value>,
        current_id: u64,
        depth: usize,
        bound_relationships: &mut HashSet<u64>,
    ) -> Result<ExistsOutcome> {
        let mut saw_null = false;

        if depth >= walk.min_hops {
            match self.exists_accept_node_candidate(
                &binding,
                walk.context,
                walk.next_node,
                current_id,
            )? {
                ExistsAcceptOutcome::Rejected => {}
                ExistsAcceptOutcome::Null => saw_null = true,
                ExistsAcceptOutcome::Accepted(next_binding) => {
                    match self.exists_probe(
                        walk.context,
                        walk.elements,
                        walk.pos + 2,
                        next_binding,
                        Some(current_id),
                        bound_relationships,
                        walk.where_clause,
                    )? {
                        ExistsOutcome::True => return Ok(ExistsOutcome::True),
                        ExistsOutcome::Null => saw_null = true,
                        ExistsOutcome::False => {}
                    }
                }
            }
        }

        if walk.can_extend && depth < walk.max_hops {
            // Authoritative store adjacency — never `relationship_index()`.
            let relationships =
                self.find_relationships(current_id, walk.type_ids, walk.direction, None)?;
            for rel_info in &relationships {
                // Cypher relationship-isomorphism, enforced across the
                // whole var-length segment (and beyond it, into the
                // rest of the pattern): an edge already consumed by an
                // earlier hop in this witness candidate can't satisfy
                // a later one.
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
                let outcome = self.exists_probe_var_length(
                    walk,
                    binding.clone(),
                    hop_target,
                    depth + 1,
                    bound_relationships,
                )?;
                bound_relationships.remove(&rel_info.id);
                match outcome {
                    ExistsOutcome::True => return Ok(ExistsOutcome::True),
                    ExistsOutcome::Null => saw_null = true,
                    ExistsOutcome::False => {}
                }
            }
        }

        Ok(if saw_null {
            ExistsOutcome::Null
        } else {
            ExistsOutcome::False
        })
    }
}
