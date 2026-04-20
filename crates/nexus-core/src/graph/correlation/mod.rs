//! Graph Correlation Analysis - Code relationship visualization & LLM assistance
//!
//! This module provides functionality to automatically build correlation graphs
//! between files, function calls, and libraries to help LLMs understand processing flow.
//!
//! # Graph Types
//!
//! - **Call Graph**: Function call relationships and execution flow
//! - **Dependency Graph**: Module/library dependencies and imports
//! - **Data Flow Graph**: Data transformation and variable usage
//! - **Component Graph**: High-level architectural components
//!
//! # Features
//!
//! - Automatic graph generation from vectorizer data
//! - Pattern recognition (pipelines, event-driven, architectural)
//! - LLM assistance with graph context
//! - Interactive visualization support
//! - Real-time graph updates
//! - Multiple export formats (JSON, GraphML, GEXF)

use crate::vectorizer_cache::{QueryMetadata, VectorizerCache};
use crate::{Error, Result};
pub use component::{
    ClassInfo, ComponentAnalyzer, ComponentCouplingAnalyzer, ComponentCouplingMetrics,
    ComponentRelationship, ComponentRelationshipInfo, ComponentStatistics,
    ComponentVisualizationConfig, FieldInfo, InterfaceInfo, MethodInfo, OOHierarchyLayout,
    ParameterInfo, PropertyInfo, apply_component_visualization, apply_oop_hierarchy_layout,
};
pub use data_flow::{
    DataFlowAnalyzer, DataFlowEdge, DataFlowVisualizationConfig, DataTransformation,
    FlowBasedLayout, FlowType, TransformationType, TypePropagator, UsageType, VariableTracker,
    VariableUsage, VariableUsageSite, apply_data_flow_visualization, apply_flow_layout,
    visualize_data_flow,
};
pub use dependency_filter::{
    DependencyFilter, calculate_node_depths, filter_dependency_graph, get_direct_dependencies,
    get_transitive_dependencies, identify_leaf_and_root_nodes,
};
pub use graph_diff::{
    EdgeDiff, GraphDiff, NodeDiff, apply_diff, calculate_structural_similarity, compare_graphs,
};
pub use graph_export::{ExportFormat, export_graph};
pub use graph_statistics::calculate_statistics;
pub use impact_analysis::{
    ChangeImpactResult, ChangeType, ImpactAnalysis, ImpactSeverity, analyze_batch_impact,
    analyze_change_impact, analyze_impact, calculate_propagation_distance, identify_critical_nodes,
};
pub use pattern_recognition::{
    ArchitecturalPatternDetector, DesignPatternDetector, DetectedPattern,
    EventDrivenPatternDetector, PatternDetectionResult, PatternDetector, PatternDifficulty,
    PatternMaturity, PatternOverlayConfig, PatternQualityMetrics, PatternRecommendation,
    PatternRecommendationEngine, PatternStatistics, PatternType, PipelinePatternDetector,
    apply_pattern_overlays, calculate_pattern_quality_metrics,
};
pub use performance::{
    GraphCache, PerformanceMetrics, PerformanceProfiler, PerformanceSummary, calculate_complexity,
    optimize_graph,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub use vectorizer_cache::{CacheKeyBuilder, CacheStatistics, VectorizerQueryCache};
pub use version_constraints::{
    ConflictSeverity, DependencyVersion, VersionCompatibility, VersionConflict, VersionConstraint,
    analyze_version_constraints,
};
pub use visualization::{
    CacheStats, EdgeInteractionData, EdgeLineStyle, EdgeStyle, GraphRenderer, InteractionData,
    LayoutAlgorithm, NodeInteractionData, NodeShape, NodeStyle, SvgRenderer, VisualizationCache,
    VisualizationConfig, apply_layout, create_svg_renderer, generate_interaction_data,
    render_graph_to_svg,
};

/// Hierarchical call graph layout algorithms
pub mod hierarchical_layout;

/// Graph visualization and rendering
pub mod visualization;

/// Call graph filtering and search functionality
pub mod call_graph_filtering;

/// Pattern recognition for architectural and design patterns
pub mod pattern_recognition;

/// Graph export to multiple formats
pub mod graph_export;

/// Graph statistics and metrics
pub mod graph_statistics;

/// Graph comparison and diff
pub mod graph_diff;

/// Performance optimization utilities
pub mod performance;

/// Dependency graph filtering
pub mod dependency_filter;

/// Dependency impact analysis
pub mod impact_analysis;

/// Enhanced vectorizer query caching
pub mod vectorizer_cache;

/// Version constraint analysis for dependencies
pub mod version_constraints;

/// Data flow analysis and tracking
pub mod data_flow;

/// Component analysis for object-oriented code
pub mod component;

/// Query execution against the vectorizer MCP
pub mod query_executor;

/// Vectorizer-driven graph extraction
pub mod vectorizer_extractor;

#[cfg(test)]
mod tests;

pub use query_executor::QueryExecutor;
pub use vectorizer_extractor::{
    VectorizerCollections, VectorizerExtractorConfig, VectorizerGraphExtractor,
};

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

        let layout_engine = hierarchical_layout::HierarchicalCallGraphLayout::with_default_config();
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
        config: hierarchical_layout::HierarchicalCallGraphConfig,
    ) -> Result<()> {
        if self.graph_type != GraphType::Call {
            return Err(Error::GraphCorrelation(
                "Hierarchical layout is only supported for call graphs".to_string(),
            ));
        }

        let layout_engine = hierarchical_layout::HierarchicalCallGraphLayout::new(config);
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
                    cycle.len() == 3 &&
                    cycle.contains(&node_id.to_string()) &&
                    cycle[0] == cycle[2] && cycle[0] != cycle[1] && // A -> B -> A pattern
                    cycle[0] == node_id // The node must be the starting point of the cycle
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

/// Graph builder trait for different graph types
pub trait GraphBuilder {
    /// Build a correlation graph from source data (synchronous version for backward compatibility)
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph>;

    /// Build a call graph from extracted data (synchronous version)
    fn build_call_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        // Default implementation delegates to synchronous build method
        self.build(data)
    }

    /// Build a dependency graph from extracted data (synchronous version)
    fn build_dependency_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        // Default implementation delegates to synchronous build method
        self.build(data)
    }

    /// Build a data flow graph from extracted data (synchronous version)
    fn build_data_flow_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        // Default implementation delegates to synchronous build method
        self.build(data)
    }

    /// Build a component graph from extracted data (synchronous version)
    fn build_component_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        // Default implementation delegates to synchronous build method
        self.build(data)
    }

    /// Get the graph type this builder creates
    fn graph_type(&self) -> GraphType;

    /// Get builder name
    fn name(&self) -> &str;

    /// Get builder capabilities
    fn capabilities(&self) -> GraphBuilderCapabilities {
        GraphBuilderCapabilities::default()
    }

    /// Validate source data before building
    fn validate_source_data(&self, data: &GraphSourceData) -> Result<()> {
        if data.files.is_empty() {
            return Err(Error::GraphCorrelation(
                "No source files provided".to_string(),
            ));
        }
        Ok(())
    }

    /// Get builder configuration
    fn config(&self) -> Option<&GraphBuilderConfig> {
        None
    }
}

