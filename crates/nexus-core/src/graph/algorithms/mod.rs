//! Graph algorithms module
//!
//! This module provides essential graph algorithms including:
//! - Breadth-First Search (BFS)
//! - Depth-First Search (DFS)
//! - Shortest Path algorithms (Dijkstra, A*)
//! - Connected Components
//! - Topological Sort
//! - Minimum Spanning Tree (MST)

use crate::{Error, Result};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

/// Graph representation for algorithms
#[derive(Debug, Clone)]
pub struct Graph {
    /// Adjacency list representation
    /// Key: node_id, Value: list of (neighbor_id, edge_weight)
    adjacency: HashMap<u64, Vec<(u64, f64)>>,
    /// Node labels
    node_labels: HashMap<u64, Vec<String>>,
    /// Edge types
    edge_types: HashMap<(u64, u64), Vec<String>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            adjacency: HashMap::new(),
            node_labels: HashMap::new(),
            edge_types: HashMap::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node_id: u64, labels: Vec<String>) {
        self.adjacency.entry(node_id).or_default();
        self.node_labels.insert(node_id, labels);
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, from: u64, to: u64, weight: f64, edge_types: Vec<String>) {
        self.adjacency.entry(from).or_default().push((to, weight));
        self.edge_types.insert((from, to), edge_types);
    }

    /// Get neighbors of a node
    pub fn get_neighbors(&self, node_id: u64) -> &[(u64, f64)] {
        self.adjacency
            .get(&node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get all nodes in the graph
    pub fn get_nodes(&self) -> Vec<u64> {
        self.adjacency.keys().cloned().collect()
    }

    /// Check if a node exists
    pub fn has_node(&self, node_id: u64) -> bool {
        self.adjacency.contains_key(&node_id)
    }

    /// Get node labels
    pub fn get_node_labels(&self, node_id: u64) -> Option<&Vec<String>> {
        self.node_labels.get(&node_id)
    }

    /// Get edge types
    pub fn get_edge_types(&self, from: u64, to: u64) -> Option<&Vec<String>> {
        self.edge_types.get(&(from, to))
    }

    /// Build graph from Engine storage
    /// This allows using graph algorithms directly on database data
    pub fn from_engine(engine: &crate::Engine, weight_property: Option<&str>) -> Result<Self> {
        let mut graph = Self::new();
        let tx = engine.transaction_manager.write().begin_read()?;

        // Add all nodes
        for node_id in 0..engine.storage.node_count() {
            if let Ok(Some(node_record)) = engine.storage.get_node(&tx, node_id) {
                if !node_record.is_deleted() {
                    // Get labels
                    let labels = engine
                        .catalog
                        .get_labels_from_bitmap(node_record.label_bits)?;

                    graph.add_node(node_id, labels);
                }
            }
        }

        // Add all relationships as edges
        for rel_id in 0..engine.storage.relationship_count() {
            if let Ok(Some(rel_record)) = engine.storage.get_relationship(&tx, rel_id) {
                if !rel_record.is_deleted() {
                    // Get relationship type
                    let rel_type = engine
                        .catalog
                        .get_type_name(rel_record.type_id)
                        .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                        .unwrap_or_else(|| "UNKNOWN".to_string());

                    // Extract weight from properties if specified
                    let mut weight = 1.0;
                    if let Some(weight_prop) = weight_property {
                        if rel_record.prop_ptr != 0 {
                            if let Ok(Some(properties)) =
                                engine.storage.load_relationship_properties(rel_id)
                            {
                                if let Some(prop_value) = properties.get(weight_prop) {
                                    if let Some(num) = prop_value.as_f64() {
                                        weight = num;
                                    } else if let Some(num) = prop_value.as_u64() {
                                        weight = num as f64;
                                    } else if let Some(num) = prop_value.as_i64() {
                                        weight = num as f64;
                                    }
                                }
                            }
                        }
                    }

                    graph.add_edge(rel_record.src_id, rel_record.dst_id, weight, vec![rel_type]);
                }
            }
        }

        Ok(graph)
    }
}

/// BFS result containing distances and paths
#[derive(Debug, Clone)]
pub struct BfsResult {
    /// Distance from source to each node
    pub distances: HashMap<u64, usize>,
    /// Parent node for each node in the BFS tree
    pub parents: HashMap<u64, u64>,
    /// Order of node discovery
    pub discovery_order: Vec<u64>,
}

/// DFS result containing discovery and finish times
#[derive(Debug, Clone)]
pub struct DfsResult {
    /// Discovery time for each node
    pub discovery_times: HashMap<u64, usize>,
    /// Finish time for each node
    pub finish_times: HashMap<u64, usize>,
    /// Parent node for each node in the DFS tree
    pub parents: HashMap<u64, u64>,
    /// Order of node discovery
    pub discovery_order: Vec<u64>,
}

/// Shortest path result
#[derive(Debug, Clone)]
pub struct ShortestPathResult {
    /// Distance from source to each node
    pub distances: HashMap<u64, f64>,
    /// Parent node for each node in the shortest path tree
    pub parents: HashMap<u64, u64>,
    /// Path from source to target (if target specified)
    pub path: Option<Vec<u64>>,
}

/// K shortest path result (for Yen's algorithm)
#[derive(Debug, Clone, PartialEq)]
pub struct KShortestPath {
    /// The path from source to target
    pub path: Vec<u64>,
    /// Total length of the path
    pub length: f64,
}

/// Connected components result
#[derive(Debug, Clone)]
pub struct ConnectedComponentsResult {
    /// Component ID for each node
    pub components: HashMap<u64, usize>,
    /// Number of connected components
    pub component_count: usize,
    /// Nodes in each component
    pub component_nodes: HashMap<usize, Vec<u64>>,
}

/// Topological sort result
#[derive(Debug, Clone)]
pub struct TopologicalSortResult {
    /// Topologically sorted nodes
    pub sorted_nodes: Vec<u64>,
    /// Whether the graph has cycles
    pub has_cycle: bool,
}

/// MST result
#[derive(Debug, Clone)]
pub struct MstResult {
    /// Edges in the MST
    pub edges: Vec<(u64, u64, f64)>,
    /// Total weight of the MST
    pub total_weight: f64,
}

pub mod traversal;

#[cfg(test)]
mod tests;
