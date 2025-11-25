//! Core graph data structures - Graph, Node, Edge
//!
//! This module provides high-level graph data structures that wrap the low-level
//! storage records and provide a more user-friendly API for graph operations.

use crate::graph::simple::PropertyValue;
use crate::storage::{NodeRecord, RecordStore, RelationshipRecord};
use crate::{Error, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Property record for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PropertyRecord {
    /// Key of the property
    key: String,
    /// Value of the property
    value: PropertyValue,
    /// Pointer to next property in chain (u64::MAX if last)
    next_ptr: u64,
}

/// Property storage for managing property chains
#[derive(Debug)]
struct PropertyStore {
    /// In-memory property storage (in real implementation, this would be persistent)
    properties: HashMap<u64, PropertyRecord>,
    /// Next available property pointer
    next_ptr: u64,
}

impl PropertyStore {
    fn new() -> Self {
        Self {
            properties: HashMap::new(),
            next_ptr: 1,
        }
    }

    /// Store a property chain and return the head pointer
    fn store_properties(&mut self, properties: HashMap<String, PropertyValue>) -> u64 {
        if properties.is_empty() {
            return u64::MAX;
        }

        let mut current_ptr = u64::MAX;

        // Store properties in reverse order to maintain chain structure
        let mut prop_vec: Vec<_> = properties.into_iter().collect();
        prop_vec.reverse();

        for (key, value) in prop_vec {
            let ptr = self.next_ptr;
            self.next_ptr += 1;

            let record = PropertyRecord {
                key,
                value,
                next_ptr: current_ptr,
            };

            self.properties.insert(ptr, record);
            current_ptr = ptr;
        }

        current_ptr
    }

    /// Load properties from a property chain
    fn load_properties(&self, head_ptr: u64) -> Result<HashMap<String, PropertyValue>> {
        let mut properties = HashMap::new();
        let mut current_ptr = head_ptr;

        while current_ptr != u64::MAX {
            if let Some(record) = self.properties.get(&current_ptr) {
                properties.insert(record.key.clone(), record.value.clone());
                current_ptr = record.next_ptr;
            } else {
                return Err(Error::Storage(format!(
                    "Property record not found at pointer {}",
                    current_ptr
                )));
            }
        }

        Ok(properties)
    }
}

/// A unique identifier for nodes in the graph
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct NodeId(pub u64);

impl NodeId {
    /// Create a new node ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for NodeId {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

impl From<NodeId> for u64 {
    fn from(node_id: NodeId) -> Self {
        node_id.0
    }
}

/// A unique identifier for relationships in the graph
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct EdgeId(pub u64);

impl EdgeId {
    /// Create a new edge ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for EdgeId {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

impl From<EdgeId> for u64 {
    fn from(edge_id: EdgeId) -> Self {
        edge_id.0
    }
}

/// A node in the graph with labels and properties
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// Unique identifier for this node
    pub id: NodeId,
    /// Labels associated with this node
    pub labels: Vec<String>,
    /// Properties of this node
    pub properties: HashMap<String, PropertyValue>,
}

impl Node {
    /// Create a new node with the given ID and labels
    pub fn new(id: NodeId, labels: Vec<String>) -> Self {
        Self {
            id,
            labels,
            properties: HashMap::new(),
        }
    }

    /// Create a new node with ID, labels, and properties
    pub fn with_properties(
        id: NodeId,
        labels: Vec<String>,
        properties: HashMap<String, PropertyValue>,
    ) -> Self {
        Self {
            id,
            labels,
            properties,
        }
    }

    /// Add a label to this node
    pub fn add_label(&mut self, label: String) {
        if !self.labels.contains(&label) {
            self.labels.push(label);
        }
    }

    /// Remove a label from this node
    pub fn remove_label(&mut self, label: &str) -> bool {
        if let Some(pos) = self.labels.iter().position(|l| l == label) {
            self.labels.remove(pos);
            true
        } else {
            false
        }
    }

    /// Check if this node has a specific label
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.contains(&label.to_string())
    }

    /// Set a property on this node
    pub fn set_property(&mut self, key: String, value: PropertyValue) {
        self.properties.insert(key, value);
    }

    /// Get a property from this node
    pub fn get_property(&self, key: &str) -> Option<&PropertyValue> {
        self.properties.get(key)
    }

    /// Remove a property from this node
    pub fn remove_property(&mut self, key: &str) -> Option<PropertyValue> {
        self.properties.remove(key)
    }

    /// Check if this node has a specific property
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    /// Get all property keys
    pub fn property_keys(&self) -> Vec<&String> {
        self.properties.keys().collect()
    }

    /// Check if this node is empty (no labels and no properties)
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty() && self.properties.is_empty()
    }
}