/// Source data for graph building
#[derive(Debug, Clone)]
pub struct GraphSourceData {
    /// File paths and their content
    pub files: HashMap<String, String>,
    /// Function definitions and calls
    pub functions: HashMap<String, Vec<String>>,
    /// Import/export relationships
    pub imports: HashMap<String, Vec<String>>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl GraphSourceData {
    /// Create new source data
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            functions: HashMap::new(),
            imports: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a file with its content
    pub fn add_file(&mut self, path: String, content: String) {
        self.files.insert(path, content);
    }

    /// Add a single function definition
    pub fn add_function(&mut self, _name: String, _signature: String, _file: String, _line: usize) {
        // Method kept for API compatibility but not storing individual functions
        // Use add_functions instead for better performance
    }

    /// Add a single call between functions
    pub fn add_call(&mut self, _caller: String, _callee: String, _file: String, _line: usize) {
        // Method kept for API compatibility but not storing individual calls
        // Use add_functions instead for better performance
    }

    /// Add a single import
    pub fn add_import(&mut self, file: String, import: String) {
        self.imports.entry(file).or_default().push(import);
    }

    /// Add function calls for a file
    pub fn add_functions(&mut self, file: String, functions: Vec<String>) {
        self.functions.insert(file, functions);
    }

    /// Add imports for a file
    pub fn add_imports(&mut self, file: String, imports: Vec<String>) {
        self.imports.insert(file, imports);
    }
}

impl Default for GraphSourceData {
    fn default() -> Self {
        Self::new()
    }
}

/// Call graph builder implementation
pub struct CallGraphBuilder {
    name: String,
    use_hierarchical_layout: bool,
    layout_config: Option<hierarchical_layout::HierarchicalCallGraphConfig>,
    recursive_call_config: RecursiveCallConfig,
}

impl CallGraphBuilder {
    /// Create a new call graph builder
    pub fn new(name: String) -> Self {
        Self {
            name,
            use_hierarchical_layout: false,
            layout_config: None,
            recursive_call_config: RecursiveCallConfig::default(),
        }
    }

    /// Create a new call graph builder with hierarchical layout
    pub fn new_with_hierarchical_layout(name: String) -> Self {
        Self {
            name,
            use_hierarchical_layout: true,
            layout_config: Some(hierarchical_layout::HierarchicalCallGraphConfig::default()),
            recursive_call_config: RecursiveCallConfig::default(),
        }
    }

    /// Enable hierarchical layout with custom configuration
    pub fn with_hierarchical_layout(
        mut self,
        config: hierarchical_layout::HierarchicalCallGraphConfig,
    ) -> Self {
        self.use_hierarchical_layout = true;
        self.layout_config = Some(config);
        self
    }

    /// Disable hierarchical layout
    pub fn without_hierarchical_layout(mut self) -> Self {
        self.use_hierarchical_layout = false;
        self.layout_config = None;
        self
    }

    /// Configure recursive call detection
    pub fn with_recursive_call_detection(mut self, config: RecursiveCallConfig) -> Self {
        self.recursive_call_config = config;
        self
    }

    /// Enable recursive call detection with default configuration
    pub fn enable_recursive_call_detection(mut self) -> Self {
        self.recursive_call_config = RecursiveCallConfig::default();
        self
    }

    /// Disable recursive call detection
    pub fn disable_recursive_call_detection(mut self) -> Self {
        self.recursive_call_config = RecursiveCallConfig {
            max_search_depth: 0,
            detect_indirect: false,
            detect_mutual: false,
            include_recursion_metadata: false,
            mark_recursive_edges: false,
        };
        self
    }
}

impl GraphBuilder for CallGraphBuilder {
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(GraphType::Call, self.name.clone());

