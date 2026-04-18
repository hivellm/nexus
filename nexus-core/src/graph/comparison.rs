//! Graph comparison and diff functionality
//!
//! This module provides utilities for comparing graphs and generating diffs
//! between different graph states or versions.

use crate::graph::simple::PropertyValue;
use crate::graph::{Edge, EdgeId, Graph, Node, NodeId};
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
    /// Overall similarity score (0.0 to 1.0)
    pub overall_similarity: f64,
    /// Structural similarity score (0.0 to 1.0)
    pub structural_similarity: f64,
    /// Content similarity score (0.0 to 1.0)
    pub content_similarity: f64,
    /// Topology analysis results (if enabled)
    pub topology_analysis: Option<TopologyAnalysis>,
    /// Graph metrics comparison (if enabled)
    pub metrics_comparison: Option<MetricsComparison>,
}

/// Topology analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyAnalysis {
    /// Number of connected components in original graph
    pub original_components: usize,
    /// Number of connected components in modified graph
    pub modified_components: usize,
    /// Changes in component structure
    pub component_changes: Vec<ComponentChange>,
    /// Changes in graph diameter
    pub diameter_change: Option<f64>,
    /// Changes in average path length
    pub avg_path_length_change: Option<f64>,
    /// Changes in clustering coefficient
    pub clustering_coefficient_change: Option<f64>,
}

/// Component change information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentChange {
    /// Type of change (added, removed, merged, split)
    pub change_type: String,
    /// Size of the component
    pub size: usize,
    /// Nodes involved in the change
    pub nodes: Vec<NodeId>,
}

/// Graph metrics comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsComparison {
    /// Original graph metrics
    pub original_metrics: GraphMetrics,
    /// Modified graph metrics
    pub modified_metrics: GraphMetrics,
    /// Percentage change for each metric
    pub percentage_changes: HashMap<String, f64>,
}

/// Graph metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetrics {
    /// Number of nodes
    pub node_count: usize,
    /// Number of edges
    pub edge_count: usize,
    /// Graph density
    pub density: f64,
    /// Average degree
    pub avg_degree: f64,
    /// Maximum degree
    pub max_degree: usize,
    /// Minimum degree
    pub min_degree: usize,
    /// Number of triangles
    pub triangle_count: usize,
    /// Clustering coefficient
    pub clustering_coefficient: f64,
    /// Assortativity coefficient
    pub assortativity: f64,
    /// Graph diameter
    pub diameter: usize,
    /// Average shortest path length
    pub avg_shortest_path: f64,
}

/// Parameters for calculating overall similarity
#[derive(Debug)]
pub struct SimilarityParams<'a> {
    /// Original nodes
    pub original_nodes: &'a HashMap<NodeId, Node>,
    /// Modified nodes
    pub modified_nodes: &'a HashMap<NodeId, Node>,
    /// Original edges
    pub original_edges: &'a HashMap<EdgeId, Edge>,
    /// Modified edges
    pub modified_edges: &'a HashMap<EdgeId, Edge>,
    /// Added nodes
    pub added_nodes: &'a [Node],
    /// Removed nodes
    pub removed_nodes: &'a [Node],
    /// Modified nodes
    pub modified_nodes_list: &'a [NodeModification],
    /// Added edges
    pub added_edges: &'a [Edge],
    /// Removed edges
    pub removed_edges: &'a [Edge],
    /// Modified edges
    pub modified_edges_list: &'a [EdgeModification],
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
    /// Whether to use fuzzy matching for node/edge identification
    pub use_fuzzy_matching: bool,
    /// Similarity threshold for fuzzy matching (0.0 to 1.0)
    pub fuzzy_threshold: f64,
    /// Whether to include graph topology analysis
    pub include_topology_analysis: bool,
    /// Whether to calculate graph metrics during comparison
    pub calculate_metrics: bool,
    /// Maximum depth for subgraph comparison
    pub max_comparison_depth: Option<usize>,
    /// Whether to include temporal analysis (if timestamps available)
    pub include_temporal_analysis: bool,
}