/// An edge (relationship) in the graph connecting two nodes
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Unique identifier for this edge
    pub id: EdgeId,
    /// Source node ID
    pub source: NodeId,
    /// Target node ID
    pub target: NodeId,
    /// Type of this relationship
    pub relationship_type: String,
    /// Properties of this edge
    pub properties: HashMap<String, PropertyValue>,
}

impl Edge {
    /// Create a new edge with the given ID, source, target, and type
    pub fn new(id: EdgeId, source: NodeId, target: NodeId, relationship_type: String) -> Self {
        Self {
            id,
            source,
            target,
            relationship_type,
            properties: HashMap::new(),
        }
    }

    /// Create a new edge with ID, source, target, type, and properties
    pub fn with_properties(
        id: EdgeId,
        source: NodeId,
        target: NodeId,
        relationship_type: String,
        properties: HashMap<String, PropertyValue>,
    ) -> Self {
        Self {
            id,
            source,
            target,
            relationship_type,
            properties,
        }
    }

    /// Set a property on this edge
    pub fn set_property(&mut self, key: String, value: PropertyValue) {
        self.properties.insert(key, value);
    }

    /// Get a property from this edge
    pub fn get_property(&self, key: &str) -> Option<&PropertyValue> {
        self.properties.get(key)
    }

    /// Remove a property from this edge
    pub fn remove_property(&mut self, key: &str) -> Option<PropertyValue> {
        self.properties.remove(key)
    }

    /// Check if this edge has a specific property
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    /// Get all property keys
    pub fn property_keys(&self) -> Vec<&String> {
        self.properties.keys().collect()
    }

    /// Check if this edge is empty (no properties)
    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    /// Get the other end of this edge given one node
    pub fn other_end(&self, node_id: NodeId) -> Option<NodeId> {
        if self.source == node_id {
            Some(self.target)
        } else if self.target == node_id {
            Some(self.source)
        } else {
            None
        }
    }
}

/// A graph containing nodes and edges
pub struct Graph {
    /// Storage backend for persistence
    store: std::cell::RefCell<RecordStore>,
    /// Catalog for label/type/key mappings
    catalog: Arc<crate::catalog::Catalog>,
    /// Cache of loaded nodes (in-memory)
    node_cache: Arc<RwLock<HashMap<NodeId, Node>>>,
    /// Cache of loaded edges (in-memory)
    edge_cache: Arc<RwLock<HashMap<EdgeId, Edge>>>,
    /// Property storage for managing property chains
    property_store: Arc<RwLock<PropertyStore>>,
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
        // Allocate a new node ID
        let node_id = self.store.borrow_mut().allocate_node_id();
        let node_id = NodeId::new(node_id);

        // Get or create label IDs
        let mut label_bits = 0u64;
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
        }

        // Create node record
        let record = NodeRecord {
            label_bits,
            first_rel_ptr: u64::MAX, // No relationships yet
            prop_ptr: u64::MAX,      // No properties yet
            ..Default::default()
        };

        // Write to storage
        self.store
            .borrow_mut()
            .write_node(node_id.value(), &record)?;

        // Create in-memory node
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

/// Helper structures for statistics calculation
#[derive(Debug, Clone)]
struct DegreeStats {
    avg_degree: f64,
    max_degree: usize,
    min_degree: usize,
}

#[derive(Debug, Clone)]
struct PathStats {
    avg_shortest_path_length: f64,
    diameter: usize,
}

#[derive(Debug, Clone)]
struct NodeTypeStats {
    isolated_nodes: usize,
    leaf_nodes: usize,
}

#[derive(Debug, Clone)]
struct EdgeTypeStats {
    self_loops: usize,
    bidirectional_edges: usize,
}

/// Statistics about the graph
#[derive(Debug, Clone)]
pub struct GraphStats {
    /// Number of active nodes in the graph
    pub total_nodes: usize,
    /// Number of active edges in the graph
    pub total_edges: usize,
    /// Number of nodes in storage (including deleted)
    pub storage_nodes: usize,
    /// Number of edges in storage (including deleted)
    pub storage_edges: usize,
    /// Number of nodes in cache
    pub cached_nodes: usize,
    /// Number of edges in cache
    pub cached_edges: usize,
    /// Average degree (connections per node)
    pub avg_degree: f64,
    /// Maximum degree of any node
    pub max_degree: usize,
    /// Minimum degree of any node
    pub min_degree: usize,
    /// Graph density (actual edges / possible edges)
    pub graph_density: f64,
    /// Number of connected components
    pub connected_components: usize,
    /// Average clustering coefficient
    pub avg_clustering_coefficient: f64,
    /// Average shortest path length
    pub avg_shortest_path_length: f64,
    /// Graph diameter (longest shortest path)
    pub diameter: usize,
    /// Number of isolated nodes (degree 0)
    pub isolated_nodes: usize,
    /// Number of leaf nodes (degree 1)
    pub leaf_nodes: usize,
    /// Number of self-loops
    pub self_loops: usize,
    /// Number of bidirectional edges
    pub bidirectional_edges: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use tempfile::TempDir;

