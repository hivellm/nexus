//! Core graph types: enums, node/edge structs, `CorrelationGraph`, and
//! associated statistics used throughout the correlation subsystem.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Graph types supported by the correlation analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphType {
    /// Function call relationships and execution flow
    Call,
    /// Module/library dependencies and imports
    Dependency,
    /// Data transformation and variable usage
    DataFlow,
    /// High-level architectural components
    Component,
}

/// Node types in the correlation graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Function or method
    Function,
    /// Module or file
    Module,
    /// Class or struct
    Class,
    /// Variable or parameter
    Variable,
    /// API endpoint or service
    API,
}

/// Edge types representing relationships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    /// Function calls another function
    Calls,
    /// Module imports another module
    Imports,
    /// Class inherits from another class
    Inherits,
    /// Component composes another component
    Composes,
    /// Data transforms from one format to another
    Transforms,
    /// Uses or references another entity
    Uses,
    /// Depends on another entity
    Depends,
    /// Recursive call (function calls itself directly or indirectly)
    RecursiveCall,
}

/// Recursive call detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecursiveCallInfo {
    /// Whether the function is involved in recursion
    pub is_recursive: bool,
    /// Direct self-calls (function calls itself)
    pub direct_recursion: bool,
    /// Indirect recursion (function calls another function that eventually calls back)
    pub indirect_recursion: bool,
    /// Recursion depth (maximum depth of recursive calls)
    pub max_depth: usize,
    /// Functions involved in the recursive cycle
    pub cycle_functions: Vec<String>,
    /// Recursion type classification
    pub recursion_type: RecursionType,
}

/// Types of recursion patterns
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecursionType {
    /// No recursion
    None,
    /// Direct recursion (function calls itself)
    Direct,
    /// Indirect recursion (A calls B, B calls A)
    Indirect,
    /// Complex recursion (A calls B, B calls C, C calls A)
    Complex,
    /// Mutual recursion (A calls B, B calls A, both are entry points)
    Mutual,
}

/// Recursive call detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecursiveCallConfig {
    /// Maximum depth to search for recursion
    pub max_search_depth: usize,
    /// Whether to detect indirect recursion
    pub detect_indirect: bool,
    /// Whether to detect mutual recursion
    pub detect_mutual: bool,
    /// Whether to include recursion metadata in nodes
    pub include_recursion_metadata: bool,
    /// Whether to mark recursive edges with special type
    pub mark_recursive_edges: bool,
}

impl Default for RecursiveCallConfig {
    fn default() -> Self {
        Self {
            max_search_depth: 10,
            detect_indirect: true,
            detect_mutual: true,
            include_recursion_metadata: true,
            mark_recursive_edges: true,
        }
    }
}

/// Graph node with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique node identifier
    pub id: String,
    /// Node type
    pub node_type: NodeType,
    /// Human-readable label
    pub label: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Position for visualization (x, y coordinates)
    pub position: Option<(f32, f32)>,
    /// Node size for visualization
    pub size: Option<f32>,
    /// Node color for visualization
    pub color: Option<String>,
}

/// Graph edge with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Unique edge identifier
    pub id: String,
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Edge type
    pub edge_type: EdgeType,
    /// Edge weight (strength of relationship)
    pub weight: f32,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Edge label for visualization
    pub label: Option<String>,
}