        // Add nodes for each file
        for file_path in source_data.files.keys() {
            let node_id = format!("file:{}", file_path);
            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Module,
                label: file_path.clone(),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            };
            graph.add_node(node)?;
        }

        // Add nodes for each function
        for (file_path, functions) in &source_data.functions {
            for function in functions {
                let node_id = format!("func:{}:{}", file_path, function);
                let node = GraphNode {
                    id: node_id.clone(),
                    node_type: NodeType::Function,
                    label: function.clone(),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                };
                graph.add_node(node)?;

                // Add edge from file to function
                let file_id = format!("file:{}", file_path);
                let edge = GraphEdge {
                    id: format!("edge:{}:{}", file_id, node_id),
                    source: file_id,
                    target: node_id,
                    edge_type: EdgeType::Uses,
                    weight: 1.0,
                    metadata: HashMap::new(),
                    label: None,
                };
                graph.add_edge(edge)?;
            }
        }

        // Apply hierarchical layout if enabled
        if self.use_hierarchical_layout {
            if let Some(config) = &self.layout_config {
                let layout_engine =
                    hierarchical_layout::HierarchicalCallGraphLayout::new(config.clone());
                graph = layout_engine.layout(graph)?;
            } else {
                let layout_engine =
                    hierarchical_layout::HierarchicalCallGraphLayout::with_default_config();
                graph = layout_engine.layout(graph)?;
            }
        }

        // Apply recursive call detection if enabled
        if self.recursive_call_config.max_search_depth > 0 {
            graph.apply_recursive_call_detection(&self.recursive_call_config)?;
        }

        Ok(graph)
    }

    fn build_call_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        self.build(data)
    }

    fn build_dependency_graph(&self, _data: &GraphSourceData) -> Result<CorrelationGraph> {
        Err(Error::GraphCorrelation(
            "CallGraphBuilder does not support dependency graphs".to_string(),
        ))
    }

    fn build_data_flow_graph(&self, _data: &GraphSourceData) -> Result<CorrelationGraph> {
        Err(Error::GraphCorrelation(
            "CallGraphBuilder does not support data flow graphs".to_string(),
        ))
    }

    fn build_component_graph(&self, _data: &GraphSourceData) -> Result<CorrelationGraph> {
        Err(Error::GraphCorrelation(
            "CallGraphBuilder does not support component graphs".to_string(),
        ))
    }

    fn graph_type(&self) -> GraphType {
        GraphType::Call
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> GraphBuilderCapabilities {
        GraphBuilderCapabilities {
            supports_call_graphs: true,
            supports_dependency_graphs: false,
            supports_data_flow_graphs: false,
            supports_component_graphs: false,
            supports_async: true,
            supports_parallel: false,
            supports_caching: false,
            supports_validation: true,
        }
    }
}

/// Dependency graph builder implementation
pub struct DependencyGraphBuilder {
    name: String,
}

impl DependencyGraphBuilder {
    /// Create a new dependency graph builder
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl GraphBuilder for DependencyGraphBuilder {
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(GraphType::Dependency, self.name.clone());

        // Add nodes for each file
        for file_path in source_data.files.keys() {
            let node_id = format!("file:{}", file_path);
            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Module,
                label: file_path.clone(),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            };
            graph.add_node(node)?;
        }

        // Add edges for imports
        for (file_path, imports) in &source_data.imports {
            let source_id = format!("file:{}", file_path);

            for import in imports {
                let target_id = format!("file:{}", import);

                // Only add edge if target file exists
                if source_data.files.contains_key(import) {
                    let edge = GraphEdge {
                        id: format!("edge:{}:{}", source_id, target_id),
                        source: source_id.clone(),
                        target: target_id,
                        edge_type: EdgeType::Imports,
                        weight: 1.0,
                        metadata: HashMap::new(),
                        label: None,
                    };
                    graph.add_edge(edge)?;
                }
            }
        }

        Ok(graph)
    }

    fn build_call_graph(&self, _data: &GraphSourceData) -> Result<CorrelationGraph> {
        Err(Error::GraphCorrelation(
            "DependencyGraphBuilder does not support call graphs".to_string(),
        ))
    }

    fn build_dependency_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        self.build(data)
    }

    fn build_data_flow_graph(&self, _data: &GraphSourceData) -> Result<CorrelationGraph> {
        Err(Error::GraphCorrelation(
            "DependencyGraphBuilder does not support data flow graphs".to_string(),
        ))
    }

    fn build_component_graph(&self, _data: &GraphSourceData) -> Result<CorrelationGraph> {
        Err(Error::GraphCorrelation(
            "DependencyGraphBuilder does not support component graphs".to_string(),
        ))
    }

    fn graph_type(&self) -> GraphType {
        GraphType::Dependency
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> GraphBuilderCapabilities {
        GraphBuilderCapabilities {
            supports_call_graphs: false,
            supports_dependency_graphs: true,
            supports_data_flow_graphs: false,
            supports_component_graphs: false,
            supports_async: true,
            supports_parallel: false,
            supports_caching: false,
            supports_validation: true,
        }
    }
}

/// Default graph builder that provides a base implementation
/// for building all types of correlation graphs
pub struct DefaultGraphBuilder {
    /// Name of the builder
    name: String,
    /// Configuration for graph building
    config: GraphBuilderConfig,
}

