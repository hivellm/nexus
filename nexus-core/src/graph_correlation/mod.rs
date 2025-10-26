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

use crate::{Error, Result};
use crate::vectorizer_cache::{QueryMetadata, VectorizerCache};
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
}

impl CallGraphBuilder {
    /// Create a new call graph builder
    pub fn new(name: String) -> Self {
        Self { name }
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

        // For now, create a basic data flow graph based on file relationships
        // This will be enhanced in future tasks
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
            default_builder: None,
        };

        // Register default builders
        manager.register_builder(Box::new(CallGraphBuilder::new("Call Graph".to_string())));
        manager.register_builder(Box::new(DependencyGraphBuilder::new(
            "Dependency Graph".to_string(),
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

/// Query executor for running queries via MCP
#[derive(Debug)]
pub struct QueryExecutor {
    /// MCP client for vectorizer communication
    mcp_client: Option<serde_json::Value>,
    /// Advanced cache for query results
    vectorizer_cache: VectorizerCache,
}

impl QueryExecutor {
    /// Create a new query executor
    pub fn new() -> Self {
        Self {
            mcp_client: None,
            vectorizer_cache: VectorizerCache::new(),
        }
    }

    /// Create a new query executor with custom cache configuration
    pub fn with_cache_config(config: crate::vectorizer_cache::CacheConfig) -> Self {
        Self {
            mcp_client: None,
            vectorizer_cache: VectorizerCache::with_config(config),
        }
    }

    /// Set the MCP client
    pub fn set_mcp_client(&mut self, client: serde_json::Value) {
        self.mcp_client = Some(client);
    }

    /// Get cache statistics
    pub async fn get_cache_statistics(&self) -> crate::vectorizer_cache::CacheStatistics {
        self.vectorizer_cache.get_statistics().await
    }

    /// Get cache metrics
    pub async fn get_cache_metrics(&self) -> crate::performance::cache::CacheMetrics {
        self.vectorizer_cache.get_metrics().await
    }

    /// Clear the cache
    pub async fn clear_cache(&self) -> Result<()> {
        self.vectorizer_cache.clear().await
    }

    /// Invalidate cache entries matching a pattern
    pub async fn invalidate_cache_pattern(&self, pattern: &str) -> Result<usize> {
        self.vectorizer_cache.invalidate_pattern(pattern).await
    }

    /// Execute a semantic query
    pub async fn execute_semantic_query(&self, query: &SemanticQuery) -> Result<QueryResult> {
        let cache_key = format!("semantic:{}:{}", query.collection, query.query);

        // Check cache first
        if let Some(cached_result) = self.vectorizer_cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached_result)?);
        }

        let start_time = std::time::SystemTime::now();

        // Execute the query via MCP
        let results = self
            .perform_mcp_semantic_search(
                &query.collection,
                &query.query,
                query.limit,
                query.threshold,
            )
            .await?;

        let execution_time = start_time.elapsed().unwrap_or_default();
        let execution_time_ms = execution_time.as_millis() as u64;

        let result = QueryResult::new(results.clone(), results.len(), execution_time_ms)
            .with_metadata(
                "query_type".to_string(),
                serde_json::Value::String("semantic".to_string()),
            )
            .with_metadata(
                "collection".to_string(),
                serde_json::Value::String(query.collection.clone()),
            );

        // Cache the result
        let query_metadata = QueryMetadata {
            query_type: "semantic".to_string(),
            collection: query.collection.clone(),
            query_string: query.query.clone(),
            threshold: query.threshold,
            limit: query.limit,
            filters: None,
        };

        let result_json = serde_json::to_value(&result)?;
        self.vectorizer_cache.put(
            cache_key,
            result_json,
            query_metadata,
            None,
        ).await?;

        Ok(result)
    }

    /// Execute a metadata query
    pub async fn execute_metadata_query(&self, query: &MetadataQuery) -> Result<QueryResult> {
        let cache_key = format!(
            "metadata:{}:{}",
            query.collection,
            serde_json::to_string(&query.filters).unwrap_or_default()
        );

        // Check cache first
        if let Some(cached_result) = self.vectorizer_cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached_result)?);
        }

        let start_time = std::time::SystemTime::now();

        // Execute the query via MCP
        let results = self
            .perform_mcp_metadata_search(
                &query.collection,
                &query.filters,
                query.limit,
                &query.sort_by,
                &query.sort_order,
            )
            .await?;

        let execution_time = start_time.elapsed().unwrap_or_default();
        let execution_time_ms = execution_time.as_millis() as u64;

        let result = QueryResult::new(results.clone(), results.len(), execution_time_ms)
            .with_metadata(
                "query_type".to_string(),
                serde_json::Value::String("metadata".to_string()),
            )
            .with_metadata(
                "collection".to_string(),
                serde_json::Value::String(query.collection.clone()),
            );

        // Cache the result
        let query_metadata = QueryMetadata {
            query_type: "metadata".to_string(),
            collection: query.collection.clone(),
            query_string: serde_json::to_string(&query.filters).unwrap_or_default(),
            threshold: None,
            limit: query.limit,
            filters: Some(serde_json::to_value(&query.filters)?),
        };

        let result_json = serde_json::to_value(&result)?;
        self.vectorizer_cache.put(
            cache_key,
            result_json,
            query_metadata,
            None,
        ).await?;

        Ok(result)
    }

    /// Execute a hybrid query
    pub async fn execute_hybrid_query(&self, query: &HybridQuery) -> Result<QueryResult> {
        let cache_key = format!(
            "hybrid:{}:{}:{}",
            query.collection,
            query.semantic_query,
            serde_json::to_string(&query.metadata_filters).unwrap_or_default()
        );

        // Check cache first
        if let Some(cached_result) = self.vectorizer_cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached_result)?);
        }

        let start_time = std::time::SystemTime::now();

        // Execute both semantic and metadata queries
        let semantic_results = self
            .perform_mcp_semantic_search(
                &query.collection,
                &query.semantic_query,
                query.limit,
                query.threshold,
            )
            .await?;
        let metadata_results = self
            .perform_mcp_metadata_search(
                &query.collection,
                &query.metadata_filters,
                query.limit,
                &None,
                &None,
            )
            .await?;

        // Combine results using RRF (Reciprocal Rank Fusion)
        let combined_results =
            self.combine_results(semantic_results, metadata_results, query.semantic_weight);

        let execution_time = start_time.elapsed().unwrap_or_default();
        let execution_time_ms = execution_time.as_millis() as u64;

        let result = QueryResult::new(
            combined_results.clone(),
            combined_results.len(),
            execution_time_ms,
        )
        .with_metadata(
            "query_type".to_string(),
            serde_json::Value::String("hybrid".to_string()),
        )
        .with_metadata(
            "collection".to_string(),
            serde_json::Value::String(query.collection.clone()),
        )
        .with_metadata(
            "semantic_weight".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(query.semantic_weight as f64).unwrap(),
            ),
        );

        // Cache the result
        let query_metadata = QueryMetadata {
            query_type: "hybrid".to_string(),
            collection: query.collection.clone(),
            query_string: format!("{} + {}", query.semantic_query, serde_json::to_string(&query.metadata_filters).unwrap_or_default()),
            threshold: query.threshold,
            limit: query.limit,
            filters: Some(serde_json::to_value(&query.metadata_filters)?),
        };

        let result_json = serde_json::to_value(&result)?;
        self.vectorizer_cache.put(
            cache_key,
            result_json,
            query_metadata,
            None,
        ).await?;

        Ok(result)
    }


    /// Perform MCP semantic search (placeholder implementation)
    async fn perform_mcp_semantic_search(
        &self,
        _collection: &str,
        _query: &str,
        _limit: Option<usize>,
        _threshold: Option<f32>,
    ) -> Result<Vec<serde_json::Value>> {
        // This is a placeholder implementation
        // In a real implementation, this would use the MCP client to call vectorizer tools
        // For now, return empty results
        Ok(vec![])
    }

    /// Perform MCP metadata search (placeholder implementation)
    async fn perform_mcp_metadata_search(
        &self,
        _collection: &str,
        _filters: &HashMap<String, serde_json::Value>,
        _limit: Option<usize>,
        _sort_by: &Option<String>,
        _sort_order: &Option<SortOrder>,
    ) -> Result<Vec<serde_json::Value>> {
        // This is a placeholder implementation
        // In a real implementation, this would use the MCP client to call vectorizer tools
        // For now, return empty results
        Ok(vec![])
    }

    /// Combine semantic and metadata results using RRF
    fn combine_results(
        &self,
        semantic_results: Vec<serde_json::Value>,
        metadata_results: Vec<serde_json::Value>,
        _semantic_weight: f32,
    ) -> Vec<serde_json::Value> {
        // Simple RRF implementation
        // In a real implementation, this would use proper RRF scoring
        let mut combined = semantic_results;
        combined.extend(metadata_results);

        // Remove duplicates based on ID field
        let mut seen_ids = std::collections::HashSet::new();
        combined.retain(|item| {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                seen_ids.insert(id.to_string())
            } else {
                true
            }
        });

        // Limit results if needed
        if combined.len() > 1000 {
            combined.truncate(1000);
        }

        combined
    }
}

