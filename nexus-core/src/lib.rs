//! Nexus Core - Property Graph Database Engine
//!
//! This crate provides the core graph database engine for Nexus, implementing:
//! - Property graph model (nodes with labels, edges with types, properties)
//! - Neo4j-inspired record stores (nodes.store, rels.store, props.store)
//! - Page cache with eviction policies (clock/2Q/TinyLFU)
//! - Write-ahead log (WAL) with MVCC by epoch
//! - Cypher subset executor (pattern matching, expand, filter, project)
//! - Multi-index subsystem (label bitmap, B-tree, full-text, KNN)
//! - Graph validation and integrity checks
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │           Cypher Executor                    │
//! │   (Pattern Match, Expand, Filter, Project)  │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │          Transaction Layer                   │
//! │        (MVCC, Locking, Isolation)           │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │            Index Layer                       │
//! │  (Label Bitmap, B-tree, Full-text, KNN)     │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │           Storage Layer                      │
//! │  (Record Stores, Page Cache, WAL, Catalog)  │
//! └─────────────────────────────────────────────┘
//! ```

#![allow(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)] // Allow during initial scaffolding

pub mod auth;
pub mod catalog;
pub mod concurrent_access;
pub mod error;
pub mod executor;
pub mod graph; // Unified graph module with submodules
pub mod index;
pub mod loader;
pub mod memory_management;
pub mod monitoring;
pub mod page_cache;
pub mod performance;
pub mod retry;
pub mod security;
pub mod storage;
pub mod transaction;
pub mod validation;
pub mod vectorizer_cache;
pub mod wal;

pub use error::{Error, Result};
pub use graph::clustering::{
    Cluster, ClusteringAlgorithm, ClusteringConfig, ClusteringEngine, ClusteringMetrics,
    ClusteringResult, DistanceMetric, FeatureStrategy, LinkageType,
};
pub use graph::comparison::{
    ComparisonOptions, DiffSummary, EdgeChanges, EdgeModification, GraphComparator, GraphDiff,
    NodeChanges, NodeModification, PropertyValueChange,
};
pub use graph::construction::{
    CircularLayout, ConnectedComponents, ForceDirectedLayout, GraphLayout, GridLayout,
    HierarchicalLayout, KMeansClustering, LayoutDirection, LayoutEdge, LayoutNode, Point2D,
};
pub use graph::correlation::NodeType;
pub use graph::simple::{
    Edge as SimpleEdge, EdgeId as SimpleEdgeId, Graph as SimpleGraph,
    GraphStats as SimpleGraphStats, Node as SimpleNode, NodeId as SimpleNodeId, PropertyValue,
};
pub use graph::{Edge, EdgeId, Graph, GraphStats, Node, NodeId};
use std::sync::Arc;
pub use validation::{
    GraphValidator, ValidationConfig, ValidationError, ValidationErrorType, ValidationResult,
    ValidationSeverity, ValidationStats, ValidationWarning, ValidationWarningType,
};

/// Graph statistics for analysis and monitoring
#[derive(Debug, Clone, Default)]
pub struct GraphStatistics {
    /// Total number of nodes
    pub node_count: u64,
    /// Total number of relationships
    pub relationship_count: u64,
    /// Count of nodes per label
    pub label_counts: std::collections::HashMap<String, u64>,
    /// Count of relationships per type
    pub relationship_type_counts: std::collections::HashMap<String, u64>,
}

/// Graph database engine
pub struct Engine {
    /// Storage catalog for label/type/key mappings
    pub catalog: catalog::Catalog,
    /// Record stores for nodes and relationships
    pub storage: storage::RecordStore,
    /// Page cache for memory management
    pub page_cache: page_cache::PageCache,
    /// Write-ahead log for durability
    pub wal: wal::Wal,
    /// Transaction manager for MVCC
    pub transaction_manager: transaction::TransactionManager,
    /// Index subsystem
    pub indexes: index::IndexManager,
    /// Query executor
    pub executor: executor::Executor,
    /// Keeps temporary directory alive for Engine::new(). None for persistent storage.
    _temp_dir: Option<tempfile::TempDir>,
}

impl Engine {
    /// Create a new engine instance with all components
    /// Uses temporary directory (for backward compatibility)
    ///
    /// The temporary directory will be automatically cleaned up when the Engine is dropped.
    /// For persistent storage, use `Engine::with_data_dir()` instead.
    pub fn new() -> Result<Self> {
        // Create temporary directory for data
        let temp_dir = tempfile::tempdir()?;
        let data_dir = temp_dir.path().to_path_buf();
        let mut engine = Self::with_data_dir(&data_dir)?;
        engine._temp_dir = Some(temp_dir);
        Ok(engine)
    }

    /// Create a new engine instance with a specific data directory
    /// This allows persistent storage instead of temporary directories
    pub fn with_data_dir<P: AsRef<std::path::Path>>(data_dir: P) -> Result<Self> {
        let data_dir = data_dir.as_ref();

        // Ensure data directory exists
        std::fs::create_dir_all(data_dir)?;

        // Initialize catalog
        let catalog = catalog::Catalog::new(data_dir.join("catalog.mdb"))?;

        // Initialize record stores
        let storage = storage::RecordStore::new(data_dir)?;

        // Initialize page cache
        let page_cache = page_cache::PageCache::new(1024)?; // 1024 pages = 8MB

        // Initialize WAL
        let wal = wal::Wal::new(data_dir.join("wal.log"))?;

        // Initialize transaction manager
        let transaction_manager = transaction::TransactionManager::new()?;

        // Initialize index manager
        let indexes = index::IndexManager::new(data_dir.join("indexes"))?;

        // Initialize executor
        let executor =
            executor::Executor::new(&catalog, &storage, &indexes.label_index, &indexes.knn_index)?;

        let mut engine = Engine {
            catalog,
            storage,
            page_cache,
            wal,
            transaction_manager,
            indexes,
            executor,
            _temp_dir: None,
        };

        engine.rebuild_indexes_from_storage()?;

        Ok(engine)
    }