/// Configuration for graph building
#[derive(Debug, Clone)]
pub struct GraphBuilderConfig {
    /// Maximum number of nodes to process
    pub max_nodes: Option<usize>,
    /// Maximum depth for recursive analysis
    pub max_depth: Option<usize>,
    /// Whether to include metadata in nodes
    pub include_metadata: bool,
    /// Whether to calculate positions for visualization
    pub calculate_positions: bool,
    /// Default node size for visualization
    pub default_node_size: f32,
    /// Default edge weight
    pub default_edge_weight: f32,
    /// Whether to enable parallel processing
    pub parallel_processing: bool,
    /// Whether to enable caching
    pub enable_caching: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Whether to validate graph integrity
    pub validate_integrity: bool,
    /// Whether to optimize for performance
    pub performance_mode: bool,
}

/// Graph builder capabilities
#[derive(Debug, Clone, Default)]
pub struct GraphBuilderCapabilities {
    /// Whether the builder supports call graphs
    pub supports_call_graphs: bool,
    /// Whether the builder supports dependency graphs
    pub supports_dependency_graphs: bool,
    /// Whether the builder supports data flow graphs
    pub supports_data_flow_graphs: bool,
    /// Whether the builder supports component graphs
    pub supports_component_graphs: bool,
    /// Whether the builder supports async operations
    pub supports_async: bool,
    /// Whether the builder supports parallel processing
    pub supports_parallel: bool,
    /// Whether the builder supports caching
    pub supports_caching: bool,
    /// Whether the builder supports validation
    pub supports_validation: bool,
}

impl Default for GraphBuilderConfig {
    fn default() -> Self {
        Self {
            max_nodes: Some(1000),
            max_depth: Some(10),
            include_metadata: true,
            calculate_positions: true,
            default_node_size: 1.0,
            default_edge_weight: 1.0,
            parallel_processing: true,
            enable_caching: true,
            cache_ttl_seconds: 3600, // 1 hour
            validate_integrity: true,
            performance_mode: false,
        }
    }
}

impl DefaultGraphBuilder {
    /// Create a new default graph builder
    pub fn new(name: String) -> Self {
        Self {
            name,
            config: GraphBuilderConfig::default(),
        }
    }

    /// Create a new default graph builder with custom configuration
    pub fn with_config(name: String, config: GraphBuilderConfig) -> Self {
        Self { name, config }
    }

    /// Get the builder configuration
    pub fn config(&self) -> &GraphBuilderConfig {
        &self.config
    }

    /// Update the builder configuration
    pub fn set_config(&mut self, config: GraphBuilderConfig) {
        self.config = config;
    }

