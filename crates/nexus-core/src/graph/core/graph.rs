//! Graph data structure and operations

use super::edge::Edge;
use super::ids::{EdgeId, NodeId};
use super::node::Node;
use super::property_store::PropertyStore;
use super::stats::{DegreeStats, EdgeTypeStats, GraphStats, NodeTypeStats, PathStats};
use crate::graph::simple::PropertyValue;
use crate::storage::{NodeRecord, RecordStore, RelationshipRecord};
use crate::{Error, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// A graph containing nodes and edges
pub struct Graph {
    /// Storage backend for persistence
    pub(super) store: std::cell::RefCell<RecordStore>,
    /// Catalog for label/type/key mappings
    pub(super) catalog: Arc<crate::catalog::Catalog>,
    /// Cache of loaded nodes (in-memory)
    pub(super) node_cache: Arc<RwLock<HashMap<NodeId, Node>>>,
    /// Cache of loaded edges (in-memory)
    pub(super) edge_cache: Arc<RwLock<HashMap<EdgeId, Edge>>>,
    /// Property storage for managing property chains
    pub(super) property_store: Arc<RwLock<PropertyStore>>,
}

impl Graph {
    /// Create a new graph with the given storage backend and catalog
    pub fn new(store: RecordStore, catalog: Arc<crate::catalog::Catalog>) -> Self {
        Self {
            store: std::cell::RefCell::new(store),
            catalog,
            node_cache: Arc::new(RwLock::new(HashMap::new())),
            edge_cache: Arc::new(RwLock::new(HashMap::new())),
            property_store: Arc::new(RwLock::new(PropertyStore::new())),
        }
    }

    /// Create a new node in the graph
    pub fn create_node(&self, labels: Vec<String>) -> Result<NodeId> {
        self.create_node_with_external_id(labels, None, crate::storage::ConflictPolicy::Error)
    }

    /// Create a new node in the graph with an optional external id.
    ///
    /// When `external_id` is `None` this is identical to
    /// [`Graph::create_node`].  When it is `Some`, the catalog's
    /// external-id index is updated atomically per `policy`.
    pub fn create_node_with_external_id(
        &self,
        labels: Vec<String>,
        external_id: Option<crate::catalog::external_id::ExternalId>,
        policy: crate::storage::ConflictPolicy,
    ) -> Result<NodeId> {
        // Resolve label ids and build label_bits.
        let mut label_bits = 0u64;
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
        }

        if let Some(ref ext) = external_id {
            let probe_id = self.store.borrow().peek_next_node_id();
            let mut wtxn = self.catalog.write_txn()?;
            match self
                .catalog
                .external_id_index()
                .put_if_absent(&mut wtxn, ext, probe_id)?
            {
                None => {
                    // Consume the id.
                    let raw_id = self.store.borrow_mut().allocate_node_id();
                    debug_assert_eq!(raw_id, probe_id);
                    let node_id = NodeId::new(raw_id);

                    let record = NodeRecord {
                        label_bits,
                        first_rel_ptr: u64::MAX,
                        prop_ptr: u64::MAX,
                        ..Default::default()
                    };
                    self.store
                        .borrow_mut()
                        .write_node(node_id.value(), &record)?;
                    wtxn.commit()?;

                    let node = Node::new(node_id, labels);
                    self.node_cache.write().insert(node_id, node);
                    return Ok(node_id);
                }
                Some(existing_raw) => {
                    drop(wtxn);
                    return match policy {
                        crate::storage::ConflictPolicy::Error => {
                            Err(crate::error::Error::ExternalIdConflict {
                                existing_internal_id: existing_raw,
                                attempted_external_id: ext.to_string(),
                            })
                        }
                        crate::storage::ConflictPolicy::Match
                        | crate::storage::ConflictPolicy::Replace => Ok(NodeId::new(existing_raw)),
                    };
                }
            }
        }

        // Plain creation (no external id).
        let node_id = self.store.borrow_mut().allocate_node_id();
        let node_id = NodeId::new(node_id);

        let record = NodeRecord {
            label_bits,
            first_rel_ptr: u64::MAX,
            prop_ptr: u64::MAX,
            ..Default::default()
        };
        self.store
            .borrow_mut()
            .write_node(node_id.value(), &record)?;

        let node = Node::new(node_id, labels);
        self.node_cache.write().insert(node_id, node);

        Ok(node_id)
    }

    /// Get a node by ID
    pub fn get_node(&self, node_id: NodeId) -> Result<Option<Node>> {
        // Check cache first
        if let Some(node) = self.node_cache.read().get(&node_id).cloned() {
            return Ok(Some(node));
        }

        // Read from storage
        let record = self.store.borrow().read_node(node_id.value())?;

        if record.is_deleted() {
            return Ok(None);
        }

        // Convert label bits to label names
        let mut labels = Vec::new();
        for i in 0..64 {
            if record.has_label(i) {
                if let Some(label_name) = self.catalog.get_label_name(i)? {
                    labels.push(label_name);
                }
            }
        }

        // Load properties from property chain
        let properties = if record.prop_ptr != u64::MAX {
            self.load_properties(record.prop_ptr)?
        } else {
            HashMap::new()
        };

        let node = Node::with_properties(node_id, labels, properties);

        // Cache the node
        self.node_cache.write().insert(node_id, node.clone());

        Ok(Some(node))
    }

    /// Update a node in the graph
    pub fn update_node(&self, node: Node) -> Result<()> {
        // Get or create label IDs
        let mut label_bits = 0u64;
        for label in &node.labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
        }

        // Read existing record to preserve relationships and properties
        let existing_record = self
            .store
            .borrow()
            .read_node(node.id.value())
            .unwrap_or_else(|_| NodeRecord::new());

        // Store properties in the property store
        let prop_ptr = if node.properties.is_empty() {
            existing_record.prop_ptr // Keep existing properties if no new ones
        } else {
            let mut property_store = self.property_store.write();
            property_store.store_properties(node.properties.clone())
        };

        // Create updated record preserving existing relationships and properties
        let record = NodeRecord {
            label_bits,
            first_rel_ptr: existing_record.first_rel_ptr, // Preserve existing relationships
            prop_ptr,
            ..existing_record
        };

        // Write to storage
        self.store
            .borrow_mut()
            .write_node(node.id.value(), &record)?;

        // Update cache
        self.node_cache.write().insert(node.id, node);

        Ok(())
    }

    /// Delete a node from the graph
    pub fn delete_node(&self, node_id: NodeId) -> Result<bool> {
        // Check if node exists
        if self.get_node(node_id)?.is_none() {
            return Ok(false);
        }

        // Mark as deleted in storage
        let mut record = self.store.borrow().read_node(node_id.value())?;
        record.mark_deleted();
        self.store
            .borrow_mut()
            .write_node(node_id.value(), &record)?;

        // Remove from cache
        self.node_cache.write().remove(&node_id);

        Ok(true)
    }

    /// Create a new edge in the graph
    pub fn create_edge(
        &self,
        source: NodeId,
        target: NodeId,
        relationship_type: String,
    ) -> Result<EdgeId> {
        // Verify both nodes exist
        if self.get_node(source)?.is_none() {
            return Err(Error::NotFound(format!(
                "Source node {} not found",
                source.value()
            )));
        }
        if self.get_node(target)?.is_none() {
            return Err(Error::NotFound(format!(
                "Target node {} not found",
                target.value()
            )));
        }

        // Allocate a new edge ID
        let edge_id = self.store.borrow_mut().allocate_rel_id();
        let edge_id = EdgeId::new(edge_id);

        // Get or create relationship type ID
        let type_id = self.catalog.get_or_create_type(&relationship_type)?;

        // Get the current first relationships for source and destination nodes
        let src_first_rel = self.get_first_relationship(source, true)?; // outgoing
        let dst_first_rel = self.get_first_relationship(target, false)?; // incoming

        // Create relationship record with proper linking
        let record = RelationshipRecord {
            src_id: source.value(),
            dst_id: target.value(),
            type_id,
            next_src_ptr: src_first_rel.unwrap_or(u64::MAX), // Link to current first outgoing rel from source
            next_dst_ptr: dst_first_rel.unwrap_or(u64::MAX), // Link to current first incoming rel to dest
            prop_ptr: u64::MAX,                              // No properties yet
            ..Default::default()
        };

        // Write to storage
        self.store
            .borrow_mut()
            .write_rel(edge_id.value(), &record)?;

        // Update node pointers to point to this new relationship
        self.update_node_relationship_pointers(source, target, edge_id.value())?;

        // Create in-memory edge
        let edge = Edge::new(edge_id, source, target, relationship_type);
        self.edge_cache.write().insert(edge_id, edge);

        Ok(edge_id)
    }

    /// Get an edge by ID
    pub fn get_edge(&self, edge_id: EdgeId) -> Result<Option<Edge>> {
        // Check cache first
        if let Some(edge) = self.edge_cache.read().get(&edge_id).cloned() {
            return Ok(Some(edge));
        }

        // Read from storage
        let record = self.store.borrow().read_rel(edge_id.value())?;

        if record.is_deleted() {
            return Ok(None);
        }

        // Get relationship type name
        let type_id = record.type_id; // Copy to avoid packed struct reference
        let relationship_type = self
            .catalog
            .get_type_name(type_id)?
            .unwrap_or_else(|| format!("Type_{}", type_id));

        // Load properties from property chain
        let properties = if record.prop_ptr != u64::MAX {
            self.load_properties(record.prop_ptr)?
        } else {
            HashMap::new()
        };

        let edge = Edge::with_properties(
            edge_id,
            NodeId::new(record.src_id),
            NodeId::new(record.dst_id),
            relationship_type,
            properties,
        );

        // Cache the edge
        self.edge_cache.write().insert(edge_id, edge.clone());

        Ok(Some(edge))
    }

    /// Update an edge in the graph
    pub fn update_edge(&self, edge: Edge) -> Result<()> {
        // Get or create relationship type ID
        let type_id = self.catalog.get_or_create_type(&edge.relationship_type)?;

        // Read existing record to preserve relationship chain pointers
        let existing_record = self
            .store
            .borrow()
            .read_rel(edge.id.value())
            .unwrap_or_else(|_| RelationshipRecord::new(0, 0, 0));

        // Create updated record preserving relationship chain pointers
        // Store properties in the property store
        let prop_ptr = if edge.properties.is_empty() {
            existing_record.prop_ptr // Keep existing properties if no new ones
        } else {
            let mut property_store = self.property_store.write();
            property_store.store_properties(edge.properties.clone())
        };

        let record = RelationshipRecord {
            src_id: edge.source.value(),
            dst_id: edge.target.value(),
            type_id,
            next_src_ptr: existing_record.next_src_ptr, // Preserve existing relationship chain
            next_dst_ptr: existing_record.next_dst_ptr, // Preserve existing relationship chain
            prop_ptr,
            ..existing_record
        };

        // Write to storage
        self.store
            .borrow_mut()
            .write_rel(edge.id.value(), &record)?;

        // Update cache
        self.edge_cache.write().insert(edge.id, edge);

        Ok(())
    }

    /// Delete an edge from the graph
    pub fn delete_edge(&self, edge_id: EdgeId) -> Result<bool> {
        // Check if edge exists
        if self.get_edge(edge_id)?.is_none() {
            return Ok(false);
        }

        // Mark as deleted in storage
        let mut record = self.store.borrow().read_rel(edge_id.value())?;
        record.mark_deleted();
        self.store
            .borrow_mut()
            .write_rel(edge_id.value(), &record)?;

        // Remove from cache
        self.edge_cache.write().remove(&edge_id);

        Ok(true)
    }

    /// Get all nodes in the graph
    pub fn get_all_nodes(&self) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        let stats = self.store.borrow_mut().stats();

        for node_id in 0..stats.node_count {
            if let Some(node) = self.get_node(NodeId::new(node_id))? {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Get all edges in the graph
    pub fn get_all_edges(&self) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        let stats = self.store.borrow_mut().stats();

        for edge_id in 0..stats.rel_count {
            if let Some(edge) = self.get_edge(EdgeId::new(edge_id))? {
                edges.push(edge);
            }
        }

        Ok(edges)
    }

    /// Get nodes with a specific label
    pub fn get_nodes_by_label(&self, label: &str) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        let all_nodes = self.get_all_nodes()?;

        for node in all_nodes {
            if node.has_label(label) {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Get edges of a specific type
    pub fn get_edges_by_type(&self, relationship_type: &str) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        let all_edges = self.get_all_edges()?;

        for edge in all_edges {
            if edge.relationship_type == relationship_type {
                edges.push(edge);
            }
        }

        Ok(edges)
    }

    /// Get edges connected to a specific node
    pub fn get_edges_for_node(&self, node_id: NodeId) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        let all_edges = self.get_all_edges()?;

        for edge in all_edges {
            if edge.source == node_id || edge.target == node_id {
                edges.push(edge);
            }
        }

        Ok(edges)
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> Result<usize> {
        Ok(self.get_all_nodes()?.len())
    }

    /// Get the number of edges in the graph
    pub fn edge_count(&self) -> Result<usize> {
        Ok(self.get_all_edges()?.len())
    }

    /// Clear all caches
    pub fn clear_cache(&self) {
        self.node_cache.write().clear();
        self.edge_cache.write().clear();
    }

    /// Get graph statistics
    pub fn stats(&self) -> Result<GraphStats> {
        let store_stats = self.store.borrow().stats();
        let node_count = self.node_count()?;
        let edge_count = self.edge_count()?;

        // Calculate advanced statistics
        let degree_stats = self.calculate_degree_statistics()?;
        let density = self.calculate_graph_density(node_count, edge_count)?;
        let connected_components = self.calculate_connected_components()?;
        let clustering_coefficient = self.calculate_avg_clustering_coefficient()?;
        let path_stats = self.calculate_path_statistics()?;
        let node_type_stats = self.calculate_node_type_statistics()?;
        let edge_type_stats = self.calculate_edge_type_statistics()?;

        Ok(GraphStats {
            total_nodes: node_count,
            total_edges: edge_count,
            storage_nodes: store_stats.node_count as usize,
            storage_edges: store_stats.rel_count as usize,
            cached_nodes: self.node_cache.read().len(),
            cached_edges: self.edge_cache.read().len(),
            avg_degree: degree_stats.avg_degree,
            max_degree: degree_stats.max_degree,
            min_degree: degree_stats.min_degree,
            graph_density: density,
            connected_components,
            avg_clustering_coefficient: clustering_coefficient,
            avg_shortest_path_length: path_stats.avg_shortest_path_length,
            diameter: path_stats.diameter,
            isolated_nodes: node_type_stats.isolated_nodes,
            leaf_nodes: node_type_stats.leaf_nodes,
            self_loops: edge_type_stats.self_loops,
            bidirectional_edges: edge_type_stats.bidirectional_edges,
        })
    }

    /// Validate the entire graph for integrity and consistency
    pub fn validate(&self) -> Result<crate::validation::ValidationResult> {
        let validator = crate::validation::GraphValidator::new();
        validator.validate_graph(self)
    }

    /// Calculate degree statistics for all nodes
    fn calculate_degree_statistics(&self) -> Result<DegreeStats> {
        let nodes = self.get_all_nodes()?;
        let edges = self.get_all_edges()?;

        if nodes.is_empty() {
            return Ok(DegreeStats {
                avg_degree: 0.0,
                max_degree: 0,
                min_degree: 0,
            });
        }

        let mut degrees = Vec::new();
        for node in &nodes {
            let degree = edges
                .iter()
                .filter(|edge| edge.source == node.id || edge.target == node.id)
                .count();
            degrees.push(degree);
        }

        let total_degree: usize = degrees.iter().sum();
        let avg_degree = total_degree as f64 / nodes.len() as f64;
        let max_degree = degrees.iter().max().copied().unwrap_or(0);
        let min_degree = degrees.iter().min().copied().unwrap_or(0);

        Ok(DegreeStats {
            avg_degree,
            max_degree,
            min_degree,
        })
    }

    /// Calculate graph density
    fn calculate_graph_density(&self, node_count: usize, edge_count: usize) -> Result<f64> {
        if node_count <= 1 {
            return Ok(0.0);
        }

        // For undirected graph: max edges = n * (n-1) / 2
        // For directed graph: max edges = n * (n-1)
        // We'll assume undirected for now
        let max_edges = node_count * (node_count - 1) / 2;
        Ok(edge_count as f64 / max_edges as f64)
    }

    /// Calculate number of connected components using DFS
    fn calculate_connected_components(&self) -> Result<usize> {
        let nodes = self.get_all_nodes()?;
        if nodes.is_empty() {
            return Ok(0);
        }

        let mut visited = std::collections::HashSet::new();
        let mut components = 0;

        for node in &nodes {
            if !visited.contains(&node.id) {
                self.dfs_component(node.id, &mut visited)?;
                components += 1;
            }
        }

        Ok(components)
    }

    /// DFS helper for connected components
    fn dfs_component(
        &self,
        node_id: NodeId,
        visited: &mut std::collections::HashSet<NodeId>,
    ) -> Result<()> {
        visited.insert(node_id);
        let edges = self.get_edges_for_node(node_id)?;

        for edge in &edges {
            let neighbor = if edge.source == node_id {
                edge.target
            } else {
                edge.source
            };
            if !visited.contains(&neighbor) {
                self.dfs_component(neighbor, visited)?;
            }
        }
        Ok(())
    }

    /// Calculate average clustering coefficient
    fn calculate_avg_clustering_coefficient(&self) -> Result<f64> {
        let nodes = self.get_all_nodes()?;
        if nodes.is_empty() {
            return Ok(0.0);
        }

        let mut total_coefficient = 0.0;
        let mut valid_nodes = 0;

        for node in &nodes {
            let edges = self.get_edges_for_node(node.id)?;
            let degree = edges.len();

            if degree < 2 {
                continue; // Skip nodes with degree < 2
            }

            // Find neighbors
            let neighbors: Vec<NodeId> = edges
                .iter()
                .map(|edge| {
                    if edge.source == node.id {
                        edge.target
                    } else {
                        edge.source
                    }
                })
                .collect();

            // Count edges between neighbors
            let mut neighbor_edges = 0;
            for i in 0..neighbors.len() {
                for j in (i + 1)..neighbors.len() {
                    let neighbor_edges_list = self.get_edges_for_node(neighbors[i])?;
                    if neighbor_edges_list.iter().any(|e| {
                        (e.source == neighbors[i] && e.target == neighbors[j])
                            || (e.source == neighbors[j] && e.target == neighbors[i])
                    }) {
                        neighbor_edges += 1;
                    }
                }
            }

            let max_possible_edges = degree * (degree - 1) / 2;
            let coefficient = if max_possible_edges > 0 {
                neighbor_edges as f64 / max_possible_edges as f64
            } else {
                0.0
            };

            total_coefficient += coefficient;
            valid_nodes += 1;
        }

        Ok(if valid_nodes > 0 {
            total_coefficient / valid_nodes as f64
        } else {
            0.0
        })
    }

    /// Calculate path statistics (average shortest path length and diameter)
    fn calculate_path_statistics(&self) -> Result<PathStats> {
        let nodes = self.get_all_nodes()?;
        if nodes.is_empty() {
            return Ok(PathStats {
                avg_shortest_path_length: 0.0,
                diameter: 0,
            });
        }

        let mut total_path_length = 0.0;
        let mut path_count = 0;
        let mut max_path_length = 0;

        // Use BFS to find shortest paths from each node
        for start_node in &nodes {
            let distances = self.bfs_shortest_paths(start_node.id)?;

            for (_, distance) in distances {
                if distance > 0 {
                    // Exclude distance to self
                    total_path_length += distance as f64;
                    path_count += 1;
                    max_path_length = max_path_length.max(distance);
                }
            }
        }

        let avg_shortest_path_length = if path_count > 0 {
            total_path_length / path_count as f64
        } else {
            0.0
        };

        Ok(PathStats {
            avg_shortest_path_length,
            diameter: max_path_length,
        })
    }

    /// BFS helper for shortest paths
    fn bfs_shortest_paths(
        &self,
        start: NodeId,
    ) -> Result<std::collections::HashMap<NodeId, usize>> {
        let mut distances = std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::new();

        distances.insert(start, 0);
        queue.push_back(start);

        while let Some(current) = queue.pop_front() {
            let current_distance = distances[&current];
            let edges = self.get_edges_for_node(current)?;

            for edge in &edges {
                let neighbor = if edge.source == current {
                    edge.target
                } else {
                    edge.source
                };

                if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor) {
                    e.insert(current_distance + 1);
                    queue.push_back(neighbor);
                }
            }
        }

        Ok(distances)
    }

    /// Calculate node type statistics
    fn calculate_node_type_statistics(&self) -> Result<NodeTypeStats> {
        let nodes = self.get_all_nodes()?;
        let edges = self.get_all_edges()?;

        let mut isolated_nodes = 0;
        let mut leaf_nodes = 0;

        for node in &nodes {
            let degree = edges
                .iter()
                .filter(|edge| edge.source == node.id || edge.target == node.id)
                .count();

            if degree == 0 {
                isolated_nodes += 1;
            } else if degree == 1 {
                leaf_nodes += 1;
            }
        }

        Ok(NodeTypeStats {
            isolated_nodes,
            leaf_nodes,
        })
    }

    /// Calculate edge type statistics
    fn calculate_edge_type_statistics(&self) -> Result<EdgeTypeStats> {
        let edges = self.get_all_edges()?;

        let mut self_loops = 0;
        let mut bidirectional_edges = 0;

        for edge in &edges {
            if edge.source == edge.target {
                self_loops += 1;
            } else {
                // Check if there's a reverse edge
                let has_reverse = edges
                    .iter()
                    .any(|e| e.source == edge.target && e.target == edge.source);
                if has_reverse {
                    bidirectional_edges += 1;
                }
            }
        }

        // Divide by 2 since we count each bidirectional pair twice
        bidirectional_edges /= 2;

        Ok(EdgeTypeStats {
            self_loops,
            bidirectional_edges,
        })
    }

    /// Validate the graph with custom configuration
    pub fn validate_with_config(
        &self,
        config: crate::validation::ValidationConfig,
    ) -> Result<crate::validation::ValidationResult> {
        let validator = crate::validation::GraphValidator::with_config(config);
        validator.validate_graph(self)
    }

    /// Quick health check for the graph
    pub fn health_check(&self) -> Result<bool> {
        let result = self.validate()?;
        Ok(result.is_valid)
    }

    /// Get validation statistics without full validation
    pub fn validation_stats(&self) -> Result<crate::validation::ValidationStats> {
        let result = self.validate()?;
        Ok(result.stats)
    }

    /// Load properties from the property chain
    fn load_properties(&self, prop_ptr: u64) -> Result<HashMap<String, PropertyValue>> {
        // If prop_ptr is u64::MAX, there are no properties
        if prop_ptr == u64::MAX {
            return Ok(HashMap::new());
        }

        // Use the property store to load properties from the chain
        let property_store = self.property_store.read();
        property_store.load_properties(prop_ptr)
    }

    /// Get the first relationship for a node in the specified direction
    fn get_first_relationship(&self, _node_id: NodeId, _outgoing: bool) -> Result<Option<u64>> {
        // For now, we'll return None to indicate no existing relationships
        // In a full implementation, this would check the node's relationship pointers
        // stored in the node record or a separate index
        Ok(None)
    }

    /// Update node relationship pointers to point to the new relationship
    fn update_node_relationship_pointers(
        &self,
        _source: NodeId,
        _target: NodeId,
        _rel_id: u64,
    ) -> Result<()> {
        // For now, we'll do nothing as we don't have node relationship pointers implemented
        // In a full implementation, this would:
        // 1. Update the source node's first_outgoing_rel pointer to point to rel_id
        // 2. Update the target node's first_incoming_rel pointer to point to rel_id
        // 3. This would require modifying the node record structure to include these pointers
        Ok(())
    }
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("node_cache", &self.node_cache.read().len())
            .field("edge_cache", &self.edge_cache.read().len())
            .finish()
    }
}
