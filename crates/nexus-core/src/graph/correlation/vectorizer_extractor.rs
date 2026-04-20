//! `VectorizerGraphExtractor` — integrates with the Vectorizer MCP server
//! to extract code relationships and materialise them as correlation
//! graphs. Owns its own query-cache and configuration.

use super::*;

/// VectorizerGraphExtractor for data access and graph generation
///
/// This struct provides integration with the Vectorizer MCP server to extract
/// code relationships and generate correlation graphs from vectorized data.
#[derive(Debug, Clone)]
pub struct VectorizerGraphExtractor {
    /// MCP client for vectorizer communication
    pub(super) mcp_client: Option<serde_json::Value>,
    /// Cache for vectorizer queries to improve performance
    pub(super) query_cache: std::collections::HashMap<String, serde_json::Value>,
    /// Configuration for graph extraction
    pub(super) config: VectorizerExtractorConfig,
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