    /// Build a call graph using the default implementation
    pub fn build_call_graph(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let mut graph =
            CorrelationGraph::new(GraphType::Call, format!("{} - Call Graph", self.name));

        // Add nodes for each file
        for file_path in source_data.files.keys() {
            if let Some(max_nodes) = self.config.max_nodes {
                if graph.nodes.len() >= max_nodes {
                    break;
                }
            }

            let node_id = format!("file:{}", file_path);
            let mut metadata = HashMap::new();
            if self.config.include_metadata {
                metadata.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.clone()),
                );
                metadata.insert(
                    "node_type".to_string(),
                    serde_json::Value::String("file".to_string()),
                );
            }

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Module,
                label: file_path.clone(),
                metadata,
                position: if self.config.calculate_positions {
                    Some((0.0, 0.0)) // Will be calculated by layout engine
                } else {
                    None
                },
                size: Some(self.config.default_node_size),
                color: Some("#3498db".to_string()),
            };
            graph.add_node(node)?;
        }

        // Add nodes for each function
        for (file_path, functions) in &source_data.functions {
            for function in functions {
                if let Some(max_nodes) = self.config.max_nodes {
                    if graph.nodes.len() >= max_nodes {
                        break;
                    }
                }

                let node_id = format!("func:{}:{}", file_path, function);
                let mut metadata = HashMap::new();
                if self.config.include_metadata {
                    metadata.insert(
                        "file_path".to_string(),
                        serde_json::Value::String(file_path.clone()),
                    );
                    metadata.insert(
                        "function_name".to_string(),
                        serde_json::Value::String(function.clone()),
                    );
                    metadata.insert(
                        "node_type".to_string(),
                        serde_json::Value::String("function".to_string()),
                    );
                }

                let node = GraphNode {
                    id: node_id.clone(),
                    node_type: NodeType::Function,
                    label: function.clone(),
                    metadata,
                    position: if self.config.calculate_positions {
                        Some((0.0, 0.0)) // Will be calculated by layout engine
                    } else {
                        None
                    },
                    size: Some(self.config.default_node_size * 0.8),
                    color: Some("#e74c3c".to_string()),
                };
                graph.add_node(node)?;

                // Add edge from file to function
                let file_id = format!("file:{}", file_path);
                let edge = GraphEdge {
                    id: format!("edge:{}:{}", file_id, node_id),
                    source: file_id,
                    target: node_id,
                    edge_type: EdgeType::Uses,
                    weight: self.config.default_edge_weight,
                    metadata: if self.config.include_metadata {
                        let mut edge_metadata = HashMap::new();
                        edge_metadata.insert(
                            "relationship".to_string(),
                            serde_json::Value::String("contains".to_string()),
                        );
                        edge_metadata
                    } else {
                        HashMap::new()
                    },
                    label: Some("contains".to_string()),
                };
                graph.add_edge(edge)?;
            }
        }

        Ok(graph)
    }

    /// Build a dependency graph using the default implementation
    pub fn build_dependency_graph(
        &self,
        source_data: &GraphSourceData,
    ) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(
            GraphType::Dependency,
            format!("{} - Dependency Graph", self.name),
        );

        // Add nodes for each file
        for file_path in source_data.files.keys() {
            if let Some(max_nodes) = self.config.max_nodes {
                if graph.nodes.len() >= max_nodes {
                    break;
                }
            }

            let node_id = format!("file:{}", file_path);
            let mut metadata = HashMap::new();
            if self.config.include_metadata {
                metadata.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.clone()),
                );
                metadata.insert(
                    "node_type".to_string(),
                    serde_json::Value::String("file".to_string()),
                );
            }

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Module,
                label: file_path.clone(),
                metadata,
                position: if self.config.calculate_positions {
                    Some((0.0, 0.0)) // Will be calculated by layout engine
                } else {
                    None
                },
                size: Some(self.config.default_node_size),
                color: Some("#2ecc71".to_string()),
            };
            graph.add_node(node)?;
        }

        // Add edges for imports
        for (file_path, imports) in &source_data.imports {
            let source_id = format!("file:{}", file_path);

            for import in imports {
                let target_id = format!("file:{}", import);

                // Only add edge if target file exists
                if source_data.files.contains_key(import) {
                    let edge = GraphEdge {
                        id: format!("edge:{}:{}", source_id, target_id),
                        source: source_id.clone(),
                        target: target_id,
                        edge_type: EdgeType::Imports,
                        weight: self.config.default_edge_weight,
                        metadata: if self.config.include_metadata {
                            let mut edge_metadata = HashMap::new();
                            edge_metadata.insert(
                                "relationship".to_string(),
                                serde_json::Value::String("imports".to_string()),
                            );
                            edge_metadata
                        } else {
                            HashMap::new()
                        },
                        label: Some("imports".to_string()),
                    };
                    graph.add_edge(edge)?;
                }
            }
        }

        Ok(graph)
    }

    /// Build a data flow graph using the default implementation
    pub fn build_data_flow_graph(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(
            GraphType::DataFlow,
            format!("{} - Data Flow Graph", self.name),
        );

        // Use DataFlowAnalyzer for enhanced variable tracking (Task 11.2)
        let mut analyzer = DataFlowAnalyzer::new();

        // Analyze source code for variable usage
        if let Err(e) = analyzer.analyze_source_code(&source_data.files) {
            tracing::warn!("Data flow analysis had issues: {}", e);
        }

        // Build base graph with file nodes
        for file_path in source_data.files.keys() {
            if let Some(max_nodes) = self.config.max_nodes {
                if graph.nodes.len() >= max_nodes {
                    break;
                }
            }

            let node_id = format!("data:{}", file_path);
            let mut metadata = HashMap::new();
            if self.config.include_metadata {
                metadata.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.clone()),
                );
                metadata.insert(
                    "node_type".to_string(),
                    serde_json::Value::String("data_source".to_string()),
                );

                // Add variable count from analyzer
                let var_count = analyzer.tracker().get_file_variables(file_path).len();
                metadata.insert(
                    "variable_count".to_string(),
                    serde_json::Value::Number(var_count.into()),
                );
            }

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Variable,
                label: format!("Data from {}", file_path),
                metadata,
                position: if self.config.calculate_positions {
                    Some((0.0, 0.0)) // Will be calculated by layout engine
                } else {
                    None
                },
                size: Some(self.config.default_node_size * 0.6),
                color: Some("#f39c12".to_string()),
            };
            graph.add_node(node)?;
        }

        // Enhance graph with variable tracking data
        graph = analyzer.build_enhanced_data_flow_graph(&graph)?;

        Ok(graph)
    }

    /// Build a component graph using the default implementation
    pub fn build_component_graph(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(
            GraphType::Component,
            format!("{} - Component Graph", self.name),
        );

        // For now, create a basic component graph based on file structure
        // This will be enhanced in future tasks
        for file_path in source_data.files.keys() {
            if let Some(max_nodes) = self.config.max_nodes {
                if graph.nodes.len() >= max_nodes {
                    break;
                }
            }

            let node_id = format!("component:{}", file_path);
            let mut metadata = HashMap::new();
            if self.config.include_metadata {
                metadata.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.clone()),
                );
                metadata.insert(
                    "node_type".to_string(),
                    serde_json::Value::String("component".to_string()),
                );
            }

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Class,
                label: format!("Component {}", file_path),
                metadata,
                position: if self.config.calculate_positions {
                    Some((0.0, 0.0)) // Will be calculated by layout engine
                } else {
                    None
                },
                size: Some(self.config.default_node_size),
                color: Some("#9b59b6".to_string()),
            };
            graph.add_node(node)?;
        }

        Ok(graph)
    }
}

