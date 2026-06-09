//! Graph builder trait, source data, configuration, concrete builder
//! implementations (`CallGraphBuilder`, `DependencyGraphBuilder`,
//! `DefaultGraphBuilder`, `DataFlowGraphBuilder`, `ComponentGraphBuilder`),
//! and the `GraphCorrelationManager` registry.

use crate::{Error, Result};
use std::collections::HashMap;

use super::graph_types::{
    CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType, RecursiveCallConfig,
};

// DataFlowAnalyzer is used by DefaultGraphBuilder::build_data_flow_graph.
use super::data_flow::DataFlowAnalyzer;

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

// ---------------------------------------------------------------------------
// Call graph builder
// ---------------------------------------------------------------------------

/// Call graph builder implementation
pub struct CallGraphBuilder {
    name: String,
    use_hierarchical_layout: bool,
    layout_config: Option<super::hierarchical_layout::HierarchicalCallGraphConfig>,
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
            layout_config: Some(super::hierarchical_layout::HierarchicalCallGraphConfig::default()),
            recursive_call_config: RecursiveCallConfig::default(),
        }
    }

    /// Enable hierarchical layout with custom configuration
    pub fn with_hierarchical_layout(
        mut self,
        config: super::hierarchical_layout::HierarchicalCallGraphConfig,
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
                    super::hierarchical_layout::HierarchicalCallGraphLayout::new(config.clone());
                graph = layout_engine.layout(graph)?;
            } else {
                let layout_engine =
                    super::hierarchical_layout::HierarchicalCallGraphLayout::with_default_config();
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

// ---------------------------------------------------------------------------
// Dependency graph builder
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Default (full-featured) graph builder
// ---------------------------------------------------------------------------

/// Default graph builder that provides a base implementation
/// for building all types of correlation graphs
pub struct DefaultGraphBuilder {
    /// Name of the builder
    name: String,
    /// Configuration for graph building
    config: GraphBuilderConfig,
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

// ---------------------------------------------------------------------------
// Thin wrapper builders (DataFlow / Component)
// ---------------------------------------------------------------------------

/// Wrapper builders for DataFlow and Component graphs
pub struct DataFlowGraphBuilder;
/// Wrapper builder for Component graphs
pub struct ComponentGraphBuilder;

impl DataFlowGraphBuilder {
    /// Create a new data flow graph builder
    pub fn new(_name: String) -> Self {
        Self
    }
}

impl ComponentGraphBuilder {
    /// Create a new component graph builder
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

// ---------------------------------------------------------------------------
// Graph correlation manager
// ---------------------------------------------------------------------------

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
