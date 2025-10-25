//! Graph comparison and diff functionality
//!
//! This module provides utilities for comparing graphs and generating diffs
//! between different graph states or versions.

use crate::graph::{Edge, EdgeId, Graph, Node, NodeId};
use crate::graph_simple::PropertyValue;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Type alias for node comparison result
type NodeComparisonResult = (Vec<Node>, Vec<Node>, Vec<NodeModification>);

/// Type alias for edge comparison result
type EdgeComparisonResult = (Vec<Edge>, Vec<Edge>, Vec<EdgeModification>);

/// Represents a difference between two graphs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDiff {
    /// Nodes that were added
    pub added_nodes: Vec<Node>,
    /// Nodes that were removed
    pub removed_nodes: Vec<Node>,
    /// Nodes that were modified
    pub modified_nodes: Vec<NodeModification>,
    /// Edges that were added
    pub added_edges: Vec<Edge>,
    /// Edges that were removed
    pub removed_edges: Vec<Edge>,
    /// Edges that were modified
    pub modified_edges: Vec<EdgeModification>,
    /// Summary statistics
    pub summary: DiffSummary,
}

/// Represents a modification to a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeModification {
    /// The node ID that was modified
    pub node_id: NodeId,
    /// The original node
    pub original: Node,
    /// The modified node
    pub modified: Node,
    /// What changed in the node
    pub changes: NodeChanges,
}

/// Represents changes to a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeChanges {
    /// Labels that were added
    pub added_labels: Vec<String>,
    /// Labels that were removed
    pub removed_labels: Vec<String>,
    /// Properties that were added
    pub added_properties: HashMap<String, PropertyValue>,
    /// Properties that were removed
    pub removed_properties: HashMap<String, PropertyValue>,
    /// Properties that were modified
    pub modified_properties: HashMap<String, PropertyValueChange>,
}

/// Represents a modification to an edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeModification {
    /// The edge ID that was modified
    pub edge_id: EdgeId,
    /// The original edge
    pub original: Edge,
    /// The modified edge
    pub modified: Edge,
    /// What changed in the edge
    pub changes: EdgeChanges,
}

/// Represents changes to an edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeChanges {
    /// Properties that were added
    pub added_properties: HashMap<String, PropertyValue>,
    /// Properties that were removed
    pub removed_properties: HashMap<String, PropertyValue>,
    /// Properties that were modified
    pub modified_properties: HashMap<String, PropertyValueChange>,
    /// Whether the relationship type changed
    pub relationship_type_changed: bool,
    /// Whether the source or target node changed
    pub endpoints_changed: bool,
}

/// Represents a change to a property value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyValueChange {
    /// The original value
    pub original: PropertyValue,
    /// The new value
    pub new: PropertyValue,
}

/// Summary statistics for a graph diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Total number of nodes in the first graph
    pub nodes_count_original: usize,
    /// Total number of nodes in the second graph
    pub nodes_count_modified: usize,
    /// Total number of edges in the first graph
    pub edges_count_original: usize,
    /// Total number of edges in the second graph
    pub edges_count_modified: usize,
    /// Number of nodes added
    pub nodes_added: usize,
    /// Number of nodes removed
    pub nodes_removed: usize,
    /// Number of nodes modified
    pub nodes_modified: usize,
    /// Number of edges added
    pub edges_added: usize,
    /// Number of edges removed
    pub edges_removed: usize,
    /// Number of edges modified
    pub edges_modified: usize,
}

/// Options for graph comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonOptions {
    /// Whether to include property changes in the diff
    pub include_property_changes: bool,
    /// Whether to include label changes in the diff
    pub include_label_changes: bool,
    /// Whether to include structural changes (edge endpoints)
    pub include_structural_changes: bool,
    /// Whether to ignore property order differences
    pub ignore_property_order: bool,
    /// Whether to treat missing properties as null values
    pub treat_missing_as_null: bool,
}

impl Default for ComparisonOptions {
    fn default() -> Self {
        Self {
            include_property_changes: true,
            include_label_changes: true,
            include_structural_changes: true,
            ignore_property_order: true,
            treat_missing_as_null: false,
        }
    }
}