impl GraphBuilder for DefaultGraphBuilder {
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        // Default implementation builds a call graph
        // This can be overridden by specific graph type builders
        self.build_call_graph(source_data)
    }

    fn build_call_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        self.build_call_graph(data)
    }

    fn build_dependency_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        self.build_dependency_graph(data)
    }

    fn build_data_flow_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        self.build_data_flow_graph(data)
    }

    fn build_component_graph(&self, data: &GraphSourceData) -> Result<CorrelationGraph> {
        self.build_component_graph(data)
    }

    fn graph_type(&self) -> GraphType {
        GraphType::Call
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> GraphBuilderCapabilities {
        GraphBuilderCapabilities {
            supports_call_graphs: true,
            supports_dependency_graphs: true,
            supports_data_flow_graphs: true,
            supports_component_graphs: true,
            supports_async: true,
            supports_parallel: self.config.parallel_processing,
            supports_caching: self.config.enable_caching,
            supports_validation: self.config.validate_integrity,
        }
    }

    fn validate_source_data(&self, data: &GraphSourceData) -> Result<()> {
        // Enhanced validation
        if data.files.is_empty() {
            return Err(Error::GraphCorrelation(
                "No source files provided".to_string(),
            ));
        }

        // Check if we exceed max nodes limit
        if let Some(max_nodes) = self.config.max_nodes {
            let total_potential_nodes =
                data.files.len() + data.functions.values().map(|v| v.len()).sum::<usize>();
            if total_potential_nodes > max_nodes {
                return Err(Error::GraphCorrelation(format!(
                    "Too many potential nodes: {} (max: {})",
                    total_potential_nodes, max_nodes
                )));
            }
        }

        // Validate file paths are not empty
        for file_path in data.files.keys() {
            if file_path.trim().is_empty() {
                return Err(Error::GraphCorrelation("Empty file path found".to_string()));
            }
        }

        Ok(())
    }

    fn config(&self) -> Option<&GraphBuilderConfig> {
        Some(&self.config)
    }
}

/// Wrapper builders for DataFlow and Component graphs
pub struct DataFlowGraphBuilder;
pub struct ComponentGraphBuilder;

impl DataFlowGraphBuilder {
    pub fn new(_name: String) -> Self {
        Self
    }
}

impl ComponentGraphBuilder {
    pub fn new(_name: String) -> Self {
        Self
    }
}

impl GraphBuilder for DataFlowGraphBuilder {
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let builder = DefaultGraphBuilder::new("DataFlow".to_string());
        builder.build_data_flow_graph(source_data)
    }

    fn graph_type(&self) -> GraphType {
        GraphType::DataFlow
    }

    fn name(&self) -> &str {
        "DataFlow Graph"
    }

    fn capabilities(&self) -> GraphBuilderCapabilities {
        GraphBuilderCapabilities {
            supports_call_graphs: false,
            supports_dependency_graphs: false,
            supports_data_flow_graphs: true,
            supports_component_graphs: false,
            supports_async: true,
            supports_parallel: false,
            supports_caching: false,
            supports_validation: true,
        }
    }
}

impl GraphBuilder for ComponentGraphBuilder {
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph> {
        let builder = DefaultGraphBuilder::new("Component".to_string());
        builder.build_component_graph(source_data)
    }

    fn graph_type(&self) -> GraphType {
        GraphType::Component
    }

    fn name(&self) -> &str {
        "Component Graph"
    }

    fn capabilities(&self) -> GraphBuilderCapabilities {
        GraphBuilderCapabilities {
            supports_call_graphs: false,
            supports_dependency_graphs: false,
            supports_data_flow_graphs: false,
            supports_component_graphs: true,
            supports_async: true,
            supports_parallel: false,
            supports_caching: false,
            supports_validation: true,
        }
    }
}

/// Graph correlation manager
pub struct GraphCorrelationManager {
    /// Available graph builders
    builders: HashMap<GraphType, Box<dyn GraphBuilder + Send + Sync>>,
    /// Default graph builder
    default_builder: Option<DefaultGraphBuilder>,
}

impl GraphCorrelationManager {
    /// Create a new graph correlation manager
    pub fn new() -> Self {
        let mut manager = Self {
            builders: HashMap::new(),
            default_builder: Some(DefaultGraphBuilder::new(
                "Default Graph Builder".to_string(),
            )),
        };

        // Register default builders for all graph types
        manager.register_builder(Box::new(CallGraphBuilder::new("Call Graph".to_string())));
        manager.register_builder(Box::new(DependencyGraphBuilder::new(
            "Dependency Graph".to_string(),
        )));
        manager.register_builder(Box::new(DataFlowGraphBuilder::new(
            "DataFlow Graph".to_string(),
        )));
        manager.register_builder(Box::new(ComponentGraphBuilder::new(
            "Component Graph".to_string(),
        )));

        manager
    }

    /// Register a graph builder
    pub fn register_builder(&mut self, builder: Box<dyn GraphBuilder + Send + Sync>) {
        self.builders.insert(builder.graph_type(), builder);
    }

    /// Build a graph of the specified type
    pub fn build_graph(
        &self,
        graph_type: GraphType,
        source_data: &GraphSourceData,
    ) -> Result<CorrelationGraph> {
        let builder = self.builders.get(&graph_type).ok_or_else(|| {
            Error::GraphCorrelation(format!("No builder found for graph type: {:?}", graph_type))
        })?;

        builder.build(source_data)
    }

    /// Get available graph types
    pub fn available_graph_types(&self) -> Vec<GraphType> {
        self.builders.keys().cloned().collect()
    }
}

impl Default for GraphCorrelationManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Collection Query Interfaces
// ============================================================================

/// Trait for different types of collection queries
pub trait CollectionQuery {
    /// Execute the query and return results
    #[allow(async_fn_in_trait)]
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult>;