impl Default for QueryExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_graph_creation() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());
        assert_eq!(graph.graph_type, GraphType::Call);
        assert_eq!(graph.name, "Test Graph");
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_add_node() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        let node = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "test_function".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        assert!(graph.add_node(node).is_ok());
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "node1");
    }

    #[test]
    fn test_add_edge() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add nodes first
        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "function1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let node2 = GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Function,
            label: "function2".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();

        // Add edge
        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };

        assert!(graph.add_edge(edge).is_ok());
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].id, "edge1");
    }

    #[test]
    fn test_duplicate_node_id() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "function1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let node2 = GraphNode {
            id: "node1".to_string(), // Same ID
            node_type: NodeType::Function,
            label: "function2".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node1).unwrap();
        assert!(graph.add_node(node2).is_err());
    }

    #[test]
    fn test_edge_with_nonexistent_node() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "nonexistent".to_string(),
            target: "also_nonexistent".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };

        assert!(graph.add_edge(edge).is_err());
    }

    #[test]
    fn test_graph_statistics() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add nodes
        for i in 0..3 {
            let node = GraphNode {
                id: format!("node{}", i),
                node_type: NodeType::Function,
                label: format!("function{}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            };
            graph.add_node(node).unwrap();
        }

        // Add edges
        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "node0".to_string(),
            target: "node1".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };
        graph.add_edge(edge).unwrap();

        let stats = graph.statistics();
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 1);
        assert_eq!(stats.avg_degree, 1.0 / 3.0);
    }

    #[test]
    fn test_call_graph_builder() {
        let builder = CallGraphBuilder::new("Test Call Graph".to_string());
        let mut source_data = GraphSourceData::new();

        source_data.add_file("test.rs".to_string(), "fn test() {}".to_string());
        source_data.add_functions("test.rs".to_string(), vec!["test".to_string()]);

        let graph = builder.build(&source_data).unwrap();
        assert_eq!(graph.graph_type, GraphType::Call);
        assert_eq!(graph.nodes.len(), 2); // 1 file + 1 function
        assert_eq!(graph.edges.len(), 1); // 1 edge from file to function
    }

    #[test]
    fn test_dependency_graph_builder() {
        let builder = DependencyGraphBuilder::new("Test Dependency Graph".to_string());
        let mut source_data = GraphSourceData::new();

        source_data.add_file("main.rs".to_string(), "use module;".to_string());
        source_data.add_file("module.rs".to_string(), "pub fn func() {}".to_string());
        source_data.add_imports("main.rs".to_string(), vec!["module.rs".to_string()]);

        let graph = builder.build(&source_data).unwrap();
        assert_eq!(graph.graph_type, GraphType::Dependency);
        assert_eq!(graph.nodes.len(), 2); // 2 files
        assert_eq!(graph.edges.len(), 1); // 1 import edge
    }

    #[test]
    fn test_graph_correlation_manager() {
        let manager = GraphCorrelationManager::new();
        let available_types = manager.available_graph_types();

        assert!(available_types.contains(&GraphType::Call));
        assert!(available_types.contains(&GraphType::Dependency));
    }

    #[test]
    fn test_json_export() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        let node = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "test_function".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();

        let json = graph.to_json().unwrap();
        assert!(json.contains("node1"));
        assert!(json.contains("test_function"));
    }

    #[test]
    fn test_graphml_export() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        let node = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "test_function".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();

        let graphml = graph.to_graphml().unwrap();
        assert!(graphml.contains("<?xml"));
        assert!(graphml.contains("node1"));
        assert!(graphml.contains("test_function"));
    }

    #[test]
    fn test_node_type_enum_variants() {
        // Test all NodeType variants exist
        let function = NodeType::Function;
        let module = NodeType::Module;
        let class = NodeType::Class;
        let variable = NodeType::Variable;
        let api = NodeType::API;

        // Test debug formatting
        assert_eq!(format!("{:?}", function), "Function");
        assert_eq!(format!("{:?}", module), "Module");
        assert_eq!(format!("{:?}", class), "Class");
        assert_eq!(format!("{:?}", variable), "Variable");
        assert_eq!(format!("{:?}", api), "API");
    }

    #[test]
    fn test_node_type_equality() {
        // Test equality between same variants
        assert_eq!(NodeType::Function, NodeType::Function);
        assert_eq!(NodeType::Module, NodeType::Module);
        assert_eq!(NodeType::Class, NodeType::Class);
        assert_eq!(NodeType::Variable, NodeType::Variable);
        assert_eq!(NodeType::API, NodeType::API);

        // Test inequality between different variants
        assert_ne!(NodeType::Function, NodeType::Module);
        assert_ne!(NodeType::Module, NodeType::Class);
        assert_ne!(NodeType::Class, NodeType::Variable);
        assert_ne!(NodeType::Variable, NodeType::API);
        assert_ne!(NodeType::API, NodeType::Function);
    }

    #[test]
    fn test_node_type_clone() {
        let original = NodeType::Function;
        let cloned = original; // NodeType implements Copy, so no need for clone()
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_node_type_copy() {
        let original = NodeType::Module;
        let copied = original; // This should work because NodeType implements Copy
        assert_eq!(original, copied);
        assert_eq!(original, NodeType::Module); // original should still be valid
    }

    #[test]
    fn test_node_type_serialization() {
        let node_types = vec![
            NodeType::Function,
            NodeType::Module,
            NodeType::Class,
            NodeType::Variable,
            NodeType::API,
        ];

        for node_type in node_types {
            // Test JSON serialization
            let json = serde_json::to_string(&node_type).unwrap();
            let deserialized: NodeType = serde_json::from_str(&json).unwrap();
            assert_eq!(node_type, deserialized);

            // Test that serialized JSON contains expected strings
            match node_type {
                NodeType::Function => assert!(json.contains("Function")),
                NodeType::Module => assert!(json.contains("Module")),
                NodeType::Class => assert!(json.contains("Class")),
                NodeType::Variable => assert!(json.contains("Variable")),
                NodeType::API => assert!(json.contains("API")),
            }
        }
    }

    #[test]
    fn test_node_type_deserialization() {
        // Test deserialization from JSON strings
        let test_cases = vec![
            ("Function", NodeType::Function),
            ("Module", NodeType::Module),
            ("Class", NodeType::Class),
            ("Variable", NodeType::Variable),
            ("API", NodeType::API),
        ];

        for (json_str, expected) in test_cases {
            let deserialized: NodeType =
                serde_json::from_str(&format!("\"{}\"", json_str)).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_node_type_in_graph_node() {
        // Test NodeType usage in GraphNode
        let node = GraphNode {
            id: "test_node".to_string(),
            node_type: NodeType::Function,
            label: "test_function".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        assert_eq!(node.node_type, NodeType::Function);
        assert_eq!(node.label, "test_function");
    }

    #[test]
    fn test_node_type_pattern_matching() {
        let node_type = NodeType::API;

        let description = match node_type {
            NodeType::Function => "A function or method",
            NodeType::Module => "A module or file",
            NodeType::Class => "A class or struct",
            NodeType::Variable => "A variable or parameter",
            NodeType::API => "An API endpoint or service",
        };

        assert_eq!(description, "An API endpoint or service");
    }

    #[test]
    fn test_node_type_all_variants() {
        // Test that we can iterate through all variants
        let all_variants = [
            NodeType::Function,
            NodeType::Module,
            NodeType::Class,
            NodeType::Variable,
            NodeType::API,
        ];

        assert_eq!(all_variants.len(), 5);

        // Test that all variants are unique
        for (i, variant1) in all_variants.iter().enumerate() {
            for (j, variant2) in all_variants.iter().enumerate() {
                if i != j {
                    assert_ne!(variant1, variant2);
                }
            }
        }
    }

    #[test]
    fn test_edge_type_enum_variants() {
        // Test all EdgeType variants exist
        let calls = EdgeType::Calls;
        let imports = EdgeType::Imports;
        let inherits = EdgeType::Inherits;
        let composes = EdgeType::Composes;
        let transforms = EdgeType::Transforms;
        let uses = EdgeType::Uses;
        let depends = EdgeType::Depends;

        // Test debug formatting
        assert_eq!(format!("{:?}", calls), "Calls");
        assert_eq!(format!("{:?}", imports), "Imports");
        assert_eq!(format!("{:?}", inherits), "Inherits");
        assert_eq!(format!("{:?}", composes), "Composes");
        assert_eq!(format!("{:?}", transforms), "Transforms");
        assert_eq!(format!("{:?}", uses), "Uses");
        assert_eq!(format!("{:?}", depends), "Depends");
    }

    #[test]
    fn test_edge_type_equality() {
        // Test equality between same variants
        assert_eq!(EdgeType::Calls, EdgeType::Calls);
        assert_eq!(EdgeType::Imports, EdgeType::Imports);
        assert_eq!(EdgeType::Inherits, EdgeType::Inherits);
        assert_eq!(EdgeType::Composes, EdgeType::Composes);
        assert_eq!(EdgeType::Transforms, EdgeType::Transforms);
        assert_eq!(EdgeType::Uses, EdgeType::Uses);
        assert_eq!(EdgeType::Depends, EdgeType::Depends);

        // Test inequality between different variants
        assert_ne!(EdgeType::Calls, EdgeType::Imports);
        assert_ne!(EdgeType::Imports, EdgeType::Inherits);
        assert_ne!(EdgeType::Inherits, EdgeType::Composes);
        assert_ne!(EdgeType::Composes, EdgeType::Transforms);
        assert_ne!(EdgeType::Transforms, EdgeType::Uses);
        assert_ne!(EdgeType::Uses, EdgeType::Depends);
        assert_ne!(EdgeType::Depends, EdgeType::Calls);
    }

    #[test]
    fn test_edge_type_clone() {
        let original = EdgeType::Calls;
        let cloned = original; // EdgeType implements Copy, so no need for clone()
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_edge_type_copy() {
        let original = EdgeType::Imports;
        let copied = original; // This should work because EdgeType implements Copy
        assert_eq!(original, copied);
        assert_eq!(original, EdgeType::Imports); // original should still be valid
    }

    #[test]
    fn test_edge_type_serialization() {
        let edge_types = vec![
            EdgeType::Calls,
            EdgeType::Imports,
            EdgeType::Inherits,
            EdgeType::Composes,
            EdgeType::Transforms,
            EdgeType::Uses,
            EdgeType::Depends,
        ];

        for edge_type in edge_types {
            // Test JSON serialization
            let json = serde_json::to_string(&edge_type).unwrap();
            let deserialized: EdgeType = serde_json::from_str(&json).unwrap();
            assert_eq!(edge_type, deserialized);

            // Test that serialized JSON contains expected strings
            match edge_type {
                EdgeType::Calls => assert!(json.contains("Calls")),
                EdgeType::Imports => assert!(json.contains("Imports")),
                EdgeType::Inherits => assert!(json.contains("Inherits")),
                EdgeType::Composes => assert!(json.contains("Composes")),
                EdgeType::Transforms => assert!(json.contains("Transforms")),
                EdgeType::Uses => assert!(json.contains("Uses")),
                EdgeType::Depends => assert!(json.contains("Depends")),
            }
        }
    }

    #[test]
    fn test_edge_type_deserialization() {
        // Test deserialization from JSON strings
        let test_cases = vec![
            ("Calls", EdgeType::Calls),
            ("Imports", EdgeType::Imports),
            ("Inherits", EdgeType::Inherits),
            ("Composes", EdgeType::Composes),
            ("Transforms", EdgeType::Transforms),
            ("Uses", EdgeType::Uses),
            ("Depends", EdgeType::Depends),
        ];

        for (json_str, expected) in test_cases {
            let deserialized: EdgeType =
                serde_json::from_str(&format!("\"{}\"", json_str)).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_edge_type_in_graph_edge() {
        // Test EdgeType usage in GraphEdge
        let edge = GraphEdge {
            id: "test_edge".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };

        assert_eq!(edge.edge_type, EdgeType::Calls);
        assert_eq!(edge.id, "test_edge");
    }

    #[test]
    fn test_edge_type_pattern_matching() {
        let edge_type = EdgeType::Transforms;

        let description = match edge_type {
            EdgeType::Calls => "Function calls another function",
            EdgeType::Imports => "Module imports another module",
            EdgeType::Inherits => "Class inherits from another class",
            EdgeType::Composes => "Component composes another component",
            EdgeType::Transforms => "Data transforms from one format to another",
            EdgeType::Uses => "Uses or references another entity",
            EdgeType::Depends => "Depends on another entity",
        };

        assert_eq!(description, "Data transforms from one format to another");
    }

    #[test]
    fn test_edge_type_all_variants() {
        // Test that we can iterate through all variants
        let all_variants = [
            EdgeType::Calls,
            EdgeType::Imports,
            EdgeType::Inherits,
            EdgeType::Composes,
            EdgeType::Transforms,
            EdgeType::Uses,
            EdgeType::Depends,
        ];

        assert_eq!(all_variants.len(), 7);

        // Test that all variants are unique
        for (i, variant1) in all_variants.iter().enumerate() {
            for (j, variant2) in all_variants.iter().enumerate() {
                if i != j {
                    assert_ne!(variant1, variant2);
                }
            }
        }
    }

    #[test]
    fn test_graph_type_variants() {
        // Test all GraphType variants
        assert_eq!(GraphType::Call, GraphType::Call);
        assert_eq!(GraphType::Dependency, GraphType::Dependency);
        assert_eq!(GraphType::DataFlow, GraphType::DataFlow);
        assert_eq!(GraphType::Component, GraphType::Component);

        assert_ne!(GraphType::Call, GraphType::Dependency);
        assert_ne!(GraphType::Call, GraphType::DataFlow);
        assert_ne!(GraphType::Call, GraphType::Component);
        assert_ne!(GraphType::Dependency, GraphType::DataFlow);
        assert_ne!(GraphType::Dependency, GraphType::Component);
        assert_ne!(GraphType::DataFlow, GraphType::Component);

        // Test serialization
        let call_json = serde_json::to_string(&GraphType::Call).unwrap();
        assert!(call_json.contains("Call"));

        let dep_json = serde_json::to_string(&GraphType::Dependency).unwrap();
        assert!(dep_json.contains("Dependency"));

        let flow_json = serde_json::to_string(&GraphType::DataFlow).unwrap();
        assert!(flow_json.contains("DataFlow"));

        let comp_json = serde_json::to_string(&GraphType::Component).unwrap();
        assert!(comp_json.contains("Component"));
    }

    #[test]
    fn test_graph_node_creation() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "file".to_string(),
            serde_json::Value::String("test.rs".to_string()),
        );
        metadata.insert("line".to_string(), serde_json::Value::Number(42.into()));

        let node = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "test_function".to_string(),
            metadata: metadata.clone(),
            position: Some((10.0, 20.0)),
            size: Some(5.0),
            color: Some("#FF0000".to_string()),
        };

        assert_eq!(node.id, "node1");
        assert_eq!(node.node_type, NodeType::Function);
        assert_eq!(node.label, "test_function");
        assert_eq!(node.metadata.len(), 2);
        assert_eq!(node.position, Some((10.0, 20.0)));
        assert_eq!(node.size, Some(5.0));
        assert_eq!(node.color, Some("#FF0000".to_string()));

        // Test clone
        let cloned = node.clone();
        assert_eq!(node.id, cloned.id);
        assert_eq!(node.node_type, cloned.node_type);
        assert_eq!(node.label, cloned.label);
        assert_eq!(node.metadata, cloned.metadata);
        assert_eq!(node.position, cloned.position);
        assert_eq!(node.size, cloned.size);
        assert_eq!(node.color, cloned.color);
    }

    #[test]
    fn test_graph_edge_creation() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "weight".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(1.5).unwrap()),
        );
        metadata.insert(
            "frequency".to_string(),
            serde_json::Value::Number(10.into()),
        );

        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            metadata: metadata.clone(),
            weight: 1.5,
            label: Some("#00FF00".to_string()),
        };

        assert_eq!(edge.id, "edge1");
        assert_eq!(edge.source, "node1");
        assert_eq!(edge.target, "node2");
        assert_eq!(edge.edge_type, EdgeType::Calls);
        assert_eq!(edge.metadata.len(), 2);
        assert_eq!(edge.weight, 1.5);
        assert_eq!(edge.label, Some("#00FF00".to_string()));

        // Test clone
        let cloned = edge.clone();
        assert_eq!(edge.id, cloned.id);
        assert_eq!(edge.source, cloned.source);
        assert_eq!(edge.target, cloned.target);
        assert_eq!(edge.edge_type, cloned.edge_type);
        assert_eq!(edge.metadata, cloned.metadata);
        assert_eq!(edge.weight, cloned.weight);
        assert_eq!(edge.label, cloned.label);
    }

    #[test]
    fn test_graph_statistics_calculation() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add nodes
        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function 1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node2 = GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Function,
            label: "Function 2".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node3 = GraphNode {
            id: "node3".to_string(),
            node_type: NodeType::Module,
            label: "Module 1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();
        graph.add_node(node3).unwrap();

        // Add edges
        let edge1 = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        };
        let edge2 = GraphEdge {
            id: "edge2".to_string(),
            source: "node2".to_string(),
            target: "node3".to_string(),
            edge_type: EdgeType::Imports,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        };

        graph.add_edge(edge1).unwrap();
        graph.add_edge(edge2).unwrap();

        let stats = graph.statistics();
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 2);
    }

    #[test]
    fn test_graph_serialization() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add test data
        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function 1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node2 = GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Function,
            label: "Function 2".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let edge1 = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();
        graph.add_edge(edge1).unwrap();

        // Test JSON serialization
        let json = graph.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.is_object());
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("edges").is_some());

        let nodes = parsed.get("nodes").unwrap().as_array().unwrap();
        let edges = parsed.get("edges").unwrap().as_array().unwrap();

        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_graph_node_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "complexity".to_string(),
            serde_json::Value::Number(5.into()),
        );
        metadata.insert("lines".to_string(), serde_json::Value::Number(100.into()));
        metadata.insert("is_public".to_string(), serde_json::Value::Bool(true));

        let node = GraphNode {
            id: "complex_node".to_string(),
            node_type: NodeType::Function,
            label: "complex_function".to_string(),
            metadata,
            position: None,
            size: None,
            color: None,
        };

        assert_eq!(
            node.metadata.get("complexity").unwrap().as_i64().unwrap(),
            5
        );
        assert_eq!(node.metadata.get("lines").unwrap().as_i64().unwrap(), 100);
        assert!(node.metadata.get("is_public").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_graph_edge_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "call_count".to_string(),
            serde_json::Value::Number(50.into()),
        );
        metadata.insert(
            "avg_duration".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(0.5).unwrap()),
        );
        metadata.insert("is_async".to_string(), serde_json::Value::Bool(false));

        let edge = GraphEdge {
            id: "frequent_call".to_string(),
            source: "caller".to_string(),
            target: "callee".to_string(),
            edge_type: EdgeType::Calls,
            metadata,
            weight: 50.0,
            label: None,
        };

        assert_eq!(
            edge.metadata.get("call_count").unwrap().as_i64().unwrap(),
            50
        );
        assert_eq!(
            edge.metadata.get("avg_duration").unwrap().as_f64().unwrap(),
            0.5
        );
        assert!(!edge.metadata.get("is_async").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_graph_manager_operations() {
        let manager = GraphCorrelationManager::new();

        // Test available graph types
        let graph_types = manager.available_graph_types();
        assert!(!graph_types.is_empty());
        assert!(graph_types.contains(&GraphType::Call));
        assert!(graph_types.contains(&GraphType::Dependency));

        // Test building graphs
        let source_data = GraphSourceData::new();
        let call_graph = manager.build_graph(GraphType::Call, &source_data);
        assert!(call_graph.is_ok());

        let dep_graph = manager.build_graph(GraphType::Dependency, &source_data);
        assert!(dep_graph.is_ok());
    }

    #[test]
    fn test_graph_visualization_properties() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add nodes with visualization properties
        let mut node1_metadata = HashMap::new();
        node1_metadata.insert(
            "importance".to_string(),
            serde_json::Value::String("high".to_string()),
        );

        let node = GraphNode {
            id: "important_func".to_string(),
            node_type: NodeType::Function,
            label: "Important Function".to_string(),
            metadata: node1_metadata,
            position: Some((10.0, 20.0)),
            size: Some(5.0),
            color: Some("#FF0000".to_string()),
        };
        graph.add_node(node).unwrap();

        // Add edge with weight
        let mut edge_metadata = HashMap::new();
        edge_metadata.insert(
            "frequency".to_string(),
            serde_json::Value::Number(100.into()),
        );

        // Add the target node first
        let other_node = GraphNode {
            id: "other_func".to_string(),
            node_type: NodeType::Function,
            label: "Other Function".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(other_node).unwrap();

        let edge = GraphEdge {
            id: "frequent_call".to_string(),
            source: "important_func".to_string(),
            target: "other_func".to_string(),
            edge_type: EdgeType::Calls,
            metadata: edge_metadata,
            weight: 1.0,
            label: Some("frequent".to_string()),
        };
        graph.add_edge(edge).unwrap();

        // Test that visualization properties are preserved
        let important_node = graph.get_node("important_func").unwrap();
        assert_eq!(important_node.label, "Important Function");
        assert_eq!(important_node.node_type, NodeType::Function);

        let frequent_edge = graph.get_edge("frequent_call").unwrap();
        assert_eq!(frequent_edge.edge_type, EdgeType::Calls);
        assert_eq!(frequent_edge.source, "important_func");
        assert_eq!(frequent_edge.target, "other_func");
    }

    #[test]
    fn test_graph_error_handling() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Test adding duplicate node
        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function 1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node1).unwrap();

        let node1_dup = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function 1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let result = graph.add_node(node1_dup);
        assert!(result.is_err());

        // Test adding edge with non-existent nodes
        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "nonexistent1".to_string(),
            target: "nonexistent2".to_string(),
            edge_type: EdgeType::Calls,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        };
        let result = graph.add_edge(edge);
        assert!(result.is_err());
    }

    #[test]
    fn test_graph_clear_operations() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add some data
        let node1 = GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function 1".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node2 = GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Function,
            label: "Function 2".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();

        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        };
        graph.add_edge(edge).unwrap();

        // Verify data exists
        let stats = graph.statistics();
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.edge_count, 1);

        // Clear and verify
        graph.nodes.clear();
        graph.edges.clear();
        let stats_after_clear = graph.statistics();
        assert_eq!(stats_after_clear.node_count, 0);
        assert_eq!(stats_after_clear.edge_count, 0);

        // Should be able to add new data after clear
        let node3 = GraphNode {
            id: "node3".to_string(),
            node_type: NodeType::Function,
            label: "Function 3".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node3).unwrap();
        let stats_final = graph.statistics();
        assert_eq!(stats_final.node_count, 1);
    }

    // VectorizerGraphExtractor tests
    #[test]
    fn test_vectorizer_graph_extractor_new() {
        let extractor = VectorizerGraphExtractor::new();
        assert!(extractor.mcp_client.is_none());
        assert!(extractor.query_cache.is_empty());
        assert_eq!(extractor.config.max_results, 1000);
        assert_eq!(extractor.config.similarity_threshold, 0.7);
        assert!(extractor.config.enable_caching);
    }

    #[test]
    fn test_vectorizer_graph_extractor_with_config() {
        let config = VectorizerExtractorConfig {
            max_results: 500,
            similarity_threshold: 0.8,
            enable_caching: false,
            cache_ttl_seconds: 1800,
            collections: VectorizerCollections {
                functions: "custom_functions".to_string(),
                imports: "custom_imports".to_string(),
                calls: "custom_calls".to_string(),
                types: "custom_types".to_string(),
                codebase: "custom_codebase".to_string(),
            },
        };

        let extractor = VectorizerGraphExtractor::with_config(config.clone());
        assert_eq!(extractor.config.max_results, 500);
        assert_eq!(extractor.config.similarity_threshold, 0.8);
        assert!(!extractor.config.enable_caching);
        assert_eq!(extractor.config.cache_ttl_seconds, 1800);
        assert_eq!(extractor.config.collections.functions, "custom_functions");
    }

    #[test]
    fn test_vectorizer_extractor_config_default() {
        let config = VectorizerExtractorConfig::default();
        assert_eq!(config.max_results, 1000);
        assert_eq!(config.similarity_threshold, 0.7);
        assert!(config.enable_caching);
        assert_eq!(config.cache_ttl_seconds, 3600);
        assert_eq!(config.collections.functions, "functions");
        assert_eq!(config.collections.imports, "imports");
        assert_eq!(config.collections.calls, "calls");
        assert_eq!(config.collections.types, "types");
        assert_eq!(config.collections.codebase, "codebase");
    }

    #[test]
    fn test_vectorizer_collections_default() {
        let collections = VectorizerCollections::default();
        assert_eq!(collections.functions, "functions");
        assert_eq!(collections.imports, "imports");
        assert_eq!(collections.calls, "calls");
        assert_eq!(collections.types, "types");
        assert_eq!(collections.codebase, "codebase");
    }

    #[test]
    fn test_set_mcp_client() {
        let mut extractor = VectorizerGraphExtractor::new();
        assert!(extractor.mcp_client.is_none());

        let client = serde_json::json!({"test": "client"});
        extractor.set_mcp_client(client.clone());
        assert!(extractor.mcp_client.is_some());
        assert_eq!(extractor.mcp_client.unwrap(), client);
    }

    #[test]
    fn test_create_function_node() {
        let extractor = VectorizerGraphExtractor::new();

        let func_data = serde_json::json!({
            "id": "func_123",
            "name": "test_function",
            "signature": "fn test_function() -> i32",
            "file": "src/main.rs"
        });

        let node = extractor.create_function_node(func_data).unwrap();
        assert_eq!(node.id, "func_123");
        assert_eq!(node.label, "test_function");
        assert_eq!(node.node_type, NodeType::Function);
        assert_eq!(
            node.metadata.get("signature").unwrap().as_str().unwrap(),
            "fn test_function() -> i32"
        );
        assert_eq!(
            node.metadata.get("file").unwrap().as_str().unwrap(),
            "src/main.rs"
        );
    }

    #[test]
    fn test_create_function_node_minimal() {
        let extractor = VectorizerGraphExtractor::new();

        let func_data = serde_json::json!({});

        let node = extractor.create_function_node(func_data).unwrap();
        assert_eq!(node.id, "unknown");
        assert_eq!(node.label, "Unknown Function");
        assert_eq!(node.node_type, NodeType::Function);
        assert!(node.metadata.is_empty());
    }

    #[test]
    fn test_create_call_edge() {
        let extractor = VectorizerGraphExtractor::new();

        let call_data = serde_json::json!({
            "id": "call_123",
            "caller": "function_a",
            "callee": "function_b",
            "frequency": 42
        });

        let edge = extractor.create_call_edge(call_data).unwrap();
        assert_eq!(edge.id, "call_123");
        assert_eq!(edge.source, "function_a");
        assert_eq!(edge.target, "function_b");
        assert_eq!(edge.edge_type, EdgeType::Calls);
        assert_eq!(edge.weight, 1.0);
        assert_eq!(
            edge.metadata.get("frequency").unwrap().as_number().unwrap(),
            &serde_json::Number::from(42)
        );
    }

    #[test]
    fn test_create_call_edge_minimal() {
        let extractor = VectorizerGraphExtractor::new();

        let call_data = serde_json::json!({});

        let edge = extractor.create_call_edge(call_data).unwrap();
        assert_eq!(edge.id, "unknown");
        assert_eq!(edge.source, "unknown");
        assert_eq!(edge.target, "unknown");
        assert_eq!(edge.edge_type, EdgeType::Calls);
        assert_eq!(edge.weight, 1.0);
        assert!(edge.metadata.is_empty());
    }

    #[test]
    fn test_create_import_relationship() {
        let extractor = VectorizerGraphExtractor::new();

        let import_data = serde_json::json!({
            "source": "module_a",
            "target": "module_b"
        });

        let (source_node, target_node, edge) =
            extractor.create_import_relationship(import_data).unwrap();

        assert_eq!(source_node.id, "module_a");
        assert_eq!(source_node.label, "module_a");
        assert_eq!(source_node.node_type, NodeType::Module);

        assert_eq!(target_node.id, "module_b");
        assert_eq!(target_node.label, "module_b");
        assert_eq!(target_node.node_type, NodeType::Module);

        assert_eq!(edge.id, "module_a->module_b");
        assert_eq!(edge.source, "module_a");
        assert_eq!(edge.target, "module_b");
        assert_eq!(edge.edge_type, EdgeType::Imports);
    }

    #[test]
    fn test_create_variable_node() {
        let extractor = VectorizerGraphExtractor::new();

        let var_data = serde_json::json!({
            "id": "var_123",
            "name": "counter",
            "type": "i32"
        });

        let node = extractor.create_variable_node(var_data).unwrap();
        assert_eq!(node.id, "var_123");
        assert_eq!(node.label, "counter");
        assert_eq!(node.node_type, NodeType::Variable);
        assert_eq!(node.metadata.get("type").unwrap().as_str().unwrap(), "i32");
    }

    #[test]
    fn test_create_transformation_edge() {
        let extractor = VectorizerGraphExtractor::new();

        let transform_data = serde_json::json!({
            "id": "transform_123",
            "source": "input_data",
            "target": "output_data"
        });

        let edge = extractor
            .create_transformation_edge(transform_data)
            .unwrap();
        assert_eq!(edge.id, "transform_123");
        assert_eq!(edge.source, "input_data");
        assert_eq!(edge.target, "output_data");
        assert_eq!(edge.edge_type, EdgeType::Transforms);
    }

    #[test]
    fn test_create_class_node() {
        let extractor = VectorizerGraphExtractor::new();

        let class_data = serde_json::json!({
            "id": "class_123",
            "name": "MyClass",
            "base_class": "BaseClass"
        });

        let node = extractor.create_class_node(class_data).unwrap();
        assert_eq!(node.id, "class_123");
        assert_eq!(node.label, "MyClass");
        assert_eq!(node.node_type, NodeType::Class);
        assert_eq!(
            node.metadata.get("base_class").unwrap().as_str().unwrap(),
            "BaseClass"
        );
    }

    #[test]
    fn test_create_interface_node() {
        let extractor = VectorizerGraphExtractor::new();

        let interface_data = serde_json::json!({
            "id": "interface_123",
            "name": "MyInterface"
        });

        let node = extractor.create_interface_node(interface_data).unwrap();
        assert_eq!(node.id, "interface_123");
        assert_eq!(node.label, "MyInterface");
        assert_eq!(node.node_type, NodeType::API);
    }

    #[test]
    fn test_create_relationship_edge() {
        let extractor = VectorizerGraphExtractor::new();

        // Test inheritance
        let inherits_data = serde_json::json!({
            "id": "rel_123",
            "source": "ChildClass",
            "target": "ParentClass",
            "type": "inherits"
        });

        let edge = extractor.create_relationship_edge(inherits_data).unwrap();
        assert_eq!(edge.edge_type, EdgeType::Inherits);

        // Test implementation
        let implements_data = serde_json::json!({
            "id": "rel_456",
            "source": "MyClass",
            "target": "MyInterface",
            "type": "implements"
        });

        let edge = extractor.create_relationship_edge(implements_data).unwrap();
        assert_eq!(edge.edge_type, EdgeType::Composes);

        // Test uses
        let uses_data = serde_json::json!({
            "id": "rel_789",
            "source": "MyClass",
            "target": "OtherClass",
            "type": "uses"
        });

        let edge = extractor.create_relationship_edge(uses_data).unwrap();
        assert_eq!(edge.edge_type, EdgeType::Uses);

        // Test default (depends)
        let depends_data = serde_json::json!({
            "id": "rel_999",
            "source": "MyClass",
            "target": "OtherClass",
            "type": "unknown"
        });

        let edge = extractor.create_relationship_edge(depends_data).unwrap();
        assert_eq!(edge.edge_type, EdgeType::Depends);
    }

    #[test]
    fn test_cache_operations() {
        let mut extractor = VectorizerGraphExtractor::new();

        // Test initial cache stats
        let (cache_entries, total_items) = extractor.cache_stats();
        assert_eq!(cache_entries, 0);
        assert_eq!(total_items, 0);

        // Test cache clear
        extractor.clear_cache();
        let (cache_entries, total_items) = extractor.cache_stats();
        assert_eq!(cache_entries, 0);
        assert_eq!(total_items, 0);
    }

    #[test]
    fn test_vectorizer_graph_extractor_default() {
        let extractor = VectorizerGraphExtractor::default();
        assert!(extractor.mcp_client.is_none());
        assert!(extractor.query_cache.is_empty());
        assert_eq!(extractor.config.max_results, 1000);
    }

    #[tokio::test]
    async fn test_extract_call_graph_empty() {
        let mut extractor = VectorizerGraphExtractor::new();

        // Test with empty query (should return empty graph)
        let graph = extractor.extract_call_graph("", None).await.unwrap();
        assert_eq!(graph.graph_type, GraphType::Call);
        assert_eq!(graph.name, "Call Graph");
        assert_eq!(graph.statistics().node_count, 0);
        assert_eq!(graph.statistics().edge_count, 0);
    }

    #[tokio::test]
    async fn test_extract_dependency_graph_empty() {
        let mut extractor = VectorizerGraphExtractor::new();

        // Test with empty query (should return empty graph)
        let graph = extractor.extract_dependency_graph("").await.unwrap();
        assert_eq!(graph.graph_type, GraphType::Dependency);
        assert_eq!(graph.name, "Dependency Graph");
        assert_eq!(graph.statistics().node_count, 0);
        assert_eq!(graph.statistics().edge_count, 0);
    }

    #[tokio::test]
    async fn test_extract_data_flow_graph_empty() {
        let mut extractor = VectorizerGraphExtractor::new();

        // Test with empty query (should return empty graph)
        let graph = extractor.extract_data_flow_graph("").await.unwrap();
        assert_eq!(graph.graph_type, GraphType::DataFlow);
        assert_eq!(graph.name, "Data Flow Graph");
        assert_eq!(graph.statistics().node_count, 0);
        assert_eq!(graph.statistics().edge_count, 0);
    }

    #[tokio::test]
    async fn test_extract_component_graph_empty() {
        let mut extractor = VectorizerGraphExtractor::new();

        // Test with empty query (should return empty graph)
        let graph = extractor.extract_component_graph("").await.unwrap();
        assert_eq!(graph.graph_type, GraphType::Component);
        assert_eq!(graph.name, "Component Graph");
        assert_eq!(graph.statistics().node_count, 0);
        assert_eq!(graph.statistics().edge_count, 0);
    }

    #[test]
    fn test_vectorizer_extractor_config_serialization() {
        let config = VectorizerExtractorConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VectorizerExtractorConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.max_results, deserialized.max_results);
        assert_eq!(
            config.similarity_threshold,
            deserialized.similarity_threshold
        );
        assert_eq!(config.enable_caching, deserialized.enable_caching);
        assert_eq!(config.cache_ttl_seconds, deserialized.cache_ttl_seconds);
        assert_eq!(
            config.collections.functions,
            deserialized.collections.functions
        );
    }

    #[test]
    fn test_vectorizer_collections_serialization() {
        let collections = VectorizerCollections::default();
        let json = serde_json::to_string(&collections).unwrap();
        let deserialized: VectorizerCollections = serde_json::from_str(&json).unwrap();

        assert_eq!(collections.functions, deserialized.functions);
        assert_eq!(collections.imports, deserialized.imports);
        assert_eq!(collections.calls, deserialized.calls);
        assert_eq!(collections.types, deserialized.types);
        assert_eq!(collections.codebase, deserialized.codebase);
    }
}