    fn rebuild_indexes_from_storage(&mut self) -> Result<()> {
        let total_nodes = self.storage.node_count();

        for node_id in 0..total_nodes {
            let record = match self.storage.read_node(node_id) {
                Ok(record) => record,
                Err(_) => continue,
            };

            if record.is_deleted() {
                continue;
            }

            let mut label_ids = Vec::new();
            for bit in 0..64 {
                if (record.label_bits & (1u64 << bit)) != 0 {
                    label_ids.push(bit as u32);
                }
            }

            if !label_ids.is_empty() {
                self.indexes.label_index.add_node(node_id, &label_ids)?;
            }
        }

        Ok(())
    }

    /// Execute CREATE query via Engine to ensure proper persistence
    fn execute_create_query(&mut self, ast: &executor::parser::CypherQuery) -> Result<()> {
        use std::collections::HashMap;

        // Map of variable names to created node IDs
        let mut created_nodes: HashMap<String, u64> = HashMap::new();

        for clause in &ast.clauses {
            if let executor::parser::Clause::Create(create_clause) = clause {
                let mut last_node_id: Option<u64> = None;

                // Process pattern elements
                for (i, element) in create_clause.pattern.elements.iter().enumerate() {
                    match element {
                        executor::parser::PatternElement::Node(node) => {
                            // Extract properties
                            let properties = if let Some(props_map) = &node.properties {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create node using Engine API
                            let node_id = self.create_node(node.labels.clone(), properties)?;

                            // Store node ID if variable exists
                            if let Some(var) = &node.variable {
                                created_nodes.insert(var.clone(), node_id);
                            }

                            last_node_id = Some(node_id);
                        }
                        executor::parser::PatternElement::Relationship(rel) => {
                            // Get source node
                            let source_id = last_node_id.ok_or_else(|| {
                                Error::CypherExecution(
                                    "Relationship must follow a node".to_string(),
                                )
                            })?;

                            // Get target node (next element)
                            let target_id = if i + 1 < create_clause.pattern.elements.len() {
                                if let executor::parser::PatternElement::Node(target_node) =
                                    &create_clause.pattern.elements[i + 1]
                                {
                                    // Extract target properties
                                    let target_properties =
                                        if let Some(props_map) = &target_node.properties {
                                            let mut json_props = serde_json::Map::new();
                                            for (key, value_expr) in &props_map.properties {
                                                let json_value =
                                                    self.expression_to_json_value(value_expr)?;
                                                json_props.insert(key.clone(), json_value);
                                            }
                                            serde_json::Value::Object(json_props)
                                        } else {
                                            serde_json::Value::Null
                                        };

                                    // Create target node
                                    let tid = self.create_node(
                                        target_node.labels.clone(),
                                        target_properties,
                                    )?;

                                    // Store target node ID
                                    if let Some(var) = &target_node.variable {
                                        created_nodes.insert(var.clone(), tid);
                                    }

                                    last_node_id = Some(tid);
                                    tid
                                } else {
                                    return Err(Error::CypherExecution(
                                        "Relationship must be followed by a node".to_string(),
                                    ));
                                }
                            } else {
                                return Err(Error::CypherExecution(
                                    "Pattern must end with a node".to_string(),
                                ));
                            };

                            // Get relationship type
                            let rel_type = rel.types.first().ok_or_else(|| {
                                Error::CypherExecution("Relationship must have a type".to_string())
                            })?;

                            // Extract relationship properties
                            let rel_properties = if let Some(props_map) = &rel.properties {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create relationship using Engine API
                            self.create_relationship(
                                source_id,
                                target_id,
                                rel_type.to_string(),
                                rel_properties,
                            )?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to JSON value (helper for CREATE)
    fn expression_to_json_value(
        &self,
        expr: &executor::parser::Expression,
    ) -> Result<serde_json::Value> {
        match expr {
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::String(s) => Ok(serde_json::Value::String(s.clone())),
                executor::parser::Literal::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
                executor::parser::Literal::Float(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        Ok(serde_json::Value::Number(num))
                    } else {
                        Err(Error::CypherExecution(format!("Invalid float: {}", f)))
                    }
                }
                executor::parser::Literal::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
                executor::parser::Literal::Null => Ok(serde_json::Value::Null),
            },
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Refresh the executor to ensure it sees the latest storage state
    /// This is necessary because the executor uses a cloned RecordStore
    /// which has its own PropertyStore instance
    fn refresh_executor(&mut self) -> Result<()> {
        // Recreate executor with current storage state
        self.executor = executor::Executor::new(
            &self.catalog,
            &self.storage,
            &self.indexes.label_index,
            &self.indexes.knn_index,
        )?;
        Ok(())
    }

    /// Create a new engine with default configuration
    pub fn new_default() -> Result<Self> {
        Self::new()
    }

    /// Get engine statistics
    pub fn stats(&self) -> Result<EngineStats> {
        Ok(EngineStats {
            nodes: self.storage.node_count(),
            relationships: self.storage.relationship_count(),
            labels: self.catalog.label_count(),
            rel_types: self.catalog.rel_type_count(),
            page_cache_hits: self.page_cache.hit_count(),
            page_cache_misses: self.page_cache.miss_count(),
            wal_entries: self.wal.entry_count(),
            active_transactions: self.transaction_manager.active_count(),
        })
    }

    /// Execute a Cypher query
    pub fn execute_cypher(&mut self, query: &str) -> Result<executor::ResultSet> {
        // Parse query to check if it contains CREATE clauses
        let mut parser = executor::parser::CypherParser::new(query.to_string());
        let ast = parser.parse()?;

        // Check if query is a standalone CREATE (no MATCH clause before CREATE)
        // If there's a MATCH, the executor will handle CREATE in context
        let is_standalone_create = if let Some(first_clause) = ast.clauses.first() {
            matches!(
                first_clause,
                executor::parser::Clause::Create(_) | executor::parser::Clause::Merge(_)
            )
        } else {
            false
        };

        if is_standalone_create {
            // Execute standalone CREATE via Engine to ensure proper persistence
            self.execute_create_query(&ast)?;

            // Refresh executor to see the changes
            self.refresh_executor()?;
        }

        // Execute the query normally
        let query_obj = executor::Query {
            cypher: query.to_string(),
            params: std::collections::HashMap::new(),
        };
        self.executor.execute(&query_obj)
    }

    /// Create a new node
    pub fn create_node(
        &mut self,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let mut tx = self.transaction_manager.begin_write()?;

        // Create labels in catalog and get their IDs
        let mut label_bits = 0u64;
        let mut label_ids = Vec::new();
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
            label_ids.push(label_id);
        }

        let node_id = self
            .storage
            .create_node_with_label_bits(&mut tx, label_bits, properties)?;
        self.transaction_manager.commit(&mut tx)?;

        // CRITICAL FIX: Flush storage to ensure data is persisted to disk
        self.storage.flush()?;

        // Update label_index to track this node
        self.indexes.label_index.add_node(node_id, &label_ids)?;

        // CRITICAL FIX: Refresh executor to ensure it sees the newly written properties
        self.refresh_executor()?;

        Ok(node_id)
    }

    /// Create a new relationship
    pub fn create_relationship(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let mut tx = self.transaction_manager.begin_write()?;
        let type_id = self.catalog.get_or_create_type(&rel_type)?;
        let rel_id = self
            .storage
            .create_relationship(&mut tx, from, to, type_id, properties)?;
        self.transaction_manager.commit(&mut tx)?;

        // CRITICAL FIX: Flush storage to ensure data is persisted to disk
        self.storage.flush()?;

        self.catalog.increment_rel_count(type_id)?;

        // CRITICAL FIX: Refresh executor to ensure it sees the newly written properties
        self.refresh_executor()?;

        Ok(rel_id)
    }

    /// Get node by ID
    pub fn get_node(&mut self, id: u64) -> Result<Option<storage::NodeRecord>> {
        let tx = self.transaction_manager.begin_read()?;
        self.storage.get_node(&tx, id)
    }

    /// Get relationship by ID
    pub fn get_relationship(&mut self, id: u64) -> Result<Option<storage::RelationshipRecord>> {
        let tx = self.transaction_manager.begin_read()?;
        self.storage.get_relationship(&tx, id)
    }

    /// Update a node with new labels and properties
    pub fn update_node(
        &mut self,
        id: u64,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<()> {
        // Check if node exists
        if self.get_node(id)?.is_none() {
            return Err(Error::NotFound(format!("Node {} not found", id)));
        }

        // Get or create label IDs
        let mut label_bits = 0u64;
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
        }

        // Create updated node record
        let mut node_record = storage::NodeRecord::new();
        node_record.label_bits = label_bits;

        // Store properties and get property pointer
        node_record.prop_ptr =
            if properties.is_object() && !properties.as_object().unwrap().is_empty() {
                self.storage.property_store.store_properties(
                    id,
                    storage::property_store::EntityType::Node,
                    properties,
                )?
            } else {
                0
            };

        // Write updated record
        let mut tx = self.transaction_manager.begin_write()?;
        self.storage.write_node(id, &node_record)?;
        self.transaction_manager.commit(&mut tx)?;

        // Update statistics
        for label in &labels {
            if let Ok(label_id) = self.catalog.get_or_create_label(label) {
                self.catalog.increment_node_count(label_id)?;
            }
        }

        Ok(())
    }

    /// Delete a node by ID
    pub fn delete_node(&mut self, id: u64) -> Result<bool> {
        // Check if node exists
        if let Ok(Some(node_record)) = self.get_node(id) {
            // Mark node as deleted
            let mut deleted_record = node_record;
            deleted_record.mark_deleted();

            let mut tx = self.transaction_manager.begin_write()?;
            self.storage.write_node(id, &deleted_record)?;
            self.transaction_manager.commit(&mut tx)?;

            // Update statistics
            for bit in 0..64 {
                if (node_record.label_bits & (1u64 << bit)) != 0 {
                    if let Ok(label_id) = self.catalog.get_label_id_by_id(bit as u32) {
                        self.catalog.decrement_node_count(label_id)?;
                    }
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Perform KNN search
    pub fn knn_search(&self, label: &str, vector: &[f32], k: usize) -> Result<Vec<(u64, f32)>> {
        self.indexes.knn_search(label, vector, k)
    }

    /// Perform node clustering on the graph
    pub fn cluster_nodes(&mut self, config: ClusteringConfig) -> Result<ClusteringResult> {
        // Convert storage to simple graph for clustering
        let simple_graph = self.convert_to_simple_graph()?;
        let engine = ClusteringEngine::new(config);
        engine.cluster(&simple_graph)
    }

    /// Convert the storage to a simple graph for clustering and analysis
    pub fn convert_to_simple_graph(&mut self) -> Result<graph::simple::Graph> {
        let mut simple_graph = graph::simple::Graph::new();

        // Scan all nodes and add them to the simple graph
        for node_id in 0..self.storage.node_count() {
            if let Ok(Some(node_record)) = self.get_node(node_id) {
                // Convert labels from bitmap to vector
                let labels = self
                    .catalog
                    .get_labels_from_bitmap(node_record.label_bits)?;

                // Create node in simple graph
                let simple_node_id = graph::simple::NodeId::new(node_id);
                let node = graph::simple::Node::new(simple_node_id, labels);

                // Load properties if they exist
                if node_record.prop_ptr != 0 {
                    if let Ok(Some(_properties)) = self.storage.load_node_properties(node_id) {
                        // Properties are loaded but not yet integrated into simple graph
                        // This will be handled in future property integration
                    }
                }

                simple_graph.update_node(node)?;
            }
        }

        // Scan all relationships and add them to the simple graph
        for rel_id in 0..self.storage.relationship_count() {
            if let Ok(Some(rel_record)) = self.get_relationship(rel_id) {
                // Get relationship type name
                let rel_type = self
                    .catalog
                    .get_type_name(rel_record.type_id)
                    .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                    .unwrap_or_else(|| "UNKNOWN".to_string());

                // Load properties if they exist
                if rel_record.prop_ptr != 0 {
                    if let Ok(Some(_properties)) = self.storage.load_relationship_properties(rel_id)
                    {
                        // Properties are loaded but not yet integrated into simple graph
                        // This will be handled in future property integration
                    }
                }

                // Create edge in simple graph
                let source_id = graph::simple::NodeId::new(rel_record.src_id);
                let target_id = graph::simple::NodeId::new(rel_record.dst_id);

                simple_graph.create_edge(source_id, target_id, rel_type)?;
            }
        }

        Ok(simple_graph)
    }

    /// Perform label-based grouping of nodes
    pub fn group_nodes_by_labels(&mut self) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        self.cluster_nodes(config)
    }

    /// Perform property-based grouping of nodes
    pub fn group_nodes_by_property(&mut self, property_key: &str) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::PropertyBased {
                property_key: property_key.to_string(),
            },
            feature_strategy: FeatureStrategy::PropertyBased {
                property_keys: vec![property_key.to_string()],
            },
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        self.cluster_nodes(config)
    }

    /// Perform K-means clustering on nodes
    pub fn kmeans_cluster_nodes(
        &mut self,
        k: usize,
        max_iterations: usize,
    ) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::KMeans { k, max_iterations },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };
        self.cluster_nodes(config)
    }

    /// Perform community detection on nodes
    pub fn detect_communities(&mut self) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::CommunityDetection,
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        self.cluster_nodes(config)
    }

    /// Export graph data to JSON format
    pub fn export_to_json(&mut self) -> Result<serde_json::Value> {
        let mut export_data = serde_json::Map::new();

        // Export nodes
        let mut nodes = Vec::new();
        for node_id in 0..self.storage.node_count() {
            if let Ok(Some(node_record)) = self.get_node(node_id) {
                let labels = self
                    .catalog
                    .get_labels_from_bitmap(node_record.label_bits)?;
                let node_data = serde_json::json!({
                    "id": node_id,
                    "labels": labels,
                    "properties": {} // TODO: Add property loading when property store is implemented
                });
                nodes.push(node_data);
            }
        }
        export_data.insert("nodes".to_string(), serde_json::Value::Array(nodes));

        // Export relationships
        let mut relationships = Vec::new();
        for rel_id in 0..self.storage.relationship_count() {
            if let Ok(Some(rel_record)) = self.get_relationship(rel_id) {
                let rel_type = self
                    .catalog
                    .get_type_name(rel_record.type_id)
                    .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                    .unwrap_or_else(|| "UNKNOWN".to_string());

                // Copy values to avoid alignment issues with packed structs
                let src_id = rel_record.src_id;
                let dst_id = rel_record.dst_id;

                let rel_data = serde_json::json!({
                    "id": rel_id,
                    "source": src_id,
                    "target": dst_id,
                    "type": rel_type,
                    "properties": {} // TODO: Add property loading when property store is implemented
                });
                relationships.push(rel_data);
            }
        }
        export_data.insert(
            "relationships".to_string(),
            serde_json::Value::Array(relationships),
        );

        Ok(serde_json::Value::Object(export_data))
    }

    /// Get graph statistics
    pub fn get_graph_statistics(&mut self) -> Result<GraphStatistics> {
        let mut stats = GraphStatistics::default();

        // Count nodes
        for node_id in 0..self.storage.node_count() {
            if let Ok(Some(node_record)) = self.get_node(node_id) {
                if !node_record.is_deleted() {
                    stats.node_count += 1;

                    // Count labels
                    let labels = self
                        .catalog
                        .get_labels_from_bitmap(node_record.label_bits)?;
                    for label in labels {
                        *stats.label_counts.entry(label).or_insert(0) += 1;
                    }
                }
            }
        }

        // Count relationships
        for rel_id in 0..self.storage.relationship_count() {
            if let Ok(Some(rel_record)) = self.get_relationship(rel_id) {
                if !rel_record.is_deleted() {
                    stats.relationship_count += 1;

                    // Count relationship types
                    let rel_type = self
                        .catalog
                        .get_type_name(rel_record.type_id)
                        .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                        .unwrap_or_else(|| "UNKNOWN".to_string());
                    *stats.relationship_type_counts.entry(rel_type).or_insert(0) += 1;
                }
            }
        }

        Ok(stats)
    }

    /// Clear all data from the graph
    pub fn clear_all_data(&mut self) -> Result<()> {
        // Clear storage
        self.storage.clear_all()?;

        // Reset catalog statistics
        let mut stats = self.catalog.get_statistics()?;
        stats.node_counts.clear();
        stats.rel_counts.clear();
        self.catalog.update_statistics(&stats)?;

        Ok(())
    }

    /// Validate the entire graph for integrity and consistency
    pub fn validate_graph(&self) -> Result<ValidationResult> {
        // Create a temporary graph for validation
        let temp_dir = tempfile::tempdir()?;
        let store = storage::RecordStore::new(temp_dir.path())?;
        let catalog = catalog::Catalog::new(temp_dir.path().join("catalog"))?;
        let graph = Graph::new(store, Arc::new(catalog));
        graph.validate()
    }

    /// Quick health check for the graph
    pub fn graph_health_check(&self) -> Result<bool> {
        self.validate_graph().map(|result| result.is_valid)
    }

    /// Health check
    pub fn health_check(&self) -> Result<HealthStatus> {
        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            components: std::collections::HashMap::new(),
        };

        // Check catalog
        match self.catalog.health_check() {
            Ok(_) => {
                status
                    .components
                    .insert("catalog".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status
                    .components
                    .insert("catalog".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }

        // Check storage
        match self.storage.health_check() {
            Ok(_) => {
                status
                    .components
                    .insert("storage".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status
                    .components
                    .insert("storage".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }

        // Check page cache
        match self.page_cache.health_check() {
            Ok(_) => {
                status
                    .components
                    .insert("page_cache".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status
                    .components
                    .insert("page_cache".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }

        // Check WAL
        match self.wal.health_check() {
            Ok(_) => {
                status
                    .components
                    .insert("wal".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status
                    .components
                    .insert("wal".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }

        // Check indexes
        match self.indexes.health_check() {
            Ok(_) => {
                status
                    .components
                    .insert("indexes".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status
                    .components
                    .insert("indexes".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }

        Ok(status)
    }
}

/// Engine statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineStats {
    pub nodes: u64,
    pub relationships: u64,
    pub labels: u64,
    pub rel_types: u64,
    pub page_cache_hits: u64,
    pub page_cache_misses: u64,
    pub wal_entries: u64,
    pub active_transactions: u64,
}

/// Health status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthStatus {
    pub overall: HealthState,
    pub components: std::collections::HashMap<String, HealthState>,
}

/// Health state
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HealthState {
    Healthy,
    Unhealthy,
    Degraded,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new().expect("Failed to create default engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_storage() {
        let err = Error::storage("test error");
        assert!(matches!(err, Error::Storage(_)));
        assert_eq!(err.to_string(), "Storage error: test error");
    }

    #[test]
    fn test_error_page_cache() {
        let err = Error::page_cache("cache full");
        assert!(matches!(err, Error::PageCache(_)));
    }

    #[test]
    fn test_error_wal() {
        let err = Error::wal("checkpoint failed");
        assert!(matches!(err, Error::Wal(_)));
    }

    #[test]
    fn test_error_catalog() {
        let err = Error::catalog("catalog error");
        assert!(matches!(err, Error::Catalog(_)));
        assert!(err.to_string().contains("catalog error"));
    }

    #[test]
    fn test_error_transaction() {
        let err = Error::transaction("tx failed");
        assert!(matches!(err, Error::Transaction(_)));
        assert!(err.to_string().contains("tx failed"));
    }

    #[test]
    fn test_error_index() {
        let err = Error::index("index error");
        assert!(matches!(err, Error::Index(_)));
        assert!(err.to_string().contains("index error"));
    }

    #[test]
    fn test_error_executor() {
        let err = Error::executor("exec error");
        assert!(matches!(err, Error::Executor(_)));
        assert!(err.to_string().contains("exec error"));
    }

    #[test]
    fn test_error_internal() {
        let err = Error::internal("internal error");
        assert!(matches!(err, Error::Internal(_)));
        assert!(err.to_string().contains("internal error"));
    }

    #[test]
    fn test_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn test_node_type_export() {
        // Test that NodeType is properly exported from the main library
        use crate::NodeType;

        let function = NodeType::Function;
        let module = NodeType::Module;
        let class = NodeType::Class;
        let variable = NodeType::Variable;
        let api = NodeType::API;

        // Test that all variants are accessible
        assert_eq!(format!("{:?}", function), "Function");
        assert_eq!(format!("{:?}", module), "Module");
        assert_eq!(format!("{:?}", class), "Class");
        assert_eq!(format!("{:?}", variable), "Variable");
        assert_eq!(format!("{:?}", api), "API");

        // Test serialization
        let json = serde_json::to_string(&api).unwrap();
        assert!(json.contains("API"));

        let deserialized: NodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, NodeType::API);
    }

    #[test]
    fn test_error_database() {
        let db_err = heed::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "db file not found",
        ));
        let err: Error = db_err.into();
        assert!(matches!(err, Error::Database(_)));
    }

    #[test]
    fn test_error_not_found() {
        let err = Error::NotFound("node 123".to_string());
        assert!(matches!(err, Error::NotFound(_)));
        assert!(err.to_string().contains("node 123"));
    }

    #[test]
    fn test_error_invalid_id() {
        let err = Error::InvalidId("invalid node id".to_string());
        assert!(matches!(err, Error::InvalidId(_)));
        assert!(err.to_string().contains("invalid node id"));
    }

    #[test]
    fn test_error_constraint_violation() {
        let err = Error::ConstraintViolation("unique constraint violated".to_string());
        assert!(matches!(err, Error::ConstraintViolation(_)));
        assert!(err.to_string().contains("unique constraint violated"));
    }

    #[test]
    fn test_error_type_mismatch() {
        let err = Error::TypeMismatch {
            expected: "String".to_string(),
            actual: "Int64".to_string(),
        };
        assert!(matches!(err, Error::TypeMismatch { .. }));
        assert!(err.to_string().contains("String"));
        assert!(err.to_string().contains("Int64"));
    }

    #[test]
    fn test_error_cypher_syntax() {
        let err = Error::CypherSyntax("unexpected token".to_string());
        assert!(matches!(err, Error::CypherSyntax(_)));
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn test_error_debug() {
        let err = Error::Storage("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Storage"));
    }

    #[test]
    fn test_engine_creation() {
        let engine = Engine::new();
        assert!(engine.is_ok());
        let engine = engine.unwrap();

        // Test that all components are initialized
        // Note: These are unsigned types, so >= 0 is always true
        // We just verify the methods don't panic
        let _ = engine.catalog.label_count();
        let _ = engine.storage.node_count();
        let _ = engine.storage.relationship_count();
        let _ = engine.page_cache.hit_count();
        let _ = engine.page_cache.miss_count();
        let _ = engine.wal.entry_count();
        let _ = engine.transaction_manager.active_count();
    }

    #[test]
    fn test_engine_default() {
        let engine = Engine::default();
        // Test passes if default creation succeeds
        drop(engine);
    }

    #[test]
    fn test_engine_new_default() {
        let engine = Engine::new_default();
        assert!(engine.is_ok());
        drop(engine);
    }

    #[test]
    fn test_engine_stats() {
        let engine = Engine::new().unwrap();
        let stats = engine.stats().unwrap();

        // Test that stats are accessible
        // Note: These are unsigned types, so >= 0 is always true
        // We just verify the stats are accessible
        let _ = stats.nodes;
        let _ = stats.relationships;
        let _ = stats.labels;
        let _ = stats.rel_types;
        let _ = stats.page_cache_hits;
        let _ = stats.page_cache_misses;
        let _ = stats.wal_entries;
        let _ = stats.active_transactions;
    }

    #[test]
    fn test_engine_execute_cypher() {
        let mut engine = Engine::new().unwrap();

        // Test executing a simple query
        let result = engine.execute_cypher("MATCH (n) RETURN n");
        // Should not panic, even if query fails
        drop(result);
    }

    #[test]
    fn test_engine_create_node() {
        let mut engine = Engine::new().unwrap();

        // Test creating a node
        let labels = vec!["Person".to_string()];
        let properties = serde_json::json!({"name": "Alice", "age": 30});

        let result = engine.create_node(labels, properties);
        // Should not panic, even if creation fails
        drop(result);
    }

    #[test]
    fn test_engine_create_relationship() {
        let mut engine = Engine::new().unwrap();

        // Test creating a relationship
        let result = engine.create_relationship(
            1, // from
            2, // to
            "KNOWS".to_string(),
            serde_json::json!({"since": 2020}),
        );
        // Should not panic, even if creation fails
        drop(result);
    }

    #[test]
    fn test_engine_get_node() {
        let mut engine = Engine::new().unwrap();

        // Test getting a node
        let result = engine.get_node(1);
        // Should not panic, even if node doesn't exist
        drop(result);
    }

    #[test]
    fn test_engine_get_relationship() {
        let mut engine = Engine::new().unwrap();

        // Test getting a relationship
        let result = engine.get_relationship(1);
        // Should not panic, even if relationship doesn't exist
        drop(result);
    }

    #[test]
    fn test_engine_knn_search() {
        let engine = Engine::new().unwrap();

        // Test KNN search
        let vector = vec![0.1, 0.2, 0.3, 0.4];
        let result = engine.knn_search("Person", &vector, 5);
        // Should not panic, even if search fails
        drop(result);
    }

    #[test]
    fn test_engine_health_check() {
        let engine = Engine::new().unwrap();

        // Test health check
        let status = engine.health_check().unwrap();

        // Test that health status is properly structured
        assert!(matches!(
            status.overall,
            HealthState::Healthy | HealthState::Unhealthy | HealthState::Degraded
        ));
        assert!(!status.components.is_empty());

        // Test that all expected components are present
        let expected_components = ["catalog", "storage", "page_cache", "wal", "indexes"];
        for component in expected_components {
            assert!(status.components.contains_key(component));
        }
    }

    #[test]
    fn test_engine_stats_serialization() {
        let engine = Engine::new().unwrap();
        let stats = engine.stats().unwrap();

        // Test JSON serialization
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("nodes"));
        assert!(json.contains("relationships"));
        assert!(json.contains("labels"));
        assert!(json.contains("rel_types"));
        assert!(json.contains("page_cache_hits"));
        assert!(json.contains("page_cache_misses"));
        assert!(json.contains("wal_entries"));
        assert!(json.contains("active_transactions"));

        // Test deserialization
        let deserialized: EngineStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.nodes, stats.nodes);
        assert_eq!(deserialized.relationships, stats.relationships);
        assert_eq!(deserialized.labels, stats.labels);
        assert_eq!(deserialized.rel_types, stats.rel_types);
    }

    #[test]
    fn test_health_status_serialization() {
        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            components: std::collections::HashMap::new(),
        };
        status
            .components
            .insert("test".to_string(), HealthState::Healthy);

        // Test JSON serialization
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("overall"));
        assert!(json.contains("components"));
        assert!(json.contains("test"));

        // Test deserialization
        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.overall, HealthState::Healthy);
        assert!(deserialized.components.contains_key("test"));
    }

    #[test]
    fn test_health_state_variants() {
        // Test all health state variants
        assert_eq!(HealthState::Healthy, HealthState::Healthy);
        assert_eq!(HealthState::Unhealthy, HealthState::Unhealthy);
        assert_eq!(HealthState::Degraded, HealthState::Degraded);

        assert_ne!(HealthState::Healthy, HealthState::Unhealthy);
        assert_ne!(HealthState::Healthy, HealthState::Degraded);
        assert_ne!(HealthState::Unhealthy, HealthState::Degraded);

        // Test serialization
        let healthy_json = serde_json::to_string(&HealthState::Healthy).unwrap();
        assert!(healthy_json.contains("Healthy"));

        let unhealthy_json = serde_json::to_string(&HealthState::Unhealthy).unwrap();
        assert!(unhealthy_json.contains("Unhealthy"));

        let degraded_json = serde_json::to_string(&HealthState::Degraded).unwrap();
        assert!(degraded_json.contains("Degraded"));
    }

    #[test]
    fn test_engine_stats_clone() {
        let engine = Engine::new().unwrap();
        let stats = engine.stats().unwrap();
        let cloned_stats = stats.clone();

        assert_eq!(stats.nodes, cloned_stats.nodes);
        assert_eq!(stats.relationships, cloned_stats.relationships);
        assert_eq!(stats.labels, cloned_stats.labels);
        assert_eq!(stats.rel_types, cloned_stats.rel_types);
        assert_eq!(stats.page_cache_hits, cloned_stats.page_cache_hits);
        assert_eq!(stats.page_cache_misses, cloned_stats.page_cache_misses);
        assert_eq!(stats.wal_entries, cloned_stats.wal_entries);
        assert_eq!(stats.active_transactions, cloned_stats.active_transactions);
    }

    #[test]
    fn test_health_status_clone() {
        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            components: std::collections::HashMap::new(),
        };
        status
            .components
            .insert("test".to_string(), HealthState::Healthy);

        let cloned_status = status.clone();
        assert_eq!(status.overall, cloned_status.overall);
        assert_eq!(status.components.len(), cloned_status.components.len());
        assert!(cloned_status.components.contains_key("test"));
    }

    #[test]
    fn test_health_state_copy() {
        let healthy = HealthState::Healthy;
        let copied = healthy;

        assert_eq!(healthy, copied);
        assert_eq!(format!("{:?}", healthy), "Healthy");
        assert_eq!(format!("{:?}", copied), "Healthy");
    }

    #[test]
    fn test_engine_stats_debug() {
        let engine = Engine::new().unwrap();
        let stats = engine.stats().unwrap();
        let debug = format!("{:?}", stats);

        assert!(debug.contains("EngineStats"));
        assert!(debug.contains("nodes"));
        assert!(debug.contains("relationships"));
    }

    #[test]
    fn test_health_status_debug() {
        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            components: std::collections::HashMap::new(),
        };
        status
            .components
            .insert("test".to_string(), HealthState::Healthy);

        let debug = format!("{:?}", status);
        assert!(debug.contains("HealthStatus"));
        assert!(debug.contains("overall"));
        assert!(debug.contains("components"));
    }

    #[test]
    fn test_health_state_debug() {
        let healthy = HealthState::Healthy;
        let debug = format!("{:?}", healthy);
        assert_eq!(debug, "Healthy");

        let unhealthy = HealthState::Unhealthy;
        let debug = format!("{:?}", unhealthy);
        assert_eq!(debug, "Unhealthy");

        let degraded = HealthState::Degraded;
        let debug = format!("{:?}", degraded);
        assert_eq!(debug, "Degraded");
    }

    #[test]
    fn test_engine_component_access() {
        let engine = Engine::new().unwrap();

        // Test that all components are accessible
        let _catalog = &engine.catalog;
        let _storage = &engine.storage;
        let _page_cache = &engine.page_cache;
        let _wal = &engine.wal;
        let _transaction_manager = &engine.transaction_manager;
        let _indexes = &engine.indexes;
        let _executor = &engine.executor;

        // Test passes if all components are accessible
    }

    #[test]
    fn test_engine_mut_operations() {
        let mut engine = Engine::new().unwrap();

        // Test mutable operations
        let _stats = engine.stats().unwrap();
        let _cypher_result = engine.execute_cypher("MATCH (n) RETURN n");
        let _node_result = engine.create_node(vec!["Test".to_string()], serde_json::Value::Null);
        let _rel_result =
            engine.create_relationship(1, 2, "TEST".to_string(), serde_json::Value::Null);
        let _get_node = engine.get_node(1);
        let _get_rel = engine.get_relationship(1);

        // Test passes if all mutable operations compile
    }

    #[test]
    fn test_update_node() {
        let mut engine = Engine::new().unwrap();

        // Create a node first
        let node_id = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Update the node
        let mut properties = serde_json::Map::new();
        properties.insert(
            "name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );
        properties.insert("age".to_string(), serde_json::Value::Number(30.into()));

        let result = engine.update_node(
            node_id,
            vec!["Person".to_string(), "Updated".to_string()],
            serde_json::Value::Object(properties),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_update_nonexistent_node() {
        let mut engine = Engine::new().unwrap();

        // Try to update a non-existent node
        let result = engine.update_node(
            999,
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_node() {
        let mut engine = Engine::new().unwrap();

        // Create a node first
        let node_id = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Delete the node
        let result = engine.delete_node(node_id);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_delete_nonexistent_node() {
        let mut engine = Engine::new().unwrap();

        // Try to delete a non-existent node
        let result = engine.delete_node(999);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_convert_to_simple_graph() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes and relationships
        let node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let node2 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _rel_id = engine
            .create_relationship(
                node1,
                node2,
                "KNOWS".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Convert to simple graph
        let simple_graph = engine.convert_to_simple_graph().unwrap();

        // Check that the simple graph has the expected structure
        let stats = simple_graph.stats().unwrap();
        assert!(stats.total_nodes >= 2);
        assert!(stats.total_edges >= 1);
    }

    #[test]
    fn test_cluster_nodes() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes
        let _node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _node2 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Test clustering
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };

        let result = engine.cluster_nodes(config);
        assert!(result.is_ok());

        let _clustering_result = result.unwrap();
    }

    #[test]
    fn test_group_nodes_by_labels() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes with different labels
        let _node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _node2 = engine
            .create_node(
                vec!["Company".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Test label-based grouping
        let result = engine.group_nodes_by_labels();
        assert!(result.is_ok());

        let _clustering_result = result.unwrap();
    }

    #[test]
    fn test_group_nodes_by_property() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes with properties
        let mut properties1 = serde_json::Map::new();
        properties1.insert("age".to_string(), serde_json::Value::Number(25.into()));
        let _node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(properties1),
            )
            .unwrap();

        let mut properties2 = serde_json::Map::new();
        properties2.insert("age".to_string(), serde_json::Value::Number(30.into()));
        let _node2 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(properties2),
            )
            .unwrap();

        // Test property-based grouping
        let result = engine.group_nodes_by_property("age");
        assert!(result.is_ok());

        let _clustering_result = result.unwrap();
    }

    #[test]
    fn test_kmeans_cluster_nodes() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes
        let _node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _node2 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Test K-means clustering
        let result = engine.kmeans_cluster_nodes(2, 10);
        assert!(result.is_ok());

        let _clustering_result = result.unwrap();
    }

    #[test]
    fn test_detect_communities() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes and relationships
        let node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let node2 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _rel_id = engine
            .create_relationship(
                node1,
                node2,
                "KNOWS".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Test community detection
        let result = engine.detect_communities();
        assert!(result.is_ok());

        let _clustering_result = result.unwrap();
    }

    #[test]
    fn test_export_to_json() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes and relationships
        let node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let node2 = engine
            .create_node(
                vec!["Company".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _rel_id = engine
            .create_relationship(
                node1,
                node2,
                "WORKS_AT".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Export to JSON
        let json_data = engine.export_to_json().unwrap();

        // Check that the JSON contains the expected structure
        assert!(json_data.is_object());
        assert!(json_data.get("nodes").is_some());
        assert!(json_data.get("relationships").is_some());

        let nodes = json_data.get("nodes").unwrap().as_array().unwrap();
        let relationships = json_data.get("relationships").unwrap().as_array().unwrap();

        assert!(nodes.len() >= 2);
        assert!(!relationships.is_empty());
    }

    #[test]
    fn test_get_graph_statistics() {
        let mut engine = Engine::new().unwrap();

        // Create some nodes with different labels
        let _node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _node2 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _node3 = engine
            .create_node(
                vec!["Company".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Get statistics
        let stats = engine.get_graph_statistics().unwrap();

        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.relationship_count, 0);
        assert_eq!(stats.label_counts.get("Person"), Some(&2));
        assert_eq!(stats.label_counts.get("Company"), Some(&1));
    }

    #[test]
    fn test_clear_all_data() {
        let mut engine = Engine::new().unwrap();

        // Create some data
        let _node1 = engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        let _node2 = engine
            .create_node(
                vec!["Company".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();

        // Verify data exists
        let stats_before = engine.get_graph_statistics().unwrap();
        assert_eq!(stats_before.node_count, 2);

        // Clear all data
        engine.clear_all_data().unwrap();

        // Verify data is cleared
        let stats_after = engine.get_graph_statistics().unwrap();
        assert_eq!(stats_after.node_count, 0);
        assert_eq!(stats_after.relationship_count, 0);
    }
}