    /// Get the collection name for this query
    fn collection(&self) -> &str;

    /// Get query parameters as JSON
    fn parameters(&self) -> serde_json::Value;
}

/// Enum wrapper for different collection query types
pub enum CollectionQueryEnum {
    Semantic(SemanticQuery),
    Metadata(MetadataQuery),
    Hybrid(HybridQuery),
}

impl CollectionQuery for CollectionQueryEnum {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        match self {
            CollectionQueryEnum::Semantic(query) => query.execute(executor).await,
            CollectionQueryEnum::Metadata(query) => query.execute(executor).await,
            CollectionQueryEnum::Hybrid(query) => query.execute(executor).await,
        }
    }

    fn collection(&self) -> &str {
        match self {
            CollectionQueryEnum::Semantic(query) => query.collection(),
            CollectionQueryEnum::Metadata(query) => query.collection(),
            CollectionQueryEnum::Hybrid(query) => query.collection(),
        }
    }

    fn parameters(&self) -> serde_json::Value {
        match self {
            CollectionQueryEnum::Semantic(query) => query.parameters(),
            CollectionQueryEnum::Metadata(query) => query.parameters(),
            CollectionQueryEnum::Hybrid(query) => query.parameters(),
        }
    }
}

/// Semantic search query for finding similar content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticQuery {
    /// Collection to search
    pub collection: String,
    /// Search query text
    pub query: String,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Similarity threshold (0.0 to 1.0)
    pub threshold: Option<f32>,
    /// Additional filters
    pub filters: Option<HashMap<String, serde_json::Value>>,
}

impl SemanticQuery {
    /// Create a new semantic query
    pub fn new(collection: String, query: String) -> Self {
        Self {
            collection,
            query,
            limit: None,
            threshold: None,
            filters: None,
        }
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Add a filter to the query
    pub fn with_filter(mut self, key: String, value: serde_json::Value) -> Self {
        if self.filters.is_none() {
            self.filters = Some(HashMap::new());
        }
        if let Some(ref mut filters) = self.filters {
            filters.insert(key, value);
        }
        self
    }
}

impl CollectionQuery for SemanticQuery {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        executor.execute_semantic_query(self).await
    }

    fn collection(&self) -> &str {
        &self.collection
    }

    fn parameters(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "query": self.query,
            "type": "semantic"
        });

        if let Some(limit) = self.limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(limit));
        }

        if let Some(threshold) = self.threshold {
            params["threshold"] =
                serde_json::Value::Number(serde_json::Number::from_f64(threshold as f64).unwrap());
        }

        if let Some(ref filters) = self.filters {
            params["filters"] = serde_json::Value::Object(
                filters
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }

        params
    }
}

/// Metadata-based query for filtering by specific fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataQuery {
    /// Collection to search
    pub collection: String,
    /// Field filters
    pub filters: HashMap<String, serde_json::Value>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Sort by field
    pub sort_by: Option<String>,
    /// Sort order (asc/desc)
    pub sort_order: Option<SortOrder>,
}

/// Sort order for queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    /// Ascending order
    Asc,
    /// Descending order
    Desc,
}

impl MetadataQuery {
    /// Create a new metadata query
    pub fn new(collection: String) -> Self {
        Self {
            collection,
            filters: HashMap::new(),
            limit: None,
            sort_by: None,
            sort_order: None,
        }
    }

    /// Add a field filter
    pub fn with_filter(mut self, field: String, value: serde_json::Value) -> Self {
        self.filters.insert(field, value);
        self
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set sorting
    pub fn with_sort(mut self, field: String, order: SortOrder) -> Self {
        self.sort_by = Some(field);
        self.sort_order = Some(order);
        self
    }
}

impl CollectionQuery for MetadataQuery {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        executor.execute_metadata_query(self).await
    }

    fn collection(&self) -> &str {
        &self.collection
    }

    fn parameters(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "type": "metadata",
            "filters": self.filters
        });

        if let Some(limit) = self.limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(limit));
        }

        if let Some(ref sort_by) = self.sort_by {
            params["sort_by"] = serde_json::Value::String(sort_by.clone());
        }

        if let Some(sort_order) = self.sort_order {
            params["sort_order"] = serde_json::Value::String(
                match sort_order {
                    SortOrder::Asc => "asc",
                    SortOrder::Desc => "desc",
                }
                .to_string(),
            );
        }

        params
    }
}

/// Hybrid query combining semantic and metadata search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridQuery {
    /// Collection to search
    pub collection: String,
    /// Semantic search query
    pub semantic_query: String,
    /// Metadata filters
    pub metadata_filters: HashMap<String, serde_json::Value>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Similarity threshold for semantic search
    pub threshold: Option<f32>,
    /// Weight for semantic vs metadata results (0.0 to 1.0)
    pub semantic_weight: f32,
}

impl HybridQuery {
    /// Create a new hybrid query
    pub fn new(collection: String, semantic_query: String) -> Self {
        Self {
            collection,
            semantic_query,
            metadata_filters: HashMap::new(),
            limit: None,
            threshold: None,
            semantic_weight: 0.7, // Default 70% semantic, 30% metadata
        }
    }

    /// Add a metadata filter
    pub fn with_metadata_filter(mut self, field: String, value: serde_json::Value) -> Self {
        self.metadata_filters.insert(field, value);
        self
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Set the semantic weight
    pub fn with_semantic_weight(mut self, weight: f32) -> Self {
        self.semantic_weight = weight.clamp(0.0, 1.0);
        self
    }
}

impl CollectionQuery for HybridQuery {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        executor.execute_hybrid_query(self).await
    }