/// Complete correlation graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationGraph {
    /// Graph type
    pub graph_type: GraphType,
    /// Graph name/title
    pub name: String,
    /// Graph description
    pub description: Option<String>,
    /// All nodes in the graph
    pub nodes: Vec<GraphNode>,
    /// All edges in the graph
    pub edges: Vec<GraphEdge>,
    /// Graph metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl CorrelationGraph {
    /// Create a new correlation graph
    pub fn new(graph_type: GraphType, name: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            graph_type,
            name,
            description: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: GraphNode) -> Result<()> {
        // Check for duplicate IDs
        if self.nodes.iter().any(|n| n.id == node.id) {
            return Err(Error::GraphCorrelation(format!(
                "Duplicate node ID: {}",
                node.id
            )));
        }

        self.nodes.push(node);
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, edge: GraphEdge) -> Result<()> {
        // Check for duplicate IDs
        if self.edges.iter().any(|e| e.id == edge.id) {
            return Err(Error::GraphCorrelation(format!(
                "Duplicate edge ID: {}",
                edge.id
            )));
        }

        // Validate that source and target nodes exist
        if !self.nodes.iter().any(|n| n.id == edge.source) {
            return Err(Error::GraphCorrelation(format!(
                "Source node not found: {}",
                edge.source
            )));
        }

        if !self.nodes.iter().any(|n| n.id == edge.target) {
            return Err(Error::GraphCorrelation(format!(
                "Target node not found: {}",
                edge.target
            )));
        }

        self.edges.push(edge);
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Get node by ID
    pub fn get_node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get edge by ID
    pub fn get_edge(&self, id: &str) -> Option<&GraphEdge> {
        self.edges.iter().find(|e| e.id == id)
    }

    /// Get all edges connected to a node
    pub fn get_node_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|e| e.source == node_id || e.target == node_id)
            .collect()
    }

    /// Calculate graph statistics
    pub fn statistics(&self) -> GraphStatistics {
        GraphStatistics {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            avg_degree: if self.nodes.is_empty() {
                0.0
            } else {
                self.edges.len() as f32 / self.nodes.len() as f32
            },
            max_degree: self
                .nodes
                .iter()
                .map(|n| self.get_node_edges(&n.id).len())
                .max()
                .unwrap_or(0),
            graph_density: if self.nodes.len() <= 1 {
                0.0
            } else {
                let max_edges = self.nodes.len() * (self.nodes.len() - 1);
                self.edges.len() as f32 / max_edges as f32
            },
        }
    }

    /// Export graph to JSON format
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| Error::GraphCorrelation(format!("JSON serialization failed: {}", e)))
    }

    /// Export graph to GraphML format
    pub fn to_graphml(&self) -> Result<String> {
        // For MVP, we'll implement a basic GraphML export
        let mut graphml = String::new();
        graphml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        graphml.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
        graphml.push_str("  <graph id=\"G\" edgedefault=\"directed\">\n");

        // Add nodes
        for node in &self.nodes {
            graphml.push_str(&format!(
                "    <node id=\"{}\" label=\"{}\">\n",
                node.id, node.label
            ));
            graphml.push_str("    </node>\n");
        }

        // Add edges
        for edge in &self.edges {
            graphml.push_str(&format!(
                "    <edge id=\"{}\" source=\"{}\" target=\"{}\" weight=\"{}\">\n",
                edge.id, edge.source, edge.target, edge.weight
            ));
            graphml.push_str("    </edge>\n");
        }

        graphml.push_str("  </graph>\n");
        graphml.push_str("</graphml>\n");

        Ok(graphml)
    }

    /// Apply hierarchical layout to the call graph
    pub fn apply_hierarchical_layout(&mut self) -> Result<()> {
        if self.graph_type != GraphType::Call {
            return Err(Error::GraphCorrelation(
                "Hierarchical layout is only supported for call graphs".to_string(),
            ));
        }

        let layout_engine =
            super::hierarchical_layout::HierarchicalCallGraphLayout::with_default_config();
        let layouted_graph = layout_engine.layout(self.clone())?;

        // Update positions and sizes
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if let Some(layouted_node) = layouted_graph.nodes.get(i) {
                node.position = layouted_node.position;
                node.size = layouted_node.size;
            }
        }

        Ok(())
    }

    /// Apply hierarchical layout with custom configuration
    pub fn apply_hierarchical_layout_with_config(
        &mut self,
        config: super::hierarchical_layout::HierarchicalCallGraphConfig,
    ) -> Result<()> {
        if self.graph_type != GraphType::Call {
            return Err(Error::GraphCorrelation(
                "Hierarchical layout is only supported for call graphs".to_string(),
            ));
        }

        let layout_engine = super::hierarchical_layout::HierarchicalCallGraphLayout::new(config);
        let layouted_graph = layout_engine.layout(self.clone())?;

        // Update positions and sizes
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if let Some(layouted_node) = layouted_graph.nodes.get(i) {
                node.position = layouted_node.position;
                node.size = layouted_node.size;
            }
        }

        Ok(())
    }

    /// Detect recursive calls in the graph
    pub fn detect_recursive_calls(
        &self,
        config: &RecursiveCallConfig,
    ) -> Result<HashMap<String, RecursiveCallInfo>> {
        let mut recursive_info = HashMap::new();

        // Get all function nodes
        let function_nodes: Vec<&GraphNode> = self
            .nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Function)
            .collect();

        for node in &function_nodes {
            let mut visited = std::collections::HashSet::new();
            let mut path = Vec::new();
            let mut cycles = Vec::new();

            // Detect recursion starting from this node
            self.detect_recursion_from_node(
                node.id.as_str(),
                &mut visited,
                &mut path,
                &mut cycles,
                config,
            )?;

            // Analyze the detected cycles
            let recursion_info = self.analyze_recursion_cycles(&cycles, node.id.as_str());
            recursive_info.insert(node.id.clone(), recursion_info);
        }

        Ok(recursive_info)
    }

    /// Detect recursion starting from a specific node using DFS
    fn detect_recursion_from_node(
        &self,
        node_id: &str,
        visited: &mut std::collections::HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
        config: &RecursiveCallConfig,
    ) -> Result<()> {
        if path.len() > config.max_search_depth {
            return Ok(());
        }

        // Check if we've already visited this node in the current path (cycle detection)
        if path.contains(&node_id.to_string()) {
            // Found a cycle - extract the cycle
            let cycle_start = path.iter().position(|id| id == node_id).unwrap();
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(node_id.to_string()); // Complete the cycle
            cycles.push(cycle);
            return Ok(());
        }

        // Skip if already visited in this search (to avoid infinite loops)
        if visited.contains(node_id) {
            return Ok(());
        }

        // Add to current path and mark as visited
        path.push(node_id.to_string());
        visited.insert(node_id.to_string());

        // Find all outgoing call edges from this node
        for edge in &self.edges {
            if edge.source == node_id && edge.edge_type == EdgeType::Calls {
                self.detect_recursion_from_node(&edge.target, visited, path, cycles, config)?;
            }
        }

        // Remove from current path when backtracking
        path.pop();
        visited.remove(node_id);
        Ok(())
    }

    /// Analyze detected cycles to determine recursion type and properties
    fn analyze_recursion_cycles(&self, cycles: &[Vec<String>], node_id: &str) -> RecursiveCallInfo {
        if cycles.is_empty() {
            return RecursiveCallInfo {
                is_recursive: false,
                direct_recursion: false,
                indirect_recursion: false,
                max_depth: 0,
                cycle_functions: Vec::new(),
                recursion_type: RecursionType::None,
            };
        }

        let mut direct_recursion = false;
        let mut indirect_recursion = false;
        let mut max_depth = 0;
        let mut all_cycle_functions = std::collections::HashSet::new();

        for cycle in cycles {
            max_depth = max_depth.max(cycle.len());
            all_cycle_functions.extend(cycle.iter().cloned());

            // Check for direct recursion (function calls itself directly)
            if cycle.len() == 2 && cycle[0] == node_id && cycle[1] == node_id {
                direct_recursion = true;
            }
            // Check for indirect recursion (function is in a cycle with other functions)
            else if cycle.len() >= 2 && cycle.contains(&node_id.to_string()) {
                indirect_recursion = true;
            }
        }

        // Determine recursion type
        let recursion_type = if direct_recursion && indirect_recursion {
            RecursionType::Complex
        } else if direct_recursion {
            RecursionType::Direct
        } else if indirect_recursion {
            // Check if this is mutual recursion (two functions calling each other)
            // Mutual recursion: A -> B -> A (exactly 3 elements, first and last are the same)
            // Indirect recursion: longer chains or more complex patterns
            let mutual_cycles = cycles
                .iter()
                .filter(|cycle| {
                    cycle.len() == 3
                        && cycle.contains(&node_id.to_string())
                        && cycle[0] == cycle[2]
                        && cycle[0] != cycle[1] // A -> B -> A pattern
                        && cycle[0] == node_id // The node must be the starting point of the cycle
                })
                .count();
            if mutual_cycles > 0 {
                RecursionType::Mutual
            } else {
                RecursionType::Indirect
            }
        } else {
            RecursionType::None
        };

        RecursiveCallInfo {
            is_recursive: direct_recursion || indirect_recursion,
            direct_recursion,
            indirect_recursion,
            max_depth,
            cycle_functions: if direct_recursion || indirect_recursion {
                all_cycle_functions.into_iter().collect()
            } else {
                Vec::new()
            },
            recursion_type,
        }
    }

    /// Apply recursive call detection and update graph metadata
    pub fn apply_recursive_call_detection(&mut self, config: &RecursiveCallConfig) -> Result<()> {
        let recursive_info = self.detect_recursive_calls(config)?;

        // Update node metadata with recursion information
        if config.include_recursion_metadata {
            for node in &mut self.nodes {
                if let Some(info) = recursive_info.get(&node.id) {
                    node.metadata.insert(
                        "recursive_call_info".to_string(),
                        serde_json::to_value(info).unwrap_or(serde_json::Value::Null),
                    );

                    // Add visual indicators for recursive functions
                    if info.is_recursive {
                        node.color = Some("#ff6b6b".to_string()); // Red for recursive
                        node.metadata
                            .insert("is_recursive".to_string(), serde_json::Value::Bool(true));
                    }
                }
            }
        }

        // Update edges to mark recursive calls
        if config.mark_recursive_edges {
            for edge in &mut self.edges {
                if edge.edge_type == EdgeType::Calls {
                    let source_info = recursive_info.get(&edge.source);
                    let target_info = recursive_info.get(&edge.target);

                    if let (Some(source), Some(target)) = (source_info, target_info) {
                        if source.is_recursive && target.is_recursive {
                            // Check if this edge is part of a recursive cycle
                            if source.cycle_functions.contains(&edge.target) {
                                edge.edge_type = EdgeType::RecursiveCall;
                                edge.metadata.insert(
                                    "is_recursive_call".to_string(),
                                    serde_json::Value::Bool(true),
                                );
                                edge.metadata.insert(
                                    "color".to_string(),
                                    serde_json::Value::String("#ff6b6b".to_string()),
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get recursive call statistics for the graph
    pub fn get_recursive_call_statistics(&self) -> RecursiveCallStatistics {
        let mut stats = RecursiveCallStatistics::default();

        for node in &self.nodes {
            if let Some(recursive_info) = node.metadata.get("recursive_call_info") {
                if let Ok(info) =
                    serde_json::from_value::<RecursiveCallInfo>(recursive_info.clone())
                {
                    stats.total_recursive_functions += 1;

                    if info.direct_recursion {
                        stats.direct_recursion_count += 1;
                    }
                    if info.indirect_recursion {
                        stats.indirect_recursion_count += 1;
                    }

                    match info.recursion_type {
                        RecursionType::Direct => stats.direct_recursion_count += 1,
                        RecursionType::Indirect => stats.indirect_recursion_count += 1,
                        RecursionType::Complex => stats.complex_recursion_count += 1,
                        RecursionType::Mutual => stats.mutual_recursion_count += 1,
                        RecursionType::None => {}
                    }

                    stats.max_recursion_depth = stats.max_recursion_depth.max(info.max_depth);
                }
            }
        }

        stats.recursive_edges = self
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::RecursiveCall)
            .count();
        stats.total_functions = self
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .count();

        if stats.total_functions > 0 {
            stats.recursion_percentage =
                (stats.total_recursive_functions as f32 / stats.total_functions as f32) * 100.0;
        }

        stats
    }
}

/// Statistics about recursive calls in the graph
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecursiveCallStatistics {
    /// Total number of recursive functions
    pub total_recursive_functions: usize,
    /// Number of functions with direct recursion
    pub direct_recursion_count: usize,
    /// Number of functions with indirect recursion
    pub indirect_recursion_count: usize,
    /// Number of functions with complex recursion
    pub complex_recursion_count: usize,
    /// Number of functions with mutual recursion
    pub mutual_recursion_count: usize,
    /// Maximum recursion depth found
    pub max_recursion_depth: usize,
    /// Number of recursive edges
    pub recursive_edges: usize,
    /// Total number of functions
    pub total_functions: usize,
    /// Percentage of functions that are recursive
    pub recursion_percentage: f32,
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStatistics {
    /// Total number of nodes
    pub node_count: usize,
    /// Total number of edges
    pub edge_count: usize,
    /// Average degree (connections per node)
    pub avg_degree: f32,
    /// Maximum degree of any node
    pub max_degree: usize,
    /// Graph density (actual edges / possible edges)
    pub graph_density: f32,
}