/// VectorizerGraphExtractor for data access and graph generation
///
/// This struct provides integration with the Vectorizer MCP server to extract
/// code relationships and generate correlation graphs from vectorized data.
#[derive(Debug, Clone)]
pub struct VectorizerGraphExtractor {
    /// MCP client for vectorizer communication
    mcp_client: Option<serde_json::Value>,
    /// Cache for vectorizer queries to improve performance
    query_cache: std::collections::HashMap<String, serde_json::Value>,
    /// Configuration for graph extraction
    config: VectorizerExtractorConfig,
}

/// Configuration for the VectorizerGraphExtractor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizerExtractorConfig {
    /// Maximum number of results per query
    pub max_results: usize,
    /// Similarity threshold for semantic search (0.0 to 1.0)
    pub similarity_threshold: f32,
    /// Enable caching of vectorizer queries
    pub enable_caching: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Collections to search for different graph types
    pub collections: VectorizerCollections,
}

/// Vectorizer collections configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizerCollections {
    /// Collection for function definitions
    pub functions: String,
    /// Collection for import/export relationships
    pub imports: String,
    /// Collection for function calls
    pub calls: String,
    /// Collection for type definitions
    pub types: String,
    /// Collection for general codebase data
    pub codebase: String,
}

impl Default for VectorizerExtractorConfig {
    fn default() -> Self {
        Self {
            max_results: 1000,
            similarity_threshold: 0.7,
            enable_caching: true,
            cache_ttl_seconds: 3600, // 1 hour
            collections: VectorizerCollections::default(),
        }
    }
}