/// Graph comparison utilities
pub struct GraphComparator;

impl GraphComparator {
    /// Compare two graphs and generate a diff
    pub fn compare_graphs(
        original: &Graph,
        modified: &Graph,
        options: &ComparisonOptions,
    ) -> Result<GraphDiff, String> {
        // Get all nodes and edges from both graphs
        let original_nodes = Self::get_all_nodes(original)?;
        let modified_nodes = Self::get_all_nodes(modified)?;
        let original_edges = Self::get_all_edges(original)?;
        let modified_edges = Self::get_all_edges(modified)?;

        // Find differences
        let (added_nodes, removed_nodes, modified_nodes) =
            Self::compare_nodes(&original_nodes, &modified_nodes, options)?;

        let (added_edges, removed_edges, modified_edges) =
            Self::compare_edges(&original_edges, &modified_edges, options)?;

        // Create summary
        let summary = DiffSummary {
            nodes_count_original: original_nodes.len(),
            nodes_count_modified: modified_nodes.len(),
            edges_count_original: original_edges.len(),
            edges_count_modified: modified_edges.len(),
            nodes_added: added_nodes.len(),
            nodes_removed: removed_nodes.len(),
            nodes_modified: modified_nodes.len(),
            edges_added: added_edges.len(),
            edges_removed: removed_edges.len(),
            edges_modified: modified_edges.len(),
        };

        Ok(GraphDiff {
            added_nodes,
            removed_nodes,
            modified_nodes,
            added_edges,
            removed_edges,
            modified_edges,
            summary,
        })
    }

    /// Get all nodes from a graph
    fn get_all_nodes(graph: &Graph) -> Result<HashMap<NodeId, Node>, String> {
        let nodes = graph
            .get_all_nodes()
            .map_err(|e| format!("Failed to get nodes: {}", e))?;

        let mut node_map = HashMap::new();
        for node in nodes {
            node_map.insert(node.id, node);
        }

        Ok(node_map)
    }

    /// Get all edges from a graph
    fn get_all_edges(graph: &Graph) -> Result<HashMap<EdgeId, Edge>, String> {
        let edges = graph
            .get_all_edges()
            .map_err(|e| format!("Failed to get edges: {}", e))?;

        let mut edge_map = HashMap::new();
        for edge in edges {
            edge_map.insert(edge.id, edge);
        }

        Ok(edge_map)
    }

    /// Compare nodes between two graphs
    fn compare_nodes(
        original: &HashMap<NodeId, Node>,
        modified: &HashMap<NodeId, Node>,
        options: &ComparisonOptions,
    ) -> Result<NodeComparisonResult, String> {
        let mut added_nodes = Vec::new();
        let mut removed_nodes = Vec::new();
        let mut modified_nodes = Vec::new();

        // Find added and modified nodes
        for (node_id, modified_node) in modified {
            match original.get(node_id) {
                Some(original_node) => {
                    // Node exists in both, check for modifications
                    if let Some(changes) =
                        Self::compare_node_changes(original_node, modified_node, options)
                    {
                        modified_nodes.push(NodeModification {
                            node_id: *node_id,
                            original: original_node.clone(),
                            modified: modified_node.clone(),
                            changes,
                        });
                    }
                }
                None => {
                    // Node was added
                    added_nodes.push(modified_node.clone());
                }
            }
        }

        // Find removed nodes
        for (node_id, original_node) in original {
            if !modified.contains_key(node_id) {
                removed_nodes.push(original_node.clone());
            }
        }

        Ok((added_nodes, removed_nodes, modified_nodes))
    }

    /// Compare edges between two graphs
    fn compare_edges(
        original: &HashMap<EdgeId, Edge>,
        modified: &HashMap<EdgeId, Edge>,
        options: &ComparisonOptions,
    ) -> Result<EdgeComparisonResult, String> {
        let mut added_edges = Vec::new();
        let mut removed_edges = Vec::new();
        let mut modified_edges = Vec::new();

        // Find added and modified edges
        for (edge_id, modified_edge) in modified {
            match original.get(edge_id) {
                Some(original_edge) => {
                    // Edge exists in both, check for modifications
                    if let Some(changes) =
                        Self::compare_edge_changes(original_edge, modified_edge, options)
                    {
                        modified_edges.push(EdgeModification {
                            edge_id: *edge_id,
                            original: original_edge.clone(),
                            modified: modified_edge.clone(),
                            changes,
                        });
                    }
                }
                None => {
                    // Edge was added
                    added_edges.push(modified_edge.clone());
                }
            }
        }

        // Find removed edges
        for (edge_id, original_edge) in original {
            if !modified.contains_key(edge_id) {
                removed_edges.push(original_edge.clone());
            }
        }

        Ok((added_edges, removed_edges, modified_edges))
    }

