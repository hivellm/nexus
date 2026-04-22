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
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Path structure for shortest path functions
pub(in crate::executor) struct Path {
    pub(in crate::executor) nodes: Vec<u64>,
    pub(in crate::executor) relationships: Vec<u64>,
}

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

        // Fallback to original linked list traversal (Phase 1-2 behavior)
        // CRITICAL FIX: Force use of linked list traversal to debug relationship finding issue
        // This ensures we're using the most reliable method that should find all relationships
        let mut relationships = Vec::new();

        // Read the node record to get the first relationship pointer
        if let Ok(node_record) = self.store().read_node(node_id) {
            let mut rel_ptr = node_record.first_rel_ptr;

            // CRITICAL DEBUG: Log node reading and first_rel_ptr
            tracing::trace!(
                "[find_relationships] Node {} read: first_rel_ptr={}, type_ids={:?}, direction={:?}",
                node_id,
                rel_ptr,
                type_ids,
                direction
            );

            // CRITICAL FIX: If first_rel_ptr is 0, try to find relationships by scanning
            // This handles the case where mmap synchronization failed and first_rel_ptr
            // was not updated correctly, but relationships exist
            // When first_rel_ptr is 0, we scan for all relationships matching the direction
            // and then follow the linked list from each found relationship
            if rel_ptr == 0 {
                tracing::trace!(
                    "[find_relationships] Node {}: first_rel_ptr is 0 - attempting to find relationships by scanning",
                    node_id
                );

                // Scan for relationships where this node is the source (for Outgoing) or target (for Incoming)
                // We'll scan recent relationships (limit to avoid performance issues)
                // CRITICAL FIX: Start from a reasonable high ID and scan backwards, checking up to 501 relationships
                // to ensure rel_id=0 is always checked. This assumes relationships are created sequentially.
                let start_id = 500; // Start from a reasonable high ID (adjust if you have more relationships)
                let scan_limit = 501; // Check at most 501 relationships (0..=500 is 501 items)
                let mut scanned_rel_ids = std::collections::HashSet::new();
                let mut scanned_count = 0;

                // First pass: Find all relationships directly connected to this node
                // Scan backwards from start_id to find recent relationships
                for check_rel_id in (0..=start_id).rev() {
                    if scanned_count >= scan_limit {
                        break;
                    }
                    scanned_count += 1;
                    if let Ok(rel_record) = self.store().read_rel(check_rel_id) {
                        if !rel_record.is_deleted() {
                            let check_src_id = rel_record.src_id;
                            let check_dst_id = rel_record.dst_id;

                            // CRITICAL FIX: Skip uninitialized relationship records
                            // These have src_id=0 and dst_id=0 (pointing to node 0 in both directions)
                            if check_src_id == 0 && check_dst_id == 0 && check_rel_id > 0 {
                                // This looks like an uninitialized record - skip it
                                continue;
                            }

                            // Check if this relationship matches the direction we're looking for
                            let matches_direction = match direction {
                                Direction::Outgoing => check_src_id == node_id,
                                Direction::Incoming => check_dst_id == node_id,
                                Direction::Both => {
                                    check_src_id == node_id || check_dst_id == node_id
                                }
                            };

                            if matches_direction {
                                let record_type_id = rel_record.type_id;
                                let matches_type =
                                    type_ids.is_empty() || type_ids.contains(&record_type_id);

                                if matches_type {
                                    scanned_rel_ids.insert(check_rel_id);
                                }
                            }
                        }
                    }
                }

                // If we found relationships via scan, add them and return
                // (Skip linked list traversal since first_rel_ptr is 0 - linked list is broken)
                if !scanned_rel_ids.is_empty() {
                    tracing::trace!(
                        "[find_relationships] Node {}: Found {} relationships via scan (first_rel_ptr was 0)",
                        node_id,
                        scanned_rel_ids.len()
                    );

                    for rel_id in scanned_rel_ids {
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

                    // Return early - we found relationships via scan
                    return Ok(relationships);
                } else {
                    tracing::trace!(
                        "[find_relationships] Node {}: first_rel_ptr is 0 - no relationships found in linked list or scan",
                        node_id
                    );
                }
            }

            // CRITICAL FIX: For Direction::Both, we MUST use scan because the linked list
            // traversal only follows ONE chain (either next_src_ptr or next_dst_ptr).
            // A node can have relationships where it's the source (outgoing chain) AND
            // relationships where it's the target (incoming chain). The linked list approach
            // only traverses one of these chains, missing relationships on the other chain.
            // For Direction::Both, scan ALL relationships to find those involving this node.
            let should_use_scan_for_both = matches!(direction, Direction::Both);

            // CRITICAL FIX: Verify that first_rel_ptr points to a valid relationship for the requested direction
            // If first_rel_ptr points to a relationship where the node is TARGET but we're looking for OUTGOING,
            // or vice versa, then first_rel_ptr is invalid and we should use scan instead
            let mut should_use_scan = rel_ptr == 0;
            if rel_ptr != 0 && !should_use_scan_for_both {
                let verify_rel_id = rel_ptr.saturating_sub(1);
                if let Ok(verify_rel) = self.store().read_rel(verify_rel_id) {
                    if !verify_rel.is_deleted() {
                        let verify_src_id = verify_rel.src_id;
                        let verify_dst_id = verify_rel.dst_id;
                        let is_valid_for_direction = match direction {
                            Direction::Outgoing => verify_src_id == node_id,
                            Direction::Incoming => verify_dst_id == node_id,
                            Direction::Both => verify_src_id == node_id || verify_dst_id == node_id,
                        };

                        if !is_valid_for_direction {
                            // first_rel_ptr points to an invalid relationship - use scan instead
                            tracing::trace!(
                                "[find_relationships] Node {}: first_rel_ptr={} points to invalid relationship {} (src={}, dst={}) for direction {:?}, using scan",
                                node_id,
                                rel_ptr,
                                verify_rel_id,
                                verify_src_id,
                                verify_dst_id,
                                direction
                            );
                            should_use_scan = true;
                        }
                    } else {
                        // Relationship is deleted - use scan
                        should_use_scan = true;
                    }
                } else {
                    // Can't read relationship - use scan
                    should_use_scan = true;
                }
            }

            // If we should use scan (either for Direction::Both or because first_rel_ptr is invalid), do it now
            if should_use_scan_for_both || (should_use_scan && rel_ptr != 0) {
                // first_rel_ptr is invalid - scan for relationships
                tracing::trace!(
                    "[find_relationships] Node {}: first_rel_ptr={} is invalid, scanning for relationships",
                    node_id,
                    rel_ptr
                );

                // CRITICAL: Scan from a high ID down to 0 to find ALL relationships
                // Start from a reasonable high ID (assume max 10000 relationships) and scan down
                // NOTE: We need a high limit because is_deleted() may return false for uninitialized records
                let start_id = 10000;
                let scan_limit = 100000; // Increased to handle sparse storage
                let mut scanned_rel_ids = std::collections::HashSet::new();
                let mut scanned_count = 0;
                let mut checked_count = 0;

                // Scan backwards from start_id to find recent relationships
                for check_rel_id in (0..=start_id).rev() {
                    if scanned_count >= scan_limit {
                        break;
                    }
                    checked_count += 1;
                    if checked_count > scan_limit * 2 {
                        // Stop if we've checked too many (many may be empty)
                        break;
                    }

                    if let Ok(rel_record) = self.store().read_rel(check_rel_id) {
                        if !rel_record.is_deleted() {
                            scanned_count += 1;
                            let check_src_id = rel_record.src_id;
                            let check_dst_id = rel_record.dst_id;

                            // CRITICAL FIX: Skip uninitialized relationship records
                            // These have src_id=0 and dst_id=0 (pointing to node 0 in both directions)
                            // which are invalid for real relationships (would be a self-loop from node 0 to node 0)
                            // A real relationship would have a valid type_id > 0 if src=0 and dst=0
                            let record_type_id = rel_record.type_id;
                            if check_src_id == 0 && check_dst_id == 0 && check_rel_id > 0 {
                                // This looks like an uninitialized record - skip it
                                // Note: we only skip if rel_id > 0 because rel_id=0 could be legitimate
                                continue;
                            }

                            let matches_direction = match direction {
                                Direction::Outgoing => check_src_id == node_id,
                                Direction::Incoming => check_dst_id == node_id,
                                Direction::Both => {
                                    check_src_id == node_id || check_dst_id == node_id
                                }
                            };

                            if matches_direction {
                                let matches_type =
                                    type_ids.is_empty() || type_ids.contains(&record_type_id);

                                if matches_type {
                                    scanned_rel_ids.insert(check_rel_id);
                                }
                            }
                        }
                    }
                }

                if !scanned_rel_ids.is_empty() {
                    tracing::trace!(
                        "[find_relationships] Node {}: Found {} relationships via scan",
                        node_id,
                        scanned_rel_ids.len()
                    );

                    for rel_id in scanned_rel_ids {
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
                } else {
                    // Scan found nothing and first_rel_ptr is invalid - no relationships exist for this direction
                    tracing::trace!(
                        "[find_relationships] Node {}: first_rel_ptr was invalid and scan found no relationships for direction {:?}",
                        node_id,
                        direction
                    );
                    return Ok(relationships); // Return empty vector
                }
            }

            let mut visited = std::collections::HashSet::new();
            let mut iteration_count = 0;
            const MAX_ITERATIONS: usize = 100000; // Failsafe limit

            while rel_ptr != 0 {
                // Failsafe: Prevent infinite loops even if visited set fails
                iteration_count += 1;
                if iteration_count > MAX_ITERATIONS {
                    tracing::error!(
                        "[ERROR] Maximum iterations ({}) exceeded in relationship chain for node {}, breaking",
                        MAX_ITERATIONS,
                        node_id
                    );
                    break;
                }

                // CRITICAL: Detect infinite loops in relationship chain
                // This protects against circular references in the relationship linked list
                if !visited.insert(rel_ptr) {
                    tracing::error!(
                        "[WARN] Infinite loop detected in relationship chain for node {}, breaking at rel_ptr={}",
                        node_id,
                        rel_ptr
                    );
                    break;
                }

                let current_rel_id = rel_ptr.saturating_sub(1);

                // CRITICAL DEBUG: Log relationship traversal
                tracing::trace!(
                    "[find_relationships] Node {}: rel_ptr={}, current_rel_id={}",
                    node_id,
                    rel_ptr,
                    current_rel_id
                );

                if let Ok(rel_record) = self.store().read_rel(current_rel_id) {
                    // Copy fields to local variables to avoid packed struct reference issues
                    let src_id = rel_record.src_id;
                    let dst_id = rel_record.dst_id;
                    let next_src_ptr = rel_record.next_src_ptr;
                    let next_dst_ptr = rel_record.next_dst_ptr;
                    let record_type_id = rel_record.type_id;
                    let is_deleted = rel_record.is_deleted();

                    // CRITICAL DEBUG: Log relationship record details
                    tracing::trace!(
                        "[find_relationships] Node {}: rel_id={}, src_id={}, dst_id={}, type_id={}, is_deleted={}, next_src_ptr={}, next_dst_ptr={}",
                        node_id,
                        current_rel_id,
                        src_id,
                        dst_id,
                        record_type_id,
                        is_deleted,
                        next_src_ptr,
                        next_dst_ptr
                    );

                    if is_deleted {
                        rel_ptr = if src_id == node_id {
                            next_src_ptr
                        } else {
                            next_dst_ptr
                        };
                        continue;
                    }

                    // record_type_id already copied above
                    let matches_type = type_ids.is_empty() || type_ids.contains(&record_type_id);
                    let matches_direction = match direction {
                        Direction::Outgoing => src_id == node_id,
                        Direction::Incoming => dst_id == node_id,
                        Direction::Both => true,
                    };

                    if matches_type && matches_direction {
                        tracing::trace!(
                            "[find_relationships] Node {}: MATCHED relationship id={}, src={}, dst={}, type_id={}",
                            node_id,
                            current_rel_id,
                            src_id,
                            dst_id,
                            record_type_id
                        );
                        relationships.push(RelationshipInfo {
                            id: current_rel_id,
                            source_id: src_id,
                            target_id: dst_id,
                            type_id: record_type_id,
                        });
                    } else {
                        tracing::trace!(
                            "[find_relationships] Node {}: SKIPPED relationship id={} (matches_type={}, matches_direction={})",
                            node_id,
                            current_rel_id,
                            matches_type,
                            matches_direction
                        );
                    }

                    let old_rel_ptr = rel_ptr;
                    rel_ptr = if src_id == node_id {
                        next_src_ptr
                    } else {
                        next_dst_ptr
                    };

                    // CRITICAL DEBUG: Log linked list traversal
                    tracing::trace!(
                        "[find_relationships] Node {}: Moving from rel_id={} to next_ptr={} (src_id={}, node_id={}, using_next_src={})",
                        node_id,
                        current_rel_id,
                        rel_ptr,
                        src_id,
                        node_id,
                        src_id == node_id
                    );

                    if rel_ptr == 0 {
                        tracing::trace!(
                            "[find_relationships] Node {}: Reached end of linked list (rel_ptr=0)",
                            node_id
                        );
                    }
                } else {
                    tracing::trace!(
                        "[find_relationships] Node {}: Failed to read relationship record for rel_id={}",
                        node_id,
                        current_rel_id
                    );
                    break;
                }
            }
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
    type_filter: Option<u32>,
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
        type_filter: Option<u32>,
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
        // Filter by type if specified
        if let Some(filter_type) = self.type_filter {
            if type_id != filter_type {
                return false;
            }
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
        type_id: Option<u32>,
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
            self.materialize_rows_from_variables(context)
        };

        if rows.is_empty() {
            return Ok(());
        }

        // Determine min and max path lengths from quantifier
        let (min_length, max_length) = match quantifier {
            parser::RelationshipQuantifier::ZeroOrMore => (0, usize::MAX),
            parser::RelationshipQuantifier::OneOrMore => (1, usize::MAX),
            parser::RelationshipQuantifier::ZeroOrOne => (0, 1),
            parser::RelationshipQuantifier::Exact(n) => (*n, *n),
            parser::RelationshipQuantifier::Range(min, max) => (*min, *max),
        };

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
                        source_id, min_length, max_length, type_id, direction,
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
                    if !path_var.is_empty() {
                        let path_nodes_values: Vec<Value> = path_nodes
                            .iter()
                            .filter_map(|node_id| self.read_node_as_value(*node_id).ok())
                            .collect();
                        new_row.insert(path_var.to_string(), Value::Array(path_nodes_values));
                    }

                    push_with_row_cap(&mut expanded_rows, new_row, "VarLengthExpand")?;
                }

                // Continue expanding if we haven't reached max length
                if path_length < max_length {
                    // Find neighbors (convert Option<u32> to slice)
                    let type_ids_slice: Vec<u32> = type_id.into_iter().collect();
                    let neighbors =
                        self.find_relationships(current_node, &type_ids_slice, direction, None)?;

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
        type_id: Option<u32>,
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

            // Find neighbors (convert Option<u32> to slice)
            let type_ids_slice: Vec<u32> = type_id.into_iter().collect();
            let neighbors = self.find_relationships(current, &type_ids_slice, direction, None)?;
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
        type_id: Option<u32>,
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

            let type_ids_slice: Vec<u32> = type_id.into_iter().collect();
            let neighbors = self.find_relationships(current, &type_ids_slice, direction, None)?;
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
            type_id,
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
        type_id: Option<u32>,
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
                let type_ids_slice: Vec<u32> = type_id.into_iter().collect();
                let neighbors = self.find_relationships(from, &type_ids_slice, direction, None)?;
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

        let type_ids_slice: Vec<u32> = type_id.into_iter().collect();
        let neighbors = self.find_relationships(current, &type_ids_slice, direction, None)?;
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
                    type_id,
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

    /// Read a node as a JSON value
    pub(in crate::executor) fn read_node_as_value(&self, node_id: u64) -> Result<Value> {
        let node_record = self.store().read_node(node_id)?;

        if node_record.is_deleted() {
            return Ok(Value::Null);
        }

        let label_names = self
            .catalog()
            .get_labels_from_bitmap(node_record.label_bits)?;
        let _labels: Vec<Value> = label_names.into_iter().map(Value::String).collect();

        let properties_value = self.store().load_node_properties(node_id)?;

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

        // Return only the properties as a flat object, matching Neo4j's format
        // But include _nexus_id for internal ID extraction during relationship traversal
        let mut node = properties_map;
        node.insert("_nexus_id".to_string(), Value::Number(node_id.into()));

        tracing::trace!(
            "read_node_as_value: node_id={}, final node has {} keys: {:?}",
            node_id,
            node.len(),
            node.keys().collect::<Vec<_>>()
        );

        Ok(Value::Object(node))
    }
}
