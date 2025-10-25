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
    /// Build a correlation graph from source data
    fn build(&self, source_data: &GraphSourceData) -> Result<CorrelationGraph>;

    /// Get the graph type this builder creates
    fn graph_type(&self) -> GraphType;

    /// Get builder name
    fn name(&self) -> &str;
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

    fn graph_type(&self) -> GraphType {
        GraphType::Call
    }

    fn name(&self) -> &str {
        &self.name
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

    fn graph_type(&self) -> GraphType {
        GraphType::Dependency
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Graph correlation manager
pub struct GraphCorrelationManager {
    /// Available graph builders
    builders: HashMap<GraphType, Box<dyn GraphBuilder + Send + Sync>>,
}

impl GraphCorrelationManager {
    /// Create a new graph correlation manager
    pub fn new() -> Self {
        let mut manager = Self {
            builders: HashMap::new(),
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
}