impl Default for VectorizerCollections {
    fn default() -> Self {
        Self {
            functions: "functions".to_string(),
            imports: "imports".to_string(),
            calls: "calls".to_string(),
            types: "types".to_string(),
            codebase: "codebase".to_string(),
        }
    }
}

impl VectorizerGraphExtractor {
    /// Create a new VectorizerGraphExtractor with default configuration
    pub fn new() -> Self {
        Self {
            mcp_client: None,
            query_cache: std::collections::HashMap::new(),
            config: VectorizerExtractorConfig::default(),
        }
    }

    /// Create a new VectorizerGraphExtractor with custom configuration
    pub fn with_config(config: VectorizerExtractorConfig) -> Self {
        Self {
            mcp_client: None,
            query_cache: std::collections::HashMap::new(),
            config,
        }
    }

    /// Set the MCP client for vectorizer communication
    pub fn set_mcp_client(&mut self, client: serde_json::Value) {
        self.mcp_client = Some(client);
    }

    /// Extract call graph data from vectorizer
    pub async fn extract_call_graph(
        &mut self,
        query: &str,
        _max_depth: Option<usize>,
    ) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Call Graph".to_string());

        // Extract function definitions
        let functions = self.search_functions(query).await?;

        // Extract function calls
        let calls = self.search_calls(query).await?;

