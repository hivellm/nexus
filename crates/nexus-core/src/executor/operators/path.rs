//! Path traversal and relationship lookup. Holds the `Path` value used by
//! shortest-path results, the `VariableLengthPathVisitor` (which implements
//! `TraversalVisitor`), and the `execute_variable_length_path` /
//! `find_shortest_path` / `find_all_shortest_paths` / `find_paths_dfs`
//! routines. Also hosts `find_relationships` (with its rel-property-index
//! fast paths) plus node/path serialisers used across operators.

use super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::engine::Executor;
use super::super::parser;
use super::super::push_with_row_cap;
use super::super::types::Direction;
use crate::relationship::{TraversalAction, TraversalError, TraversalVisitor};
use crate::storage::RecordStore;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Path structure for shortest path functions
pub(in crate::executor) struct Path {
    pub(in crate::executor) nodes: Vec<u64>,
    pub(in crate::executor) relationships: Vec<u64>,
}

/// Per-query safety cap on the BFS depth `execute_variable_length_path`
/// will explore for an unbounded quantifier (`*` / `+`) or a `{m,}` /
/// `{m,n}` quantifier whose upper bound is very large. Mirrors
/// `quantified_expand.rs`'s `MAX_QPP_DEPTH = 64` (same rationale: keeps
/// per-frame memory and traversal fan-out tractable on a dense graph;
/// a sibling constant rather than a shared import since this legacy
/// BFS operator and the `QuantifiedExpand` operator have independent
/// state shapes). A legitimate bounded quantifier like `[*1..5]` stays
/// well under this cap and is unaffected.
///
/// `pub(in crate::executor)` (not private) so the `EXISTS` pattern
/// probe's own variable-length walk (`eval::helpers::exists_probe_var_length`)
/// shares the exact same ceiling instead of duplicating the literal —
/// an unbounded `*`/`+` quantifier inside `EXISTS` faces the identical
/// exponential-trail-count hazard this constant exists to cap.
pub(in crate::executor) const MAX_VAR_LENGTH_PATH_DEPTH: usize = 64;