    /// Compare changes between two nodes
    pub fn compare_node_changes(
        original: &Node,
        modified: &Node,
        options: &ComparisonOptions,
    ) -> Option<NodeChanges> {
        let mut changes = NodeChanges {
            added_labels: Vec::new(),
            removed_labels: Vec::new(),
            added_properties: HashMap::new(),
            removed_properties: HashMap::new(),
            modified_properties: HashMap::new(),
        };

        let mut has_changes = false;

        // Compare labels if enabled
        if options.include_label_changes {
            let original_labels: HashSet<String> = original.labels.iter().cloned().collect();
            let modified_labels: HashSet<String> = modified.labels.iter().cloned().collect();

            for label in &modified_labels {
                if !original_labels.contains(label) {
                    changes.added_labels.push(label.clone());
                    has_changes = true;
                }
            }

            for label in &original_labels {
                if !modified_labels.contains(label) {
                    changes.removed_labels.push(label.clone());
                    has_changes = true;
                }
            }
        }

        // Compare properties if enabled
        if options.include_property_changes {
            for (key, modified_value) in &modified.properties {
                match original.properties.get(key) {
                    Some(original_value) => {
                        if !Self::values_equal(original_value, modified_value, options) {
                            changes.modified_properties.insert(
                                key.clone(),
                                PropertyValueChange {
                                    original: original_value.clone(),
                                    new: modified_value.clone(),
                                },
                            );
                            has_changes = true;
                        }
                    }
                    None => {
                        changes
                            .added_properties
                            .insert(key.clone(), modified_value.clone());
                        has_changes = true;
                    }
                }
            }

            for (key, original_value) in &original.properties {
                if !modified.properties.contains_key(key) {
                    changes
                        .removed_properties
                        .insert(key.clone(), original_value.clone());
                    has_changes = true;
                }
            }
        }

        if has_changes { Some(changes) } else { None }
    }

    /// Compare changes between two edges
    pub fn compare_edge_changes(
        original: &Edge,
        modified: &Edge,
        options: &ComparisonOptions,
    ) -> Option<EdgeChanges> {
        let mut changes = EdgeChanges {
            added_properties: HashMap::new(),
            removed_properties: HashMap::new(),
            modified_properties: HashMap::new(),
            relationship_type_changed: false,
            endpoints_changed: false,
        };

        let mut has_changes = false;

        // Check structural changes
        if options.include_structural_changes {
            if original.relationship_type != modified.relationship_type {
                changes.relationship_type_changed = true;
                has_changes = true;
            }

            if original.source != modified.source || original.target != modified.target {
                changes.endpoints_changed = true;
                has_changes = true;
            }
        }

        // Compare properties if enabled
        if options.include_property_changes {
            for (key, modified_value) in &modified.properties {
                match original.properties.get(key) {
                    Some(original_value) => {
                        if !Self::values_equal(original_value, modified_value, options) {
                            changes.modified_properties.insert(
                                key.clone(),
                                PropertyValueChange {
                                    original: original_value.clone(),
                                    new: modified_value.clone(),
                                },
                            );
                            has_changes = true;
                        }
                    }
                    None => {
                        changes
                            .added_properties
                            .insert(key.clone(), modified_value.clone());
                        has_changes = true;
                    }
                }
            }

            for (key, original_value) in &original.properties {
                if !modified.properties.contains_key(key) {
                    changes
                        .removed_properties
                        .insert(key.clone(), original_value.clone());
                    has_changes = true;
                }
            }
        }

        if has_changes { Some(changes) } else { None }
    }