    fn collection(&self) -> &str {
        &self.collection
    }

    fn parameters(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "type": "hybrid",
            "semantic_query": self.semantic_query,
            "metadata_filters": self.metadata_filters,
            "semantic_weight": self.semantic_weight
        });

        if let Some(limit) = self.limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(limit));
        }

        if let Some(threshold) = self.threshold {
            params["threshold"] =
                serde_json::Value::Number(serde_json::Number::from_f64(threshold as f64).unwrap());
        }

        params
    }
}

/// Query result containing search results and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Search results
    pub results: Vec<serde_json::Value>,
    /// Total number of results found
    pub total: usize,
    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
    /// Query metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl QueryResult {
    /// Create a new query result
    pub fn new(results: Vec<serde_json::Value>, total: usize, execution_time_ms: u64) -> Self {
        Self {
            results,
            total,
            execution_time_ms,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the result
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if the result is empty
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// Get the number of results
    pub fn len(&self) -> usize {
        self.results.len()
    }
}

/// Query builder for constructing complex queries
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    collection: String,
    query_type: QueryType,
    semantic_query: Option<String>,
    metadata_filters: HashMap<String, serde_json::Value>,
    limit: Option<usize>,
    threshold: Option<f32>,
    sort_by: Option<String>,
    sort_order: Option<SortOrder>,
    semantic_weight: f32,
}

/// Types of queries that can be built
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    /// Semantic search only
    Semantic,
    /// Metadata filtering only
    Metadata,
    /// Hybrid search
    Hybrid,
}

impl QueryBuilder {
    /// Create a new query builder
    pub fn new(collection: String) -> Self {
        Self {
            collection,
            query_type: QueryType::Semantic,
            semantic_query: None,
            metadata_filters: HashMap::new(),
            limit: None,
            threshold: None,
            sort_by: None,
            sort_order: None,
            semantic_weight: 0.7,
        }
    }

    /// Set the query type
    pub fn query_type(mut self, query_type: QueryType) -> Self {
        self.query_type = query_type;
        self
    }

    /// Set the semantic query
    pub fn semantic_query(mut self, query: String) -> Self {
        self.semantic_query = Some(query);
        self
    }

    /// Add a metadata filter
    pub fn metadata_filter(mut self, field: String, value: serde_json::Value) -> Self {
        self.metadata_filters.insert(field, value);
        self
    }

    /// Set the maximum number of results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the similarity threshold
    pub fn threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Set sorting
    pub fn sort(mut self, field: String, order: SortOrder) -> Self {
        self.sort_by = Some(field);
        self.sort_order = Some(order);
        self
    }

    /// Set the semantic weight for hybrid queries
    pub fn semantic_weight(mut self, weight: f32) -> Self {
        self.semantic_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Build the query
    pub fn build(self) -> Result<CollectionQueryEnum> {
        match self.query_type {
            QueryType::Semantic => {
                let query = self.semantic_query.ok_or_else(|| {
                    Error::GraphCorrelation(
                        "Semantic query is required for semantic search".to_string(),
                    )
                })?;

                let mut semantic_query = SemanticQuery::new(self.collection, query);
                if let Some(limit) = self.limit {
                    semantic_query = semantic_query.with_limit(limit);
                }
                if let Some(threshold) = self.threshold {
                    semantic_query = semantic_query.with_threshold(threshold);
                }
                for (key, value) in self.metadata_filters {
                    semantic_query = semantic_query.with_filter(key, value);
                }

                Ok(CollectionQueryEnum::Semantic(semantic_query))
            }
            QueryType::Metadata => {
                let mut metadata_query = MetadataQuery::new(self.collection);
                for (key, value) in self.metadata_filters {
                    metadata_query = metadata_query.with_filter(key, value);
                }
                if let Some(limit) = self.limit {
                    metadata_query = metadata_query.with_limit(limit);
                }
                if let (Some(sort_by), Some(sort_order)) = (self.sort_by, self.sort_order) {
                    metadata_query = metadata_query.with_sort(sort_by, sort_order);
                }

                Ok(CollectionQueryEnum::Metadata(metadata_query))
            }
            QueryType::Hybrid => {
                let query = self.semantic_query.ok_or_else(|| {
                    Error::GraphCorrelation(
                        "Semantic query is required for hybrid search".to_string(),
                    )
                })?;

                let mut hybrid_query = HybridQuery::new(self.collection, query);
                for (key, value) in self.metadata_filters {
                    hybrid_query = hybrid_query.with_metadata_filter(key, value);
                }
                if let Some(limit) = self.limit {
                    hybrid_query = hybrid_query.with_limit(limit);
                }
                if let Some(threshold) = self.threshold {
                    hybrid_query = hybrid_query.with_threshold(threshold);
                }
                hybrid_query = hybrid_query.with_semantic_weight(self.semantic_weight);

                Ok(CollectionQueryEnum::Hybrid(hybrid_query))
            }
        }
    }
}

// Re-export call graph filtering types for convenience
pub use call_graph_filtering::{
    CallGraphFilter, CallGraphFiltering, CallGraphPath, CallGraphSearch, CallGraphSearchResult,
    EdgeFilter, NodeFilter, PathSearch,
};