impl Executor {
    pub(in crate::executor) fn find_relationships(
        &self,
        node_id: u64,
        type_ids: &[u32],
        direction: Direction,
        cache: Option<&crate::cache::MultiLayerCache>,
    ) -> Result<Vec<RelationshipInfo>> {
        // Phase 8.1: Try specialized relationship storage first (if enabled)
        // CRITICAL FIX: Temporarily disabled to debug relationship finding issue
        // The relationship_storage may not be updated correctly when relationships
        // are created in separate transactions, causing only the first relationship
        // to be found. We'll use linked list traversal instead for now.
        /*
        if self.enable_relationship_optimizations {
            if let Some(ref rel_storage) = self.shared.relationship_storage {
                let type_filter = if type_ids.len() == 1 {
                    Some(type_ids[0])
                } else {
                    None // Multiple types or all types - will filter later
                };

                if let Ok(rel_records) =
                    rel_storage
                        .read()
                        .get_relationships(node_id, direction, type_filter)
                {
                    // Convert RelationshipRecord to RelationshipInfo
                    let mut relationships = Vec::with_capacity(rel_records.len());
                    for rel_record in rel_records {
                        // Filter by type_ids if multiple types specified
                        if type_ids.is_empty() || type_ids.contains(&rel_record.type_id) {
                            relationships.push(RelationshipInfo {
                                id: rel_record.id,
                                source_id: rel_record.source_id,
                                target_id: rel_record.target_id,
                                type_id: rel_record.type_id,
                            });
                        }
                    }
                    if !relationships.is_empty() {
                        return Ok(relationships);
                    }
                }
            }
        }
        */

        // Phase 3: Fallback to adjacency list (fastest path)
        // CRITICAL FIX: Temporarily disabled to debug relationship finding issue
        // The adjacency list may not be updated correctly when relationships
        // are created in separate transactions. We'll use linked list traversal instead for now.
        /*
        if let Ok(Some(adj_rel_ids)) = match direction {
            Direction::Outgoing => self
                .store()
                .get_outgoing_relationships_adjacency(node_id, type_ids),
            Direction::Incoming => self
                .store()
                .get_incoming_relationships_adjacency(node_id, type_ids),
            Direction::Both => {
                // Get both outgoing and incoming
                let outgoing = self
                    .store()
                    .get_outgoing_relationships_adjacency(node_id, type_ids)?;
                let incoming = self
                    .store()
                    .get_incoming_relationships_adjacency(node_id, type_ids)?;
                match (outgoing, incoming) {
                    (Some(mut out), Some(mut inc)) => {
                        out.append(&mut inc);
                        Ok(Some(out))
                    }
                    (Some(out), None) => Ok(Some(out)),
                    (None, Some(inc)) => Ok(Some(inc)),
                    (None, None) => Ok(None),
                }
            }
        } {
            // Phase 3 Optimization: Batch read relationship records for better performance
            let mut relationships = Vec::with_capacity(adj_rel_ids.len());

            // Read records in batch (process all at once to improve cache locality)
            for rel_id in adj_rel_ids {
                if let Ok(rel_record) = self.store().read_rel(rel_id) {
                    if !rel_record.is_deleted() {
                        relationships.push(RelationshipInfo {
                            id: rel_id,
                            source_id: rel_record.src_id,
                            target_id: rel_record.dst_id,
                            type_id: rel_record.type_id,
                        });
                    }
                }
            }
            return Ok(relationships);
        }
        */

        // Fallback: Try to use relationship index if available (Phase 3 optimization)
        // CRITICAL FIX: Temporarily disabled to debug relationship finding issue
        // The relationship index may not be updated correctly when relationships
        // are created in separate transactions. We'll use linked list traversal instead for now.
        /*
        if let Some(cache) = cache {
            let rel_index = cache.relationship_index();

            // Check if this is a high-degree node and use optimized path
            let traversal_stats = rel_index.get_traversal_stats();
            let is_high_degree = traversal_stats.avg_relationships_per_node > 50.0;

            // Get relationship IDs from index
            let rel_ids = if is_high_degree {
                // Use optimized path for high-degree nodes
                match direction {
                    Direction::Outgoing => rel_index.get_high_degree_relationships(
                        node_id,
                        type_ids,
                        true,
                        Some(1000),
                    )?,
                    Direction::Incoming => rel_index.get_high_degree_relationships(
                        node_id,
                        type_ids,
                        false,
                        Some(1000),
                    )?,
                    Direction::Both => {
                        let mut outgoing = rel_index.get_high_degree_relationships(
                            node_id,
                            type_ids,
                            true,
                            Some(500),
                        )?;
                        let mut incoming = rel_index.get_high_degree_relationships(
                            node_id,
                            type_ids,
                            false,
                            Some(500),
                        )?;
                        outgoing.append(&mut incoming);
                        outgoing
                    }
                }
            } else {
                // Use standard path for regular nodes
                match direction {
                    Direction::Outgoing => {
                        rel_index.get_node_relationships(node_id, type_ids, true)?
                    }
                    Direction::Incoming => {
                        rel_index.get_node_relationships(node_id, type_ids, false)?
                    }
                    Direction::Both => {
                        let mut outgoing =
                            rel_index.get_node_relationships(node_id, type_ids, true)?;
                        let mut incoming =
                            rel_index.get_node_relationships(node_id, type_ids, false)?;
                        outgoing.append(&mut incoming);
                        outgoing
                    }
                }
            };

            // Convert relationship IDs to RelationshipInfo by reading from storage
            let mut relationships = Vec::new();
            for rel_id in rel_ids {
                if let Ok(rel_record) = self.store().read_rel(rel_id) {
                    if !rel_record.is_deleted() {
                        relationships.push(RelationshipInfo {
                            id: rel_id,
                            source_id: rel_record.src_id,
                            target_id: rel_record.dst_id,
                            type_id: rel_record.type_id,
                        });
                    }
                }
            }

            return Ok(relationships);
        }
        */

        // Authoritative both-direction adjacency, maintained in
        // `RecordStore::write_rel` and rebuilt from the records on open. This
        // replaces the `first_rel_ptr` chain walk (which threads only a node's
        // OUTGOING edges, so incoming expansion never found anything through
        // it) and its scan fallback (which only ever probed relationship ids
        // `0..=10_000`, so on any graph with more relationships than that an
        // incoming or `Both` traversal from a node whose edges sit at higher
        // ids silently returned nothing — e.g. every `(:Person)<-[:HAS_CREATOR]-`
        // on a loaded LDBC graph). The index knows every live edge of a node in
        // both directions in O(degree). See
        // `phase0_perf-store-reverse-incoming-adjacency-index`.
        let _ = cache;
        let store = self.store();

        // Acquire fence pairs with the Release fence in `write_rel`, which
        // publishes a relationship record before the reader observes it via the
        // adjacency index, so a matched id always resolves to a fully
        // initialised record rather than an allocated-but-unwritten slot.
        std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);

        let rel_ids = match direction {
            Direction::Outgoing => store.outgoing_relationships(node_id),
            Direction::Incoming => store.incoming_relationships(node_id),
            Direction::Both => store.connected_relationships(node_id),
        };