    fn create_test_graph() -> (Graph, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RecordStore::new(dir.path()).unwrap();
        let catalog = Arc::new(Catalog::new(dir.path().join("catalog")).unwrap());
        let graph = Graph::new(store, catalog);
        (graph, dir)
    }

    #[test]
    fn test_node_creation() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        assert_eq!(node_id.value(), 0);

        let node = graph.get_node(node_id).unwrap().unwrap();
        assert_eq!(node.id, node_id);
        assert!(node.has_label("Person"));
        assert_eq!(node.labels.len(), 1);
    }

    #[test]
    fn test_node_with_multiple_labels() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph
            .create_node(vec!["Person".to_string(), "Employee".to_string()])
            .unwrap();

        let node = graph.get_node(node_id).unwrap().unwrap();
        assert!(node.has_label("Person"));
        assert!(node.has_label("Employee"));
        assert_eq!(node.labels.len(), 2);
    }

    #[test]
    fn test_node_properties() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        node.set_property(
            "name".to_string(),
            PropertyValue::String("test".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));

        assert!(node.has_property("name"));
        assert!(node.has_property("age"));
        assert_eq!(node.property_keys().len(), 2);
    }

    #[test]
    fn test_edge_creation() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();

        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();
        assert_eq!(edge_id.value(), 0);

        let edge = graph.get_edge(edge_id).unwrap().unwrap();
        assert_eq!(edge.id, edge_id);
        assert_eq!(edge.source, source_id);
        assert_eq!(edge.target, target_id);
        assert_eq!(edge.relationship_type, "KNOWS");
    }

    #[test]
    fn test_edge_properties() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let mut edge = graph.get_edge(edge_id).unwrap().unwrap();
        edge.set_property("since".to_string(), PropertyValue::Int64(2020));

        assert!(edge.has_property("since"));
        assert_eq!(edge.property_keys().len(), 1);
    }

    #[test]
    fn test_node_deletion() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        assert!(graph.get_node(node_id).unwrap().is_some());

        let deleted = graph.delete_node(node_id).unwrap();
        assert!(deleted);
        assert!(graph.get_node(node_id).unwrap().is_none());
    }

    #[test]
    fn test_edge_deletion() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        assert!(graph.get_edge(edge_id).unwrap().is_some());

        let deleted = graph.delete_edge(edge_id).unwrap();
        assert!(deleted);
        assert!(graph.get_edge(edge_id).unwrap().is_none());
    }

    #[test]
    fn test_get_nodes_by_label() {
        let (graph, _dir) = create_test_graph();

        let _person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _company = graph.create_node(vec!["Company".to_string()]).unwrap();

        let person_nodes = graph.get_nodes_by_label("Person").unwrap();
        assert_eq!(person_nodes.len(), 2);

        let company_nodes = graph.get_nodes_by_label("Company").unwrap();
        assert_eq!(company_nodes.len(), 1);
    }

    #[test]
    fn test_get_edges_by_type() {
        let (graph, _dir) = create_test_graph();

        let person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let company = graph.create_node(vec!["Company".to_string()]).unwrap();

        let _knows_edge = graph
            .create_edge(person1, person2, "KNOWS".to_string())
            .unwrap();
        let _works_edge = graph
            .create_edge(person1, company, "WORKS_AT".to_string())
            .unwrap();

        let knows_edges = graph.get_edges_by_type("KNOWS").unwrap();
        assert_eq!(knows_edges.len(), 1);

        let works_edges = graph.get_edges_by_type("WORKS_AT").unwrap();
        assert_eq!(works_edges.len(), 1);
    }

    #[test]
    fn test_get_edges_for_node() {
        let (graph, _dir) = create_test_graph();

        let person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let person3 = graph.create_node(vec!["Person".to_string()]).unwrap();

        let _edge1 = graph
            .create_edge(person1, person2, "KNOWS".to_string())
            .unwrap();
        let _edge2 = graph
            .create_edge(person1, person3, "KNOWS".to_string())
            .unwrap();

        let edges = graph.get_edges_for_node(person1).unwrap();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_graph_stats() {
        let (graph, _dir) = create_test_graph();

        let _person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _edge = graph
            .create_edge(NodeId::new(0), NodeId::new(1), "KNOWS".to_string())
            .unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_edges, 1);
    }

    #[test]
    fn test_node_label_operations() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Add label
        node.add_label("Employee".to_string());
        assert!(node.has_label("Employee"));
        assert_eq!(node.labels.len(), 2);

        // Remove label
        let removed = node.remove_label("Person");
        assert!(removed);
        assert!(!node.has_label("Person"));
        assert!(node.has_label("Employee"));
        assert_eq!(node.labels.len(), 1);

        // Try to remove non-existent label
        let not_removed = node.remove_label("NonExistent");
        assert!(!not_removed);
    }

    #[test]
    fn test_edge_other_end() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let edge = graph.get_edge(edge_id).unwrap().unwrap();

        assert_eq!(edge.other_end(source_id), Some(target_id));
        assert_eq!(edge.other_end(target_id), Some(source_id));
        assert_eq!(edge.other_end(NodeId::new(999)), None);
    }

    #[test]
    fn test_node_property_operations() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Set properties
        node.set_property(
            "name".to_string(),
            PropertyValue::String("test".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));

        // Check properties
        assert!(node.has_property("name"));
        assert!(node.has_property("age"));
        assert_eq!(node.property_keys().len(), 2);

        // Get property
        let age = node.get_property("age").unwrap();
        assert_eq!(age, &PropertyValue::Int64(30));

        // Remove property
        let removed = node.remove_property("age");
        assert_eq!(removed, Some(PropertyValue::Int64(30)));
        assert!(!node.has_property("age"));
        assert!(node.has_property("name"));
    }

    #[test]
    fn test_edge_property_operations() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let mut edge = graph.get_edge(edge_id).unwrap().unwrap();

        // Set properties
        edge.set_property("since".to_string(), PropertyValue::Int64(2020));
        edge.set_property("strength".to_string(), PropertyValue::Float64(0.8));

        // Check properties
        assert!(edge.has_property("since"));
        assert!(edge.has_property("strength"));
        assert_eq!(edge.property_keys().len(), 2);

        // Get property
        let since = edge.get_property("since").unwrap();
        assert_eq!(since, &PropertyValue::Int64(2020));

        // Remove property
        let removed = edge.remove_property("strength");
        assert_eq!(removed, Some(PropertyValue::Float64(0.8)));
        assert!(!edge.has_property("strength"));
        assert!(edge.has_property("since"));
    }

    #[test]
    fn test_property_chain_traversal() {
        let (graph, _dir) = create_test_graph();

        // Create a node with properties
        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Set multiple properties
        node.set_property(
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));
        node.set_property("active".to_string(), PropertyValue::Bool(true));

        // Update the node to store properties
        graph.update_node(node).unwrap();

        // Retrieve the node and verify properties are loaded from the chain
        let retrieved_node = graph.get_node(node_id).unwrap().unwrap();

        // The properties should be loaded from the property chain
        assert!(retrieved_node.has_property("name"));
        assert!(retrieved_node.has_property("age"));
        assert!(retrieved_node.has_property("active"));

        assert_eq!(
            retrieved_node.get_property("name"),
            Some(&PropertyValue::String("Alice".to_string()))
        );
        assert_eq!(
            retrieved_node.get_property("age"),
            Some(&PropertyValue::Int64(30))
        );
        assert_eq!(
            retrieved_node.get_property("active"),
            Some(&PropertyValue::Bool(true))
        );
    }

    #[test]
    fn test_node_is_empty() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec![]).unwrap();
        let node = graph.get_node(node_id).unwrap().unwrap();
        assert!(node.is_empty());

        let mut node_with_label = graph.get_node(node_id).unwrap().unwrap();
        node_with_label.add_label("Person".to_string());
        assert!(!node_with_label.is_empty());
    }

    #[test]
    fn test_edge_is_empty() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let edge = graph.get_edge(edge_id).unwrap().unwrap();
        assert!(edge.is_empty());

        let mut edge_with_props = graph.get_edge(edge_id).unwrap().unwrap();
        edge_with_props.set_property("since".to_string(), PropertyValue::Int64(2020));
        assert!(!edge_with_props.is_empty());
    }

    #[test]
    fn test_clear_cache() {
        let (graph, _dir) = create_test_graph();

        let node_id1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let node_id2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _edge_id = graph
            .create_edge(node_id1, node_id2, "KNOWS".to_string())
            .unwrap();

        // Verify cache has entries
        assert!(!graph.node_cache.read().is_empty());
        assert!(!graph.edge_cache.read().is_empty());

        // Clear cache
        graph.clear_cache();

        // Verify cache is empty
        assert!(graph.node_cache.read().is_empty());
        assert!(graph.edge_cache.read().is_empty());
    }
}