    /// Compare two property values for equality
    pub fn values_equal(a: &PropertyValue, b: &PropertyValue, _options: &ComparisonOptions) -> bool {
        match (a, b) {
            (PropertyValue::Null, PropertyValue::Null) => true,
            (PropertyValue::Bool(a_val), PropertyValue::Bool(b_val)) => a_val == b_val,
            (PropertyValue::Int64(a_val), PropertyValue::Int64(b_val)) => a_val == b_val,
            (PropertyValue::Float64(a_val), PropertyValue::Float64(b_val)) => {
                (a_val - b_val).abs() < f64::EPSILON
            }
            (PropertyValue::String(a_val), PropertyValue::String(b_val)) => a_val == b_val,
            (PropertyValue::Bytes(a_val), PropertyValue::Bytes(b_val)) => a_val == b_val,
            _ => false,
        }
    }

    /// Calculate similarity between two graphs
    pub fn calculate_similarity(
        graph1: &Graph,
        graph2: &Graph,
        options: &ComparisonOptions,
    ) -> Result<f64, String> {
        let diff = Self::compare_graphs(graph1, graph2, options)?;

        let total_nodes = diff
            .summary
            .nodes_count_original
            .max(diff.summary.nodes_count_modified);
        let total_edges = diff
            .summary
            .edges_count_original
            .max(diff.summary.edges_count_modified);

        if total_nodes == 0 && total_edges == 0 {
            return Ok(1.0); // Both graphs are empty
        }

        let node_similarity = if total_nodes > 0 {
            1.0 - (diff.summary.nodes_added
                + diff.summary.nodes_removed
                + diff.summary.nodes_modified) as f64
                / total_nodes as f64
        } else {
            1.0
        };

        let edge_similarity = if total_edges > 0 {
            1.0 - (diff.summary.edges_added
                + diff.summary.edges_removed
                + diff.summary.edges_modified) as f64
                / total_edges as f64
        } else {
            1.0
        };

        // Weighted average (nodes and edges equally weighted)
        Ok((node_similarity + edge_similarity) / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_simple::PropertyValue;

    #[test]
    fn test_property_value_equality() {
        let options = ComparisonOptions::default();

        // Test null values
        assert!(GraphComparator::values_equal(
            &PropertyValue::Null,
            &PropertyValue::Null,
            &options
        ));

        // Test boolean values
        assert!(GraphComparator::values_equal(
            &PropertyValue::Bool(true),
            &PropertyValue::Bool(true),
            &options
        ));
        assert!(!GraphComparator::values_equal(
            &PropertyValue::Bool(true),
            &PropertyValue::Bool(false),
            &options
        ));

        // Test integer values
        assert!(GraphComparator::values_equal(
            &PropertyValue::Int64(42),
            &PropertyValue::Int64(42),
            &options
        ));
        assert!(!GraphComparator::values_equal(
            &PropertyValue::Int64(42),
            &PropertyValue::Int64(43),
            &options
        ));

        // Test string values
        assert!(GraphComparator::values_equal(
            &PropertyValue::String("hello".to_string()),
            &PropertyValue::String("hello".to_string()),
            &options
        ));
        assert!(!GraphComparator::values_equal(
            &PropertyValue::String("hello".to_string()),
            &PropertyValue::String("world".to_string()),
            &options
        ));
    }

    #[test]
    fn test_bytes_equality() {
        let options = ComparisonOptions::default();

        let bytes1 = PropertyValue::Bytes(vec![1, 2, 3]);
        let bytes2 = PropertyValue::Bytes(vec![1, 2, 3]);
        let bytes3 = PropertyValue::Bytes(vec![1, 2, 4]);

        assert!(GraphComparator::values_equal(&bytes1, &bytes2, &options));
        assert!(!GraphComparator::values_equal(&bytes1, &bytes3, &options));
    }

    #[test]
    fn test_bytes_inequality() {
        let options = ComparisonOptions::default();

        let bytes1 = PropertyValue::Bytes(vec![1, 2, 3]);
        let bytes2 = PropertyValue::Bytes(vec![3, 2, 1]);

        assert!(!GraphComparator::values_equal(&bytes1, &bytes2, &options));
    }
}