        let mut relationships = Vec::with_capacity(rel_ids.len());
        for rel_id in rel_ids {
            let rel_record = match store.read_rel(rel_id) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if rel_record.is_deleted() {
                continue;
            }
            // Copy out of the packed record before use (unaligned field
            // references are rejected).
            let src_id = rel_record.src_id;
            let dst_id = rel_record.dst_id;
            let type_id = rel_record.type_id;
            // The index is an accelerator, not the correctness authority:
            // re-check the direction against the record itself so a stale entry
            // could never fabricate an edge.
            let matches_direction = match direction {
                Direction::Outgoing => src_id == node_id,
                Direction::Incoming => dst_id == node_id,
                Direction::Both => src_id == node_id || dst_id == node_id,
            };
            if !matches_direction {
                continue;
            }
            if !type_ids.is_empty() && !type_ids.contains(&type_id) {
                continue;
            }
            relationships.push(RelationshipInfo {
                id: rel_id,
                source_id: src_id,
                target_id: dst_id,
                type_id,
            });
        }

        Ok(relationships)
    }
    /// Phase 8.3: Filter relationships using property index when applicable
    pub(in crate::executor) fn filter_relationships_by_property_index(
        &self,
        relationships: &[RelationshipInfo],
        type_id: Option<u32>,
        context: &ExecutionContext,
        rel_var: &str,
    ) -> Result<Vec<RelationshipInfo>> {
        // If no property index is available, return relationships as-is
        let prop_index = match &self.shared.relationship_property_index {
            Some(idx) => idx,
            None => return Ok(relationships.to_vec()),
        };

        // Try to extract property filters from context
        // For now, we'll check if there are any property filters in the WHERE clause
        // by looking at the execution context's filter expressions
        // This is a simplified implementation - a full implementation would parse
        // the WHERE clause AST to extract relationship property filters

        // For now, return relationships as-is
        // A full implementation would:
        // 1. Parse WHERE clause to find relationship property filters (e.g., r.weight > 10)
        // 2. Use RelationshipPropertyIndex to find matching relationship IDs
        // 3. Filter the relationships list to only include indexed matches
        Ok(relationships.to_vec())
    }

    /// Phase 8.3: Extract relationship property filters from WHERE clause and use index
    pub(in crate::executor) fn use_relationship_property_index_for_expand(
        &self,
        type_ids: &[u32],
        _context: &ExecutionContext,
        rel_var: &str,
    ) -> Result<Option<Vec<u64>>> {
        // Check if property index is available
        let prop_index = match &self.shared.relationship_property_index {
            Some(idx) => idx,
            None => return Ok(None),
        };

        // For now, we can't extract filters from WHERE clause without the full query AST
        // A full implementation would:
        // 1. Store WHERE clause filters in ExecutionContext during query planning
        // 2. Parse filters to find relationship property filters (e.g., r.weight > 10)
        // 3. Use RelationshipPropertyIndex::query_by_property to get matching relationship IDs
        // 4. Return the filtered list

        // Example of how it would work:
        // if let Some((prop_name, operator, value)) = extract_relationship_property_filter(rel_var, context) {
        //     let type_id = type_ids.first().copied();
        //     let rel_ids = prop_index.read().query_by_property(type_id, &prop_name, operator, &value)?;
        //     return Ok(Some(rel_ids));
        // }

        Ok(None)
    }
}

/// Phase 8.2: Visitor for variable-length path traversal
struct VariableLengthPathVisitor {
    start_node: u64,
    min_length: usize,
    max_length: usize,
    /// Type IDs to match (empty = match every type), mirroring the
    /// `find_relationships` matching rule.
    type_filter: Vec<u32>,
    direction: Direction,
    paths: Vec<(Vec<u64>, Vec<u64>)>, // (path_nodes, path_relationships)
    current_path_nodes: Vec<u64>,
    current_path_rels: Vec<u64>,
}

impl VariableLengthPathVisitor {
    pub(in crate::executor) fn new(
        start_node: u64,
        min_length: usize,
        max_length: usize,
        type_filter: Vec<u32>,
        direction: Direction,
    ) -> Self {
        Self {
            start_node,
            min_length,
            max_length,
            type_filter,
            direction,
            paths: Vec::new(),
            current_path_nodes: vec![start_node],
            current_path_rels: Vec::new(),
        }
    }

    pub(in crate::executor) fn get_paths(self) -> Vec<(Vec<u64>, Vec<u64>)> {
        self.paths
    }
}

impl TraversalVisitor for VariableLengthPathVisitor {
    fn visit_node(
        &mut self,
        node_id: u64,
        depth: usize,
    ) -> std::result::Result<TraversalAction, TraversalError> {
        // Update current path nodes if this is a new node
        if !self.current_path_nodes.contains(&node_id) {
            // This shouldn't happen in normal traversal, but handle it
            if let Some(&last) = self.current_path_nodes.last() {
                if last != node_id {
                    // Reset path if we're at a different node
                    self.current_path_nodes = vec![self.start_node, node_id];
                    self.current_path_rels.clear();
                }
            }
        }

        // Check if we've reached a valid path length
        // Path length is number of relationships, which is depth
        if depth >= self.min_length && depth <= self.max_length {
            // Save this path (only if it's complete and valid)
            if self.current_path_nodes.len() == depth + 1 && self.current_path_rels.len() == depth {
                self.paths.push((
                    self.current_path_nodes.clone(),
                    self.current_path_rels.clone(),
                ));
            }
        }

        // Continue traversal if we haven't reached max length
        if depth < self.max_length {
            Ok(TraversalAction::Continue)
        } else {
            Ok(TraversalAction::SkipChildren)
        }
    }