impl Default for ComparisonOptions {
    fn default() -> Self {
        Self {
            include_property_changes: true,
            include_label_changes: true,
            include_structural_changes: true,
            ignore_property_order: true,
            treat_missing_as_null: false,
            use_fuzzy_matching: false,
            fuzzy_threshold: 0.8,
            include_topology_analysis: false,
            calculate_metrics: false,
            max_comparison_depth: None,
            include_temporal_analysis: false,
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
        let (added_nodes, removed_nodes, modified_nodes_list) =
            Self::compare_nodes(&original_nodes, &modified_nodes, options)?;

        let (added_edges, removed_edges, modified_edges_list) =
            Self::compare_edges(&original_edges, &modified_edges, options)?;

        // Calculate similarity scores
        let similarity_params = SimilarityParams {
            original_nodes: &original_nodes,
            modified_nodes: &modified_nodes,
            original_edges: &original_edges,
            modified_edges: &modified_edges,
            added_nodes: &added_nodes,
            removed_nodes: &removed_nodes,
            modified_nodes_list: &modified_nodes_list,
            added_edges: &added_edges,
            removed_edges: &removed_edges,
            modified_edges_list: &modified_edges_list,
        };
        let overall_similarity = Self::calculate_overall_similarity(&similarity_params);

        let structural_similarity = Self::calculate_structural_similarity(
            &original_edges,
            &modified_edges,
            &added_edges,
            &removed_edges,
            &modified_edges_list,
        );

        let content_similarity = Self::calculate_content_similarity(
            &original_nodes,
            &modified_nodes,
            &added_nodes,
            &removed_nodes,
            &modified_nodes_list,
        );

        // Perform topology analysis if enabled
        let topology_analysis = if options.include_topology_analysis {
            Some(Self::analyze_topology(
                original,
                modified,
                &original_nodes,
                &modified_nodes,
            )?)
        } else {
            None
        };

        // Calculate metrics comparison if enabled
        let metrics_comparison = if options.calculate_metrics {
            Some(Self::calculate_metrics_comparison(original, modified)?)
        } else {
            None
        };

        // Create summary
        let summary = DiffSummary {
            nodes_count_original: original_nodes.len(),
            nodes_count_modified: modified_nodes.len(),
            edges_count_original: original_edges.len(),
            edges_count_modified: modified_edges.len(),
            nodes_added: added_nodes.len(),
            nodes_removed: removed_nodes.len(),
            nodes_modified: modified_nodes_list.len(),
            edges_added: added_edges.len(),
            edges_removed: removed_edges.len(),
            edges_modified: modified_edges_list.len(),
            overall_similarity,
            structural_similarity,
            content_similarity,
            topology_analysis,
            metrics_comparison,
        };

        Ok(GraphDiff {
            added_nodes,
            removed_nodes,
            modified_nodes: modified_nodes_list,
            added_edges,
            removed_edges,
            modified_edges: modified_edges_list,
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
    pub fn values_equal(
        a: &PropertyValue,
        b: &PropertyValue,
        _options: &ComparisonOptions,
    ) -> bool {
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
        Ok(diff.summary.overall_similarity)
    }

    /// Calculate overall similarity score
    fn calculate_overall_similarity(params: &SimilarityParams<'_>) -> f64 {
        let total_nodes = params.original_nodes.len().max(params.modified_nodes.len());
        let total_edges = params.original_edges.len().max(params.modified_edges.len());

        if total_nodes == 0 && total_edges == 0 {
            return 1.0; // Both graphs are empty
        }

        let node_similarity = if total_nodes > 0 {
            1.0 - (params.added_nodes.len()
                + params.removed_nodes.len()
                + params.modified_nodes_list.len()) as f64
                / total_nodes as f64
        } else {
            1.0
        };

        let edge_similarity = if total_edges > 0 {
            1.0 - (params.added_edges.len()
                + params.removed_edges.len()
                + params.modified_edges_list.len()) as f64
                / total_edges as f64
        } else {
            1.0
        };

        // Weighted average (nodes and edges equally weighted)
        (node_similarity + edge_similarity) / 2.0
    }

    /// Calculate structural similarity score
    fn calculate_structural_similarity(
        original_edges: &HashMap<EdgeId, Edge>,
        modified_edges: &HashMap<EdgeId, Edge>,
        added_edges: &[Edge],
        removed_edges: &[Edge],
        modified_edges_list: &[EdgeModification],
    ) -> f64 {
        let total_edges = original_edges.len().max(modified_edges.len());

        if total_edges == 0 {
            return 1.0;
        }

        1.0 - (added_edges.len() + removed_edges.len() + modified_edges_list.len()) as f64
            / total_edges as f64
    }

    /// Calculate content similarity score
    fn calculate_content_similarity(
        original_nodes: &HashMap<NodeId, Node>,
        modified_nodes: &HashMap<NodeId, Node>,
        added_nodes: &[Node],
        removed_nodes: &[Node],
        modified_nodes_list: &[NodeModification],
    ) -> f64 {
        let total_nodes = original_nodes.len().max(modified_nodes.len());

        if total_nodes == 0 {
            return 1.0;
        }

        1.0 - (added_nodes.len() + removed_nodes.len() + modified_nodes_list.len()) as f64
            / total_nodes as f64
    }

    /// Analyze graph topology changes
    fn analyze_topology(
        _original: &Graph,
        _modified: &Graph,
        _original_nodes: &HashMap<NodeId, Node>,
        _modified_nodes: &HashMap<NodeId, Node>,
    ) -> Result<TopologyAnalysis, String> {
        // This is a simplified implementation
        // In a real implementation, you would use graph algorithms to find connected components
        // and calculate various topology metrics

        let original_components = 1; // Simplified: assume single component
        let modified_components = 1; // Simplified: assume single component

        Ok(TopologyAnalysis {
            original_components,
            modified_components,
            component_changes: Vec::new(),
            diameter_change: None,
            avg_path_length_change: None,
            clustering_coefficient_change: None,
        })
    }

    /// Calculate metrics comparison
    fn calculate_metrics_comparison(
        _original: &Graph,
        _modified: &Graph,
    ) -> Result<MetricsComparison, String> {
        // This is a simplified implementation
        // In a real implementation, you would calculate actual graph metrics

        let original_metrics = GraphMetrics {
            node_count: 0,
            edge_count: 0,
            density: 0.0,
            avg_degree: 0.0,
            max_degree: 0,
            min_degree: 0,
            triangle_count: 0,
            clustering_coefficient: 0.0,
            assortativity: 0.0,
            diameter: 0,
            avg_shortest_path: 0.0,
        };

        let modified_metrics = GraphMetrics {
            node_count: 0,
            edge_count: 0,
            density: 0.0,
            avg_degree: 0.0,
            max_degree: 0,
            min_degree: 0,
            triangle_count: 0,
            clustering_coefficient: 0.0,
            assortativity: 0.0,
            diameter: 0,
            avg_shortest_path: 0.0,
        };

        let mut percentage_changes = HashMap::new();
        percentage_changes.insert("node_count".to_string(), 0.0);
        percentage_changes.insert("edge_count".to_string(), 0.0);

        Ok(MetricsComparison {
            original_metrics,
            modified_metrics,
            percentage_changes,
        })
    }

    /// Find similar nodes using fuzzy matching
    fn find_similar_nodes(
        node: &Node,
        candidates: &HashMap<NodeId, Node>,
        threshold: f64,
    ) -> Vec<(NodeId, f64)> {
        let mut similarities = Vec::new();

        for (candidate_id, candidate_node) in candidates {
            let similarity = Self::calculate_node_similarity(node, candidate_node);
            if similarity >= threshold {
                similarities.push((*candidate_id, similarity));
            }
        }

        // Sort by similarity (highest first)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        similarities
    }

    /// Calculate similarity between two nodes
    pub fn calculate_node_similarity(node1: &Node, node2: &Node) -> f64 {
        let mut similarity = 0.0;
        let mut total_weight = 0.0;

        // Label similarity
        let label_weight = 0.4;
        let label_similarity = Self::calculate_label_similarity(&node1.labels, &node2.labels);
        similarity += label_similarity * label_weight;
        total_weight += label_weight;

        // Property similarity
        let property_weight = 0.6;
        let property_similarity =
            Self::calculate_property_similarity(&node1.properties, &node2.properties);
        similarity += property_similarity * property_weight;
        total_weight += property_weight;

        similarity / total_weight
    }

    /// Calculate label similarity
    pub fn calculate_label_similarity(labels1: &[String], labels2: &[String]) -> f64 {
        if labels1.is_empty() && labels2.is_empty() {
            return 1.0;
        }

        let set1: HashSet<&String> = labels1.iter().collect();
        let set2: HashSet<&String> = labels2.iter().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        if union == 0 {
            1.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Calculate property similarity
    pub fn calculate_property_similarity(
        prop1: &HashMap<String, PropertyValue>,
        prop2: &HashMap<String, PropertyValue>,
    ) -> f64 {
        if prop1.is_empty() && prop2.is_empty() {
            return 1.0;
        }

        let keys1: HashSet<&String> = prop1.keys().collect();
        let keys2: HashSet<&String> = prop2.keys().collect();

        let key_intersection = keys1.intersection(&keys2).count();
        let key_union = keys1.union(&keys2).count();

        if key_union == 0 {
            return 1.0;
        }

        let key_similarity = key_intersection as f64 / key_union as f64;

        // Calculate value similarity for common keys
        let mut value_similarity = 0.0;
        let mut common_key_count = 0;

        for key in keys1.intersection(&keys2) {
            if let (Some(val1), Some(val2)) = (prop1.get(*key), prop2.get(*key)) {
                if Self::values_equal(val1, val2, &ComparisonOptions::default()) {
                    value_similarity += 1.0;
                }
                common_key_count += 1;
            }
        }

        let value_sim = if common_key_count > 0 {
            value_similarity / common_key_count as f64
        } else {
            1.0
        };

        // Weighted combination of key and value similarity
        key_similarity * 0.3 + value_sim * 0.7
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::simple::PropertyValue;

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