        // Build graph nodes from functions
        for func in functions {
            let node = self.create_function_node(func)?;
            graph.add_node(node)?;
        }

        // Build graph edges from calls
        for call in calls {
            let edge = self.create_call_edge(call)?;
            graph.add_edge(edge)?;
        }

        Ok(graph)
    }

    /// Extract dependency graph data from vectorizer
    pub async fn extract_dependency_graph(&mut self, query: &str) -> Result<CorrelationGraph> {
        let mut graph =
            CorrelationGraph::new(GraphType::Dependency, "Dependency Graph".to_string());

        // Extract import/export relationships
        let imports = self.search_imports(query).await?;

        // Build graph nodes and edges from imports
        for import in imports {
            let (source_node, target_node, edge) = self.create_import_relationship(import)?;

            // Add nodes if they don't exist
            if graph.get_node(&source_node.id).is_none() {
                graph.add_node(source_node)?;
            }
            if graph.get_node(&target_node.id).is_none() {
                graph.add_node(target_node)?;
            }

            graph.add_edge(edge)?;
        }

        Ok(graph)
    }

    /// Extract data flow graph data from vectorizer
    pub async fn extract_data_flow_graph(&mut self, query: &str) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Data Flow Graph".to_string());

        // Extract variable usage and transformations
        let variables = self.search_variables(query).await?;
        let transformations = self.search_transformations(query).await?;

        // Build graph nodes from variables
        for var in variables {
            let node = self.create_variable_node(var)?;
            graph.add_node(node)?;
        }

        // Build graph edges from transformations
        for transform in transformations {
            let edge = self.create_transformation_edge(transform)?;
            graph.add_edge(edge)?;
        }

        Ok(graph)
    }

    /// Extract component graph data from vectorizer
    pub async fn extract_component_graph(&mut self, query: &str) -> Result<CorrelationGraph> {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Component Graph".to_string());

        // Extract class and interface definitions
        let classes = self.search_classes(query).await?;
        let interfaces = self.search_interfaces(query).await?;

        // Build graph nodes from classes and interfaces
        for class in classes {
            let node = self.create_class_node(class)?;
            graph.add_node(node)?;
        }

        for interface in interfaces {
            let node = self.create_interface_node(interface)?;
            graph.add_node(node)?;
        }

        // Extract inheritance and composition relationships
        let relationships = self.search_relationships(query).await?;

        // Build graph edges from relationships
        for rel in relationships {
            let edge = self.create_relationship_edge(rel)?;
            graph.add_edge(edge)?;
        }

        Ok(graph)
    }

    /// Search for functions using semantic search
    async fn search_functions(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.functions.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for function calls using semantic search
    async fn search_calls(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.calls.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for imports using semantic search
    async fn search_imports(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.imports.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for variables using semantic search
    async fn search_variables(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.types.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for transformations using semantic search
    async fn search_transformations(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.codebase.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for classes using semantic search
    async fn search_classes(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.types.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for interfaces using semantic search
    async fn search_interfaces(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.types.clone();
        self.semantic_search(&collection, query).await
    }

    /// Search for relationships using semantic search
    async fn search_relationships(&mut self, query: &str) -> Result<Vec<serde_json::Value>> {
        let collection = self.config.collections.codebase.clone();
        self.semantic_search(&collection, query).await
    }

    /// Perform semantic search using MCP vectorizer
    async fn semantic_search(
        &mut self,
        collection: &str,
        query: &str,
    ) -> Result<Vec<serde_json::Value>> {
        // Check cache first
        let cache_key = format!("{}:{}", collection, query);
        if self.config.enable_caching {
            if let Some(cached_result) = self.query_cache.get(&cache_key) {
                return Ok(cached_result.as_array().unwrap_or(&vec![]).to_vec());
            }
        }

        // Perform MCP vectorizer search
        let results = self.perform_mcp_search(collection, query).await?;

        // Cache results if enabled
        if self.config.enable_caching {
            self.query_cache
                .insert(cache_key, serde_json::Value::Array(results.clone()));
        }

        Ok(results)
    }

    /// Perform actual MCP search (placeholder implementation)
    async fn perform_mcp_search(
        &self,
        _collection: &str,
        _query: &str,
    ) -> Result<Vec<serde_json::Value>> {
        // This is a placeholder implementation
        // In a real implementation, this would use the MCP client to call vectorizer tools
        // For now, return empty results
        Ok(vec![])
    }

    /// Create a function node from vectorizer data
    pub fn create_function_node(&self, func_data: serde_json::Value) -> Result<GraphNode> {
        let id = func_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let label = func_data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Function")
            .to_string();

        let mut metadata = HashMap::new();
        if let Some(signature) = func_data.get("signature").and_then(|v| v.as_str()) {
            metadata.insert(
                "signature".to_string(),
                serde_json::Value::String(signature.to_string()),
            );
        }
        if let Some(file) = func_data.get("file").and_then(|v| v.as_str()) {
            metadata.insert(
                "file".to_string(),
                serde_json::Value::String(file.to_string()),
            );
        }

        Ok(GraphNode {
            id,
            node_type: NodeType::Function,
            label,
            metadata,
            position: None,
            size: None,
            color: None,
        })
    }

    /// Create a call edge from vectorizer data
    pub fn create_call_edge(&self, call_data: serde_json::Value) -> Result<GraphEdge> {
        let id = call_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let source = call_data
            .get("caller")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let target = call_data
            .get("callee")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut metadata = HashMap::new();
        if let Some(frequency) = call_data.get("frequency").and_then(|v| v.as_number()) {
            metadata.insert(
                "frequency".to_string(),
                serde_json::Value::Number(frequency.clone()),
            );
        }

        Ok(GraphEdge {
            id,
            source,
            target,
            edge_type: EdgeType::Calls,
            metadata,
            weight: 1.0,
            label: None,
        })
    }

    /// Create import relationship from vectorizer data
    pub fn create_import_relationship(
        &self,
        import_data: serde_json::Value,
    ) -> Result<(GraphNode, GraphNode, GraphEdge)> {
        let source_id = import_data
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let target_id = import_data
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let source_node = GraphNode {
            id: source_id.clone(),
            node_type: NodeType::Module,
            label: source_id.clone(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let target_node = GraphNode {
            id: target_id.clone(),
            node_type: NodeType::Module,
            label: target_id.clone(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let edge = GraphEdge {
            id: format!("{}->{}", source_id, target_id),
            source: source_id,
            target: target_id,
            edge_type: EdgeType::Imports,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        };

        Ok((source_node, target_node, edge))
    }

    /// Create a variable node from vectorizer data
    pub fn create_variable_node(&self, var_data: serde_json::Value) -> Result<GraphNode> {
        let id = var_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let label = var_data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Variable")
            .to_string();

        let mut metadata = HashMap::new();
        if let Some(var_type) = var_data.get("type").and_then(|v| v.as_str()) {
            metadata.insert(
                "type".to_string(),
                serde_json::Value::String(var_type.to_string()),
            );
        }

        Ok(GraphNode {
            id,
            node_type: NodeType::Variable,
            label,
            metadata,
            position: None,
            size: None,
            color: None,
        })
    }

    /// Create a transformation edge from vectorizer data
    pub fn create_transformation_edge(
        &self,
        transform_data: serde_json::Value,
    ) -> Result<GraphEdge> {
        let id = transform_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let source = transform_data
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let target = transform_data
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(GraphEdge {
            id,
            source,
            target,
            edge_type: EdgeType::Transforms,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        })
    }

    /// Create a class node from vectorizer data
    pub fn create_class_node(&self, class_data: serde_json::Value) -> Result<GraphNode> {
        let id = class_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let label = class_data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Class")
            .to_string();

        let mut metadata = HashMap::new();
        if let Some(base_class) = class_data.get("base_class").and_then(|v| v.as_str()) {
            metadata.insert(
                "base_class".to_string(),
                serde_json::Value::String(base_class.to_string()),
            );
        }

        Ok(GraphNode {
            id,
            node_type: NodeType::Class,
            label,
            metadata,
            position: None,
            size: None,
            color: None,
        })
    }

    /// Create an interface node from vectorizer data
    pub fn create_interface_node(&self, interface_data: serde_json::Value) -> Result<GraphNode> {
        let id = interface_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let label = interface_data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Interface")
            .to_string();

        Ok(GraphNode {
            id,
            node_type: NodeType::API,
            label,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        })
    }

    /// Create a relationship edge from vectorizer data
    pub fn create_relationship_edge(&self, rel_data: serde_json::Value) -> Result<GraphEdge> {
        let id = rel_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let source = rel_data
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let target = rel_data
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let edge_type = match rel_data.get("type").and_then(|v| v.as_str()) {
            Some("inherits") => EdgeType::Inherits,
            Some("implements") => EdgeType::Composes,
            Some("uses") => EdgeType::Uses,
            _ => EdgeType::Depends,
        };

        Ok(GraphEdge {
            id,
            source,
            target,
            edge_type,
            metadata: HashMap::new(),
            weight: 1.0,
            label: None,
        })
    }

    /// Clear the query cache
    pub fn clear_cache(&mut self) {
        self.query_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (
            self.query_cache.len(),
            self.query_cache
                .values()
                .map(|v| v.as_array().map(|a| a.len()).unwrap_or(0))
                .sum(),
        )
    }
}

impl Default for VectorizerGraphExtractor {
    fn default() -> Self {
        Self::new()
    }
}