    fn visit_relationship(&mut self, rel_id: u64, source: u64, target: u64, type_id: u32) -> bool {
        // Filter by type if specified (empty = match every type)
        if !self.type_filter.is_empty() && !self.type_filter.contains(&type_id) {
            return false;
        }

        // Update current path - find which node is the next in the path
        let last_node = *self.current_path_nodes.last().unwrap();
        if source == last_node {
            self.current_path_nodes.push(target);
            self.current_path_rels.push(rel_id);
            true
        } else if target == last_node {
            self.current_path_nodes.push(source);
            self.current_path_rels.push(rel_id);
            true
        } else {
            // Relationship doesn't match current path - skip
            false
        }
    }

    fn should_prune(&self, node_id: u64, depth: usize) -> bool {
        // Prune if we've exceeded max length
        if depth > self.max_length {
            return true;
        }

        // Prune if we've already visited this node in the current path (avoid cycles)
        self.current_path_nodes.contains(&node_id)
    }
}

impl Executor {
    /// Execute variable-length path expansion using BFS
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor) fn execute_variable_length_path(
        &self,
        context: &mut ExecutionContext,
        type_ids: &[u32],
        direction: Direction,
        source_var: &str,
        target_var: &str,
        rel_var: &str,
        path_var: &str,
        quantifier: &parser::RelationshipQuantifier,
    ) -> Result<()> {
        use std::collections::{HashSet, VecDeque};

        // Get source nodes from context
        let rows = if !context.result_set.rows.is_empty() {
            self.result_set_as_rows(context)
        } else {
            self.materialize_rows_from_variables(context)?
        };

        if rows.is_empty() {
            return Ok(());
        }

        // Determine min and max path lengths from quantifier. `*` / `+`
        // request an unbounded upper length (`usize::MAX`); `{m,n}` /
        // `{m}` upper bounds come straight from the user's literal and
        // can be arbitrarily large too. Either way, clamp to
        // `MAX_VAR_LENGTH_PATH_DEPTH` so the BFS below can never expand
        // an unbounded number of hops (the pre-fix `usize::MAX` case
        // made the `path_length < max_length` continuation condition
        // effectively never false, looping until the graph or the
        // process's memory ran out).
        let (min_length, max_length) = match quantifier {
            parser::RelationshipQuantifier::ZeroOrMore => (0, usize::MAX),
            parser::RelationshipQuantifier::OneOrMore => (1, usize::MAX),
            parser::RelationshipQuantifier::ZeroOrOne => (0, 1),
            parser::RelationshipQuantifier::Exact(n) => (*n, *n),
            parser::RelationshipQuantifier::Range(min, max) => (*min, *max),
        };
        let max_length = max_length.min(MAX_VAR_LENGTH_PATH_DEPTH);

        let mut expanded_rows = Vec::new();

        // Phase 8.2: Try to use AdvancedTraversalEngine if optimizations are enabled
        // DISABLED: The optimized traversal has issues with fixed-length paths (*2, {2}, *1..3)
        // The VariableLengthPathVisitor doesn't track paths correctly in all cases.
        // Use the fallback BFS which works correctly for all quantifier types.
        let use_optimized_traversal = false; // Temporarily disabled - use BFS fallback
        let _original_condition = self.enable_relationship_optimizations
            && self.shared.traversal_engine.is_some()
            && max_length < 100;

        // Process each source row
        for row in rows {
            let source_value = row
                .get(source_var)
                .cloned()
                .or_else(|| context.get_variable(source_var).cloned())
                .unwrap_or(Value::Null);

            let source_id = match Self::extract_entity_id(&source_value) {
                Some(id) => id,
                None => continue,
            };

            // Phase 8.2: Use optimized traversal if available and appropriate
            if use_optimized_traversal {
                if let Some(ref traversal_engine) = self.shared.traversal_engine {
                    let mut visitor = VariableLengthPathVisitor::new(
                        source_id,
                        min_length,
                        max_length,
                        type_ids.to_vec(),
                        direction,
                    );

                    if let Ok(result) = traversal_engine.traverse_bfs_optimized(
                        source_id,
                        direction,
                        max_length,
                        &mut visitor,
                    ) {
                        // Process paths found by optimized traversal
                        let paths = visitor.get_paths();
                        for (path_nodes, path_rels) in paths {
                            if path_nodes.len() - 1 >= min_length
                                && path_nodes.len() - 1 <= max_length
                            {
                                let target_node =
                                    self.read_node_as_value(*path_nodes.last().unwrap())?;
                                let mut new_row = row.clone();
                                new_row.insert(source_var.to_string(), source_value.clone());
                                new_row.insert(target_var.to_string(), target_node);

                                // Add relationship variable if specified
                                if !rel_var.is_empty() && !path_rels.is_empty() {
                                    let rel_values: Vec<Value> = path_rels
                                        .iter()
                                        .filter_map(|rel_id| {
                                            if let Ok(rel_record) = self.store().read_rel(*rel_id) {
                                                Some(RelationshipInfo {
                                                    id: *rel_id,
                                                    source_id: rel_record.src_id,
                                                    target_id: rel_record.dst_id,
                                                    type_id: rel_record.type_id,
                                                })
                                            } else {
                                                None
                                            }
                                        })
                                        .filter_map(|rel_info| {
                                            self.read_relationship_as_value(&rel_info).ok()
                                        })
                                        .collect();

                                    if path_rels.len() == 1 {
                                        if let Some(first) = rel_values.first() {
                                            new_row
                                                .entry(rel_var.to_string())
                                                .or_insert_with(|| first.clone());
                                        }
                                    } else {
                                        new_row
                                            .insert(rel_var.to_string(), Value::Array(rel_values));
                                    }
                                }

                                // Add path variable if specified
                                if !path_var.is_empty() {
                                    let path_nodes_values: Vec<Value> = path_nodes
                                        .iter()
                                        .filter_map(|node_id| {
                                            self.read_node_as_value(*node_id).ok()
                                        })
                                        .collect();
                                    new_row.insert(
                                        path_var.to_string(),
                                        Value::Array(path_nodes_values),
                                    );
                                }

                                push_with_row_cap(
                                    &mut expanded_rows,
                                    new_row,
                                    "VarLengthExpand (single path)",
                                )?;
                            }
                        }
                        continue; // Skip to next source node
                    }
                }
            }

            // Fallback: Original BFS implementation
            // BFS to find all paths matching the quantifier
            let mut queue = VecDeque::new();
            let mut visited = HashSet::new();

            // Entry: (node_id, path_length, path_relationships, path_nodes)
            queue.push_back((source_id, 0, Vec::<u64>::new(), vec![source_id]));
            visited.insert((source_id, 0));

            while let Some((current_node, path_length, path_rels, path_nodes)) = queue.pop_front() {
                // Check if we've reached a valid path length
                if path_length >= min_length && path_length <= max_length {
                    // Create a result row for this path
                    let target_node = self.read_node_as_value(current_node)?;
                    let mut new_row = row.clone();
                    new_row.insert(source_var.to_string(), source_value.clone());
                    new_row.insert(target_var.to_string(), target_node);

                    // Add relationship variable if specified
                    if !rel_var.is_empty() && !path_rels.is_empty() {
                        let rel_values: Vec<Value> = path_rels
                            .iter()
                            .filter_map(|rel_id| {
                                if let Ok(rel_record) = self.store().read_rel(*rel_id) {
                                    Some(RelationshipInfo {
                                        id: *rel_id,
                                        source_id: rel_record.src_id,
                                        target_id: rel_record.dst_id,
                                        type_id: rel_record.type_id,
                                    })
                                } else {
                                    None
                                }
                            })
                            .filter_map(|rel_info| self.read_relationship_as_value(&rel_info).ok())
                            .collect();

                        if path_rels.len() == 1 {
                            // Single relationship - return as object, not array
                            if let Some(first) = rel_values.first() {
                                new_row
                                    .entry(rel_var.to_string())
                                    .or_insert_with(|| first.clone());
                            }
                        } else {
                            // Multiple relationships - return as array
                            new_row.insert(rel_var.to_string(), Value::Array(rel_values));
                        }
                    }

                    // Add path variable if specified
                    //
                    // phase8_neo4j-concurrency-gaps §2 — acquire the store
                    // guard ONCE for the whole path (instead of once per
                    // node in it) before mapping every node id through
                    // `read_node_as_value_with_store`; see that method's
                    // doc comment for the acquire-once rationale. Neither
                    // this closure nor anything it calls touches
                    // `self.store()` again, so holding the guard across
                    // the whole `filter_map` is safe.
                    if !path_var.is_empty() {
                        let path_store = self.store();
                        // A path value is the alternating node/relationship
                        // sequence `[n0, r0, n1, r1, …, nN]`. `nodes()`,
                        // `relationships()` and `length()` all read a path by
                        // filtering this list on the entity-kind marker, so
                        // binding nodes only made `relationships(p)` empty and
                        // `length(p)` zero for every variable-length match.
                        // BFS keeps the two vectors aligned as
                        // `path_nodes[i] -[path_rels[i]]-> path_nodes[i + 1]`,
                        // so each relationship slots in after its source node.
                        let mut path_elements: Vec<Value> =
                            Vec::with_capacity(path_nodes.len() + path_rels.len());
                        for (i, node_id) in path_nodes.iter().enumerate() {
                            if let Ok(node) =
                                self.read_node_as_value_with_store(&path_store, *node_id)
                            {
                                path_elements.push(node);
                            }
                            // `_with_store` variants only: `parking_lot`'s
                            // RwLock is not reentrant, so re-acquiring while
                            // `path_store` is alive would deadlock.
                            if let Some(rel_id) = path_rels.get(i)
                                && let Ok(rel_record) = path_store.read_rel(*rel_id)
                                && let Ok(rel) = self.read_relationship_as_value_with_store(
                                    &path_store,
                                    &RelationshipInfo {
                                        id: *rel_id,
                                        source_id: rel_record.src_id,
                                        target_id: rel_record.dst_id,
                                        type_id: rel_record.type_id,
                                    },
                                )
                            {
                                path_elements.push(rel);
                            }
                        }
                        drop(path_store);
                        new_row.insert(path_var.to_string(), Value::Array(path_elements));
                    }

                    push_with_row_cap(&mut expanded_rows, new_row, "VarLengthExpand")?;
                }

                // Continue expanding if we haven't reached max length
                if path_length < max_length {
                    let neighbors =
                        self.find_relationships(current_node, type_ids, direction, None)?;

                    for rel_info in neighbors {
                        let next_node = match direction {
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

                        // Avoid cycles: don't revisit nodes in the current path
                        if path_nodes.contains(&next_node) {
                            continue;
                        }

                        let new_path_length = path_length + 1;
                        let mut new_path_rels = path_rels.clone();
                        new_path_rels.push(rel_info.id);
                        let mut new_path_nodes = path_nodes.clone();
                        new_path_nodes.push(next_node);

                        // Add to queue if not already visited at this length
                        let visit_key = (next_node, new_path_length);
                        if !visited.contains(&visit_key) {
                            visited.insert(visit_key);
                            queue.push_back((
                                next_node,
                                new_path_length,
                                new_path_rels,
                                new_path_nodes,
                            ));
                        }
                    }
                }
            }
        }

        self.update_variables_from_rows(context, &expanded_rows);
        self.update_result_set_from_rows(context, &expanded_rows);

        Ok(())
    }

    /// Find shortest path between two nodes using BFS
    pub(in crate::executor) fn find_shortest_path(
        &self,
        start_id: u64,
        end_id: u64,
        type_ids: &[u32],
        direction: Direction,
    ) -> Result<Option<Path>> {
        use std::collections::{HashMap, VecDeque};

        if start_id == end_id {
            // Path to self is empty
            return Ok(Some(Path {
                nodes: vec![start_id],
                relationships: Vec::new(),
            }));
        }

        let mut queue = VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        let mut parent: HashMap<u64, (u64, u64)> = HashMap::new(); // node -> (parent_node, relationship_id)

        queue.push_back(start_id);
        visited.insert(start_id);

        while let Some(current) = queue.pop_front() {
            if current == end_id {
                // Reconstruct path
                let mut path_nodes = Vec::new();
                let mut path_rels = Vec::new();
                let mut node = end_id;

                while node != start_id {
                    path_nodes.push(node);
                    if let Some((parent_node, rel_id)) = parent.get(&node) {
                        path_rels.push(*rel_id);
                        node = *parent_node;
                    } else {
                        break;
                    }
                }
                path_nodes.push(start_id);
                path_nodes.reverse();
                path_rels.reverse();

                return Ok(Some(Path {
                    nodes: path_nodes,
                    relationships: path_rels,
                }));
            }

            let neighbors = self.find_relationships(current, type_ids, direction, None)?;
            for rel_info in neighbors {
                let next_node = match direction {
                    Direction::Outgoing => rel_info.target_id,
                    Direction::Incoming => rel_info.source_id,
                    Direction::Both => {
                        if rel_info.source_id == current {
                            rel_info.target_id
                        } else {
                            rel_info.source_id
                        }
                    }
                };

                if !visited.contains(&next_node) {
                    visited.insert(next_node);
                    parent.insert(next_node, (current, rel_info.id));
                    queue.push_back(next_node);
                }
            }
        }

        Ok(None) // No path found
    }

    /// Find all shortest paths between two nodes using BFS
    pub(in crate::executor) fn find_all_shortest_paths(
        &self,
        start_id: u64,
        end_id: u64,
        type_ids: &[u32],
        direction: Direction,
    ) -> Result<Vec<Path>> {
        use std::collections::{HashMap, VecDeque};

        if start_id == end_id {
            return Ok(vec![Path {
                nodes: vec![start_id],
                relationships: Vec::new(),
            }]);
        }

        // First BFS to find shortest distance
        let mut queue = VecDeque::new();
        let mut distances: HashMap<u64, usize> = HashMap::new();
        queue.push_back((start_id, 0));
        distances.insert(start_id, 0);

        while let Some((current, dist)) = queue.pop_front() {
            if current == end_id {
                break; // Found target
            }

            let neighbors = self.find_relationships(current, type_ids, direction, None)?;
            for rel_info in neighbors {
                let next_node = match direction {
                    Direction::Outgoing => rel_info.target_id,
                    Direction::Incoming => rel_info.source_id,
                    Direction::Both => {
                        if rel_info.source_id == current {
                            rel_info.target_id
                        } else {
                            rel_info.source_id
                        }
                    }
                };

                distances.entry(next_node).or_insert_with(|| {
                    queue.push_back((next_node, dist + 1));
                    dist + 1
                });
            }
        }

        // Get shortest distance
        let shortest_dist = if let Some(&dist) = distances.get(&end_id) {
            dist
        } else {
            return Ok(Vec::new()); // No path found
        };

        // Now find all paths of shortest length using DFS
        let mut paths = Vec::new();
        let mut current_path = vec![start_id];
        self.find_paths_dfs(
            start_id,
            end_id,
            type_ids,
            direction,
            shortest_dist,
            &mut current_path,
            &mut paths,
            &distances,
        )?;

        Ok(paths)
    }

    /// DFS helper to find all paths of a specific length
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor) fn find_paths_dfs(
        &self,
        current: u64,
        target: u64,
        type_ids: &[u32],
        direction: Direction,
        remaining_steps: usize,
        current_path: &mut Vec<u64>,
        paths: &mut Vec<Path>,
        distances: &std::collections::HashMap<u64, usize>,
    ) -> Result<()> {
        if current == target && remaining_steps == 0 {
            // Found a path of correct length
            let mut path_rels = Vec::new();
            for i in 0..current_path.len() - 1 {
                let from = current_path[i];
                let to = current_path[i + 1];
                let neighbors = self.find_relationships(from, type_ids, direction, None)?;
                if let Some(rel_info) = neighbors.iter().find(|r| match direction {
                    Direction::Outgoing => r.target_id == to,
                    Direction::Incoming => r.source_id == to,
                    Direction::Both => r.source_id == to || r.target_id == to,
                }) {
                    path_rels.push(rel_info.id);
                }
            }
            paths.push(Path {
                nodes: current_path.clone(),
                relationships: path_rels,
            });
            return Ok(());
        }

        if remaining_steps == 0 {
            return Ok(());
        }

        // Check if we can still reach target
        if let Some(&dist_to_target) = distances.get(&current) {
            if dist_to_target > remaining_steps {
                return Ok(());
            }
        }

        let neighbors = self.find_relationships(current, type_ids, direction, None)?;
        for rel_info in neighbors {
            let next_node = match direction {
                Direction::Outgoing => rel_info.target_id,
                Direction::Incoming => rel_info.source_id,
                Direction::Both => {
                    if rel_info.source_id == current {
                        rel_info.target_id
                    } else {
                        rel_info.source_id
                    }
                }
            };

            if !current_path.contains(&next_node) {
                current_path.push(next_node);
                self.find_paths_dfs(
                    next_node,
                    target,
                    type_ids,
                    direction,
                    remaining_steps - 1,
                    current_path,
                    paths,
                    distances,
                )?;
                current_path.pop();
            }
        }

        Ok(())
    }

    /// Convert Path to JSON Value
    pub(in crate::executor) fn path_to_value(&self, path: &Path) -> Value {
        let mut path_obj = serde_json::Map::new();

        // Add nodes array
        let nodes: Vec<Value> = path
            .nodes
            .iter()
            .filter_map(|node_id| self.read_node_as_value(*node_id).ok())
            .collect();
        path_obj.insert("nodes".to_string(), Value::Array(nodes));

        // Add relationships array
        let rels: Vec<Value> = path
            .relationships
            .iter()
            .filter_map(|rel_id| {
                if let Ok(rel_record) = self.store().read_rel(*rel_id) {
                    let rel_info = RelationshipInfo {
                        id: *rel_id,
                        source_id: rel_record.src_id,
                        target_id: rel_record.dst_id,
                        type_id: rel_record.type_id,
                    };
                    self.read_relationship_as_value(&rel_info).ok()
                } else {
                    None
                }
            })
            .collect();
        path_obj.insert("relationships".to_string(), Value::Array(rels));

        Value::Object(path_obj)
    }

    /// Read a node as a JSON value.
    ///
    /// Acquires its own `store()` read guard. This is the right choice
    /// for one-off reads, but bulk scanners that materialise many nodes
    /// in a loop should acquire the guard ONCE and call
    /// [`Self::read_node_as_value_with_store`] per element instead — see
    /// that method's doc comment for the concurrency rationale.
    pub(in crate::executor) fn read_node_as_value(&self, node_id: u64) -> Result<Value> {
        let store = self.store();
        self.read_node_as_value_with_store(&store, node_id)
    }

    /// Same as [`Self::read_node_as_value`], but for callers that
    /// already hold a `store()` read guard (e.g. a scan/expand loop
    /// materialising many nodes in a row).
    ///
    /// phase8_neo4j-concurrency-gaps §2 — `read_node_as_value` is the
    /// single most-called node materialiser in the executor (every
    /// scan, expand hop, and index seek routes through it), and its
    /// own `self.store()` acquisition was already halved to one call in
    /// the §2.2 pass. The remaining cost is the OUTER acquisition
    /// itself: bulk scanners (`execute_node_by_label`,
    /// `execute_all_nodes_scan`, `execute_node_index_seek`, the
    /// `Expand` target loop, ...) call this once PER CANDIDATE NODE,
    /// each call independently re-acquiring the single
    /// `ExecutorShared.store` `parking_lot::RwLock` — the exact
    /// per-iteration-lock-acquisition-in-a-loop pattern documented in
    /// the `per-iteration-rwlock-re-acquisition-in-scan-loops-collapses-
    /// under-thread-count` knowledge entry (same family as §1's
    /// `read_all_node_headers` fix for `count_live_nodes_*`). Threading
    /// an already-held `&RecordStore` through this method lets those
    /// loops acquire the guard once for the whole scan instead of once
    /// per node.
    ///
    /// SAFETY (non-reentrancy): `parking_lot::RwLock` does not allow a
    /// thread to recursively re-acquire a lock it already holds (see the
    /// `parking-lot-rwlock-does-not-allow-recursive-acquire` anti-pattern
    /// entry) — a caller passing `store` in here MUST NOT be in the
    /// middle of a call chain that would try to acquire `self.store()`
    /// again before this method returns. This method itself never calls
    /// `self.store()`; `self.catalog()` is a distinct lock (LMDB-backed).
    pub(in crate::executor) fn read_node_as_value_with_store(
        &self,
        store: &RecordStore,
        node_id: u64,
    ) -> Result<Value> {
        let node_record = store.read_node(node_id)?;

        if node_record.is_deleted() {
            return Ok(Value::Null);
        }

        let label_names = self
            .catalog()
            .get_labels_from_bitmap(node_record.label_bits)?;
        let labels: Vec<Value> = label_names.into_iter().map(Value::String).collect();

        // phase8_neo4j-concurrency-gaps §2 — pass the `prop_ptr` this
        // function already read above instead of calling
        // `load_node_properties(node_id)`, which internally re-reads
        // the node record (a second `nodes_mmap` lock acquisition, plus
        // a second `property_store` corruption cross-check) purely to
        // re-derive the same `prop_ptr` we already have. See
        // `RecordStore::load_node_properties_with_ptr`'s doc comment.
        let properties_value =
            store.load_node_properties_with_ptr(node_id, node_record.prop_ptr)?;

        tracing::trace!(
            "read_node_as_value: node_id={}, properties_value={:?}",
            node_id,
            properties_value
        );

        let properties_value = properties_value.unwrap_or_else(|| Value::Object(Map::new()));

        let properties_map = match properties_value {
            Value::Object(map) => {
                tracing::trace!(
                    "read_node_as_value: node_id={}, properties_map has {} keys: {:?}",
                    node_id,
                    map.len(),
                    map.keys().collect::<Vec<_>>()
                );
                map
            }
            other => {
                tracing::trace!(
                    "read_node_as_value: node_id={}, properties_value is not Object: {:?}",
                    node_id,
                    other
                );
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };

        // Return the properties as a flat object, matching Neo4j's format,
        // plus reserved internal fields: `_nexus_id` for id extraction during
        // relationship traversal, and `_nexus_labels` so callers can recover
        // the node's labels. openCypher treats labels as part of node
        // identity; this mirrors the sibling constructor
        // `Engine::node_to_result_value` (which already emits `_nexus_labels`)
        // and the `_nexus_id` / `_nexus_rel_type` precedent, and is ignored by
        // SDKs that only read declared properties.
        let mut node = properties_map;
        node.insert("_nexus_id".to_string(), Value::Number(node_id.into()));
        node.insert("_nexus_labels".to_string(), Value::Array(labels));

        tracing::trace!(
            "read_node_as_value: node_id={}, final node has {} keys: {:?}",
            node_id,
            node.len(),
            node.keys().collect::<Vec<_>>()
        );

        Ok(Value::Object(node))
    }
}
