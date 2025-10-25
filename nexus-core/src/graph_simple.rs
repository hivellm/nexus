//! Simplified graph data structures for testing
//! This module provides the core graph data structures without storage dependencies

use crate::error::{Error, Result};
use std::collections::HashMap;

/// A unique identifier for nodes in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// Property value types for simplified testing
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// 64-bit integer value
    Int64(i64),
    /// 64-bit floating point value
    Float64(f64),
    /// String value
    String(String),
    /// Bytes value
    Bytes(Vec<u8>),
}

/// A node in the graph with labels and properties
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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

/// A simple in-memory graph containing nodes and edges
pub struct Graph {
    /// Cache of loaded nodes (in-memory)
    nodes: HashMap<NodeId, Node>,
    /// Cache of loaded edges (in-memory)
    edges: HashMap<EdgeId, Edge>,
    /// Next node ID counter
    next_node_id: u64,
    /// Next edge ID counter
    next_edge_id: u64,
}

impl Graph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            next_node_id: 0,
            next_edge_id: 0,
        }
    }

    /// Create a new node in the graph
    pub fn create_node(&mut self, labels: Vec<String>) -> Result<NodeId> {
        let node_id = NodeId::new(self.next_node_id);
        self.next_node_id += 1;

        let node = Node::new(node_id, labels);
        self.nodes.insert(node_id, node);

        Ok(node_id)
    }

    /// Get a node by ID
    pub fn get_node(&self, node_id: NodeId) -> Result<Option<&Node>> {
        Ok(self.nodes.get(&node_id))
    }

    /// Get a mutable reference to a node by ID
    pub fn get_node_mut(&mut self, node_id: NodeId) -> Result<Option<&mut Node>> {
        Ok(self.nodes.get_mut(&node_id))
    }

    /// Update a node in the graph
    pub fn update_node(&mut self, node: Node) -> Result<()> {
        self.nodes.insert(node.id, node);
        Ok(())
    }

    /// Delete a node from the graph
    pub fn delete_node(&mut self, node_id: NodeId) -> Result<bool> {
        if self.nodes.remove(&node_id).is_some() {
            // Also remove all edges connected to this node
            self.edges.retain(|_, edge| edge.source != node_id && edge.target != node_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Create a new edge in the graph
    pub fn create_edge(
        &mut self,
        source: NodeId,
        target: NodeId,
        relationship_type: String,
    ) -> Result<EdgeId> {
        // Verify both nodes exist
        if !self.nodes.contains_key(&source) {
            return Err(Error::NotFound(format!(
                "Source node {} not found",
                source.value()
            )));
        }
        if !self.nodes.contains_key(&target) {
            return Err(Error::NotFound(format!(
                "Target node {} not found",
                target.value()
            )));
        }

        let edge_id = EdgeId::new(self.next_edge_id);
        self.next_edge_id += 1;

        let edge = Edge::new(edge_id, source, target, relationship_type);
        self.edges.insert(edge_id, edge);

        Ok(edge_id)
    }

    /// Get an edge by ID
    pub fn get_edge(&self, edge_id: EdgeId) -> Result<Option<&Edge>> {
        Ok(self.edges.get(&edge_id))
    }

    /// Get a mutable reference to an edge by ID
    pub fn get_edge_mut(&mut self, edge_id: EdgeId) -> Result<Option<&mut Edge>> {
        Ok(self.edges.get_mut(&edge_id))
    }

    /// Update an edge in the graph
    pub fn update_edge(&mut self, edge: Edge) -> Result<()> {
        self.edges.insert(edge.id, edge);
        Ok(())
    }

    /// Delete an edge from the graph
    pub fn delete_edge(&mut self, edge_id: EdgeId) -> Result<bool> {
        Ok(self.edges.remove(&edge_id).is_some())
    }

    /// Get all nodes in the graph
    pub fn get_all_nodes(&self) -> Result<Vec<&Node>> {
        Ok(self.nodes.values().collect())
    }

    /// Get all edges in the graph
    pub fn get_all_edges(&self) -> Result<Vec<&Edge>> {
        Ok(self.edges.values().collect())
    }

    /// Get nodes with a specific label
    pub fn get_nodes_by_label(&self, label: &str) -> Result<Vec<&Node>> {
        Ok(self
            .nodes
            .values()
            .filter(|node| node.has_label(label))
            .collect())
    }

    /// Get edges of a specific type
    pub fn get_edges_by_type(&self, relationship_type: &str) -> Result<Vec<&Edge>> {
        Ok(self
            .edges
            .values()
            .filter(|edge| edge.relationship_type == relationship_type)
            .collect())
    }

    /// Get edges connected to a specific node
    pub fn get_edges_for_node(&self, node_id: NodeId) -> Result<Vec<&Edge>> {
        Ok(self
            .edges
            .values()
            .filter(|edge| edge.source == node_id || edge.target == node_id)
            .collect())
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> Result<usize> {
        Ok(self.nodes.len())
    }

    /// Get the number of edges in the graph
    pub fn edge_count(&self) -> Result<usize> {
        Ok(self.edges.len())
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.next_node_id = 0;
        self.next_edge_id = 0;
    }

    /// Get graph statistics
    pub fn stats(&self) -> Result<GraphStats> {
        Ok(GraphStats {
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            cached_nodes: self.nodes.len(),
            cached_edges: self.edges.len(),
        })
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("node_count", &self.nodes.len())
            .field("edge_count", &self.edges.len())
            .finish()
    }
}

/// Statistics about the graph
#[derive(Debug, Clone)]
pub struct GraphStats {
    /// Number of active nodes in the graph
    pub total_nodes: usize,
    /// Number of active edges in the graph
    pub total_edges: usize,
    /// Number of nodes in cache
    pub cached_nodes: usize,
    /// Number of edges in cache
    pub cached_edges: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut graph = Graph::new();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        assert_eq!(node_id.value(), 0);

        let node = graph.get_node(node_id).unwrap().unwrap();
        assert_eq!(node.id, node_id);
        assert!(node.has_label("Person"));
        assert_eq!(node.labels.len(), 1);
    }

    #[test]
    fn test_node_with_multiple_labels() {
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node_mut(node_id).unwrap().unwrap().clone();

        node.set_property("name".to_string(), PropertyValue::String("John".to_string()));
        node.set_property("age".to_string(), PropertyValue::Int64(30));

        graph.update_node(node).unwrap();

        let updated_node = graph.get_node(node_id).unwrap().unwrap();
        assert!(updated_node.has_property("name"));
        assert!(updated_node.has_property("age"));
        assert_eq!(updated_node.property_keys().len(), 2);
    }

    #[test]
    fn test_edge_creation() {
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let mut edge = graph.get_edge_mut(edge_id).unwrap().unwrap().clone();
        edge.set_property("since".to_string(), PropertyValue::Int64(2020));

        graph.update_edge(edge).unwrap();

        let updated_edge = graph.get_edge(edge_id).unwrap().unwrap();
        assert!(updated_edge.has_property("since"));
        assert_eq!(updated_edge.property_keys().len(), 1);
    }

    #[test]
    fn test_node_deletion() {
        let mut graph = Graph::new();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        assert!(graph.get_node(node_id).unwrap().is_some());

        let deleted = graph.delete_node(node_id).unwrap();
        assert!(deleted);
        assert!(graph.get_node(node_id).unwrap().is_none());
    }

    #[test]
    fn test_edge_deletion() {
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node_mut(node_id).unwrap().unwrap().clone();

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

        graph.update_node(node).unwrap();
    }

    #[test]
    fn test_edge_other_end() {
        let mut graph = Graph::new();

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
        let mut graph = Graph::new();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node_mut(node_id).unwrap().unwrap().clone();

        // Set properties
        node.set_property("name".to_string(), PropertyValue::String("John".to_string()));
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

        graph.update_node(node).unwrap();
    }

    #[test]
    fn test_edge_property_operations() {
        let mut graph = Graph::new();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let mut edge = graph.get_edge_mut(edge_id).unwrap().unwrap().clone();

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

        graph.update_edge(edge).unwrap();
    }

    #[test]
    fn test_node_is_empty() {
        let mut graph = Graph::new();

        let node_id = graph.create_node(vec![]).unwrap();
        let node = graph.get_node(node_id).unwrap().unwrap();
        assert!(node.is_empty());

        let mut node_with_label = graph.get_node_mut(node_id).unwrap().unwrap().clone();
        node_with_label.add_label("Person".to_string());
        assert!(!node_with_label.is_empty());

        graph.update_node(node_with_label).unwrap();
    }

    #[test]
    fn test_edge_is_empty() {
        let mut graph = Graph::new();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let edge = graph.get_edge(edge_id).unwrap().unwrap();
        assert!(edge.is_empty());

        let mut edge_with_props = graph.get_edge_mut(edge_id).unwrap().unwrap().clone();
        edge_with_props.set_property("since".to_string(), PropertyValue::Int64(2020));
        assert!(!edge_with_props.is_empty());

        graph.update_edge(edge_with_props).unwrap();
    }

    #[test]
    fn test_clear_graph() {
        let mut graph = Graph::new();

        let node1_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let node2_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _edge_id = graph
            .create_edge(node1_id, node2_id, "KNOWS".to_string())
            .unwrap();

        // Verify graph has data
        assert!(!graph.nodes.is_empty());
        assert!(!graph.edges.is_empty());

        // Clear graph
        graph.clear();

        // Verify graph is empty
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
        assert_eq!(graph.next_node_id, 0);
        assert_eq!(graph.next_edge_id, 0);
    }

    #[test]
    fn test_edge_creation_with_nonexistent_nodes() {
        let mut graph = Graph::new();

        // Try to create edge with non-existent nodes
        let result = graph.create_edge(
            NodeId::new(999),
            NodeId::new(1000),
            "KNOWS".to_string(),
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound(_)));
    }

    #[test]
    fn test_node_deletion_removes_connected_edges() {
        let mut graph = Graph::new();

        let node1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let node2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let node3 = graph.create_node(vec!["Person".to_string()]).unwrap();

        let edge1 = graph
            .create_edge(node1, node2, "KNOWS".to_string())
            .unwrap();
        let edge2 = graph
            .create_edge(node1, node3, "KNOWS".to_string())
            .unwrap();

        // Verify edges exist
        assert!(graph.get_edge(edge1).unwrap().is_some());
        assert!(graph.get_edge(edge2).unwrap().is_some());

        // Delete node1
        graph.delete_node(node1).unwrap();

        // Verify edges are removed
        assert!(graph.get_edge(edge1).unwrap().is_none());
        assert!(graph.get_edge(edge2).unwrap().is_none());
    }
}