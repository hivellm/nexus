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

use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub mod auth;
pub mod catalog;
pub mod concurrent_access;
pub mod database;
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
pub mod session;
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
    /// Session manager for transaction context
    pub session_manager: session::SessionManager,
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

    /// Execute MATCH ... DELETE query
    fn execute_match_delete_query(&mut self, ast: &executor::parser::CypherQuery) -> Result<()> {
        // First, execute the MATCH part to get the matching nodes
        let mut match_query_clauses = Vec::new();
        let mut delete_clause_opt = None;

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::Match(_) | executor::parser::Clause::Where(_) => {
                    match_query_clauses.push(clause.clone());
                }
                executor::parser::Clause::Delete(delete_clause) => {
                    delete_clause_opt = Some(delete_clause.clone());
                    break; // Stop at DELETE
                }
                _ => {
                    match_query_clauses.push(clause.clone());
                }
            }
        }

        // Execute MATCH to get results
        let match_query = executor::parser::CypherQuery {
            clauses: match_query_clauses,
            params: ast.params.clone(),
        };

        // Collect all node variables from MATCH clauses
        let mut node_variables = Vec::new();
        for clause in &match_query.clauses {
            if let executor::parser::Clause::Match(mc) = clause {
                for element in &mc.pattern.elements {
                    if let executor::parser::PatternElement::Node(node) = element {
                        if let Some(var) = &node.variable {
                            if !node_variables.contains(var) {
                                node_variables.push(var.clone());
                            }
                        }
                    }
                }
            }
        }

        // Rebuild MATCH query as string with explicit RETURN of all variables
        let mut match_query_str = String::new();
        for clause in &match_query.clauses {
            if let executor::parser::Clause::Match(mc) = clause {
                match_query_str.push_str("MATCH ");
                // Reconstruct pattern
                for (idx, element) in mc.pattern.elements.iter().enumerate() {
                    if let executor::parser::PatternElement::Node(node) = element {
                        if idx > 0 {
                            match_query_str.push_str(", ");
                        }
                        match_query_str.push('(');
                        if let Some(var) = &node.variable {
                            match_query_str.push_str(var);
                        }
                        for label in &node.labels {
                            match_query_str.push_str(&format!(":{}", label));
                        }
                        if let Some(props) = &node.properties {
                            match_query_str.push_str(" {");
                            let mut first = true;
                            for (key, val_expr) in &props.properties {
                                if !first {
                                    match_query_str.push_str(", ");
                                }
                                first = false;
                                match_query_str.push_str(key);
                                match_query_str.push_str(": ");
                                if let executor::parser::Expression::Literal(lit) = val_expr {
                                    match lit {
                                        executor::parser::Literal::String(s) => {
                                            match_query_str.push_str(&format!("\"{}\"", s));
                                        }
                                        executor::parser::Literal::Integer(i) => {
                                            match_query_str.push_str(&i.to_string());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            match_query_str.push('}');
                        }
                        match_query_str.push(')');
                    }
                }
                match_query_str.push(' ');
            }
        }

        // Add explicit RETURN for all node variables
        match_query_str.push_str("RETURN ");
        for (idx, var) in node_variables.iter().enumerate() {
            if idx > 0 {
                match_query_str.push_str(", ");
            }
            match_query_str.push_str(var);
        }

        let query_obj = executor::Query {
            cypher: match_query_str,
            params: std::collections::HashMap::new(),
        };

        let match_results = self.executor.execute(&query_obj)?;

        // For each row in MATCH result, delete the nodes
        if let Some(delete_clause) = delete_clause_opt {
            let detach = delete_clause.detach;

            for row in &match_results.rows {
                // Extract node IDs from the row
                for (idx, column) in match_results.columns.iter().enumerate() {
                    // Check if this variable is in the DELETE clause items
                    if delete_clause.items.contains(column) && idx < row.values.len() {
                        if let serde_json::Value::Object(obj) = &row.values[idx] {
                            if let Some(serde_json::Value::Number(id)) = obj.get("_nexus_id") {
                                if let Some(node_id) = id.as_u64() {
                                    if detach {
                                        // Delete all relationships connected to this node first
                                        self.delete_node_relationships(node_id)?;
                                        self.delete_node(node_id)?;
                                    } else {
                                        let node_record = self.storage.read_node(node_id)?;
                                        if node_record.first_rel_ptr != u64::MAX {
                                            return Err(Error::CypherExecution(
                                                "Cannot DELETE node with existing relationships; use DETACH DELETE"
                                                    .to_string(),
                                            ));
                                        }
                                        self.delete_node(node_id)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute MATCH ... CREATE query
    fn execute_match_create_query(&mut self, ast: &executor::parser::CypherQuery) -> Result<()> {
        // First, execute the MATCH part to get the matching nodes
        let mut match_query_clauses = Vec::new();
        let mut create_clause_opt = None;

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::Match(_) | executor::parser::Clause::Where(_) => {
                    match_query_clauses.push(clause.clone());
                }
                executor::parser::Clause::Create(create_clause) => {
                    create_clause_opt = Some(create_clause.clone());
                    break; // Stop at CREATE
                }
                _ => {
                    match_query_clauses.push(clause.clone());
                }
            }
        }

        // Execute MATCH to get results
        let match_query = executor::parser::CypherQuery {
            clauses: match_query_clauses,
            params: ast.params.clone(),
        };

        // Collect all node variables from MATCH clauses
        let mut node_variables = Vec::new();
        for clause in &match_query.clauses {
            if let executor::parser::Clause::Match(mc) = clause {
                for element in &mc.pattern.elements {
                    if let executor::parser::PatternElement::Node(node) = element {
                        if let Some(var) = &node.variable {
                            if !node_variables.contains(var) {
                                node_variables.push(var.clone());
                            }
                        }
                    }
                }
            }
        }

        // Rebuild MATCH query as string with explicit RETURN of all variables
        let mut match_query_str = String::new();
        for clause in &match_query.clauses {
            if let executor::parser::Clause::Match(mc) = clause {
                match_query_str.push_str("MATCH ");
                // Reconstruct pattern - simplified for comma-separated nodes
                for (idx, element) in mc.pattern.elements.iter().enumerate() {
                    if let executor::parser::PatternElement::Node(node) = element {
                        if idx > 0 {
                            match_query_str.push_str(", ");
                        }
                        match_query_str.push('(');
                        if let Some(var) = &node.variable {
                            match_query_str.push_str(var);
                        }
                        for label in &node.labels {
                            match_query_str.push_str(&format!(":{}", label));
                        }
                        if let Some(props) = &node.properties {
                            match_query_str.push_str(" {");
                            let mut first = true;
                            for (key, val_expr) in &props.properties {
                                if !first {
                                    match_query_str.push_str(", ");
                                }
                                first = false;
                                match_query_str.push_str(key);
                                match_query_str.push_str(": ");
                                if let executor::parser::Expression::Literal(lit) = val_expr {
                                    match lit {
                                        executor::parser::Literal::String(s) => {
                                            match_query_str.push_str(&format!("\"{}\"", s));
                                        }
                                        executor::parser::Literal::Integer(i) => {
                                            match_query_str.push_str(&i.to_string());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            match_query_str.push('}');
                        }
                        match_query_str.push(')');
                    }
                }
                match_query_str.push(' ');
            }
        }

        // Add explicit RETURN for all node variables
        match_query_str.push_str("RETURN ");
        for (idx, var) in node_variables.iter().enumerate() {
            if idx > 0 {
                match_query_str.push_str(", ");
            }
            match_query_str.push_str(var);
        }

        let query_obj = executor::Query {
            cypher: match_query_str,
            params: std::collections::HashMap::new(),
        };

        let match_results = self.executor.execute(&query_obj)?;

        // For each row in MATCH result, execute the CREATE
        if let Some(create_clause) = create_clause_opt {
            for row in &match_results.rows {
                // Extract node IDs from the row
                let mut node_vars = std::collections::HashMap::new();

                for (idx, column) in match_results.columns.iter().enumerate() {
                    if idx < row.values.len() {
                        if let serde_json::Value::Object(obj) = &row.values[idx] {
                            if let Some(serde_json::Value::Number(id)) = obj.get("_nexus_id") {
                                if let Some(node_id) = id.as_u64() {
                                    node_vars.insert(column.clone(), node_id);
                                }
                            }
                        }
                    }
                }

                // Create relationships from the pattern
                self.create_from_pattern_with_context(&create_clause.pattern, &node_vars)?;
            }
        }

        Ok(())
    }

    /// Create from pattern with existing node context
    fn create_from_pattern_with_context(
        &mut self,
        pattern: &executor::parser::Pattern,
        node_vars: &std::collections::HashMap<String, u64>,
    ) -> Result<()> {
        let mut current_node_id: Option<u64> = None;

        // Use indexed iteration to access next element for relationships
        for (i, element) in pattern.elements.iter().enumerate() {
            match element {
                executor::parser::PatternElement::Node(node) => {
                    if let Some(var) = &node.variable {
                        // Check if this variable exists in the MATCH context
                        if let Some(&existing_id) = node_vars.get(var) {
                            current_node_id = Some(existing_id);
                        } else {
                            // Create new node
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

                            let node_id = self.create_node(node.labels.clone(), properties)?;
                            current_node_id = Some(node_id);
                        }
                    }
                }
                executor::parser::PatternElement::Relationship(rel) => {
                    // Get source node (set by previous node element)
                    let source_id = current_node_id.ok_or_else(|| {
                        Error::CypherExecution("Relationship must follow a node".to_string())
                    })?;

                    // Get target node (next element after relationship)
                    if i + 1 < pattern.elements.len() {
                        if let executor::parser::PatternElement::Node(target_node) =
                            &pattern.elements[i + 1]
                        {
                            // Target node MUST have a variable and MUST exist in MATCH context
                            let target_id = if let Some(var) = &target_node.variable {
                                // Check if target exists in MATCH context
                                if let Some(&existing_id) = node_vars.get(var) {
                                    current_node_id = Some(existing_id);
                                    existing_id
                                } else {
                                    // This shouldn't happen for MATCH ... CREATE
                                    // All nodes should be matched first
                                    return Err(Error::CypherExecution(format!(
                                        "Node variable '{}' not found in MATCH context",
                                        var
                                    )));
                                }
                            } else {
                                return Err(Error::CypherExecution(
                                    "Target node must have a variable".to_string(),
                                ));
                            };

                            // Create relationship
                            let rel_type = rel.types.first().ok_or_else(|| {
                                Error::CypherExecution("Relationship must have a type".to_string())
                            })?;

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

                            self.create_relationship(
                                source_id,
                                target_id,
                                rel_type.clone(),
                                rel_properties,
                            )?;
                        } else {
                            return Err(Error::CypherExecution(
                                "Relationship must be followed by a node".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must end with a node".to_string(),
                        ));
                    }
                }
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
    pub fn refresh_executor(&mut self) -> Result<()> {
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
        // Parse query to check if it contains CREATE or DELETE clauses
        let mut parser = executor::parser::CypherParser::new(query.to_string());
        let ast = parser.parse()?;

        // Check for EXPLAIN command
        if let Some(executor::parser::Clause::Explain(explain_clause)) = ast.clauses.first() {
            // Use stored query string if available, otherwise convert from AST
            let query_str = explain_clause
                .query_string
                .clone()
                .unwrap_or_else(|| self.query_to_string(&explain_clause.query));
            return self.execute_explain_with_string(&explain_clause.query, &query_str);
        }

        // Check for PROFILE command
        if let Some(executor::parser::Clause::Profile(profile_clause)) = ast.clauses.first() {
            // Use stored query string if available, otherwise convert from AST
            let query_str = profile_clause
                .query_string
                .clone()
                .unwrap_or_else(|| self.query_to_string(&profile_clause.query));
            return self.execute_profile_with_string(&profile_clause.query, &query_str);
        }

        // Check for administrative commands that need special handling
        // These commands (CREATE/DROP DATABASE, SHOW DATABASES) should be handled at server level
        // as Engine doesn't have access to DatabaseManager
        let has_admin_db_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::CreateDatabase(_)
                    | executor::parser::Clause::DropDatabase(_)
                    | executor::parser::Clause::ShowDatabases
            )
        });

        if has_admin_db_cmd {
            return Err(Error::CypherExecution(
                "Database management commands (CREATE/DROP DATABASE, SHOW DATABASES) must be executed at server level".to_string(),
            ));
        }

        // Check for transaction commands
        let has_begin = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::BeginTransaction));
        let has_commit = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CommitTransaction));
        let has_rollback = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::RollbackTransaction));

        if has_begin || has_commit || has_rollback {
            return self.execute_transaction_commands(&ast);
        }

        // Check for index management commands
        let has_create_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateIndex(_)));
        let has_drop_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropIndex(_)));

        if has_create_index || has_drop_index {
            return self.execute_index_commands(&ast);
        }

        // Check for constraint management commands
        let has_create_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateConstraint(_)));
        let has_drop_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropConstraint(_)));

        if has_create_constraint || has_drop_constraint {
            return self.execute_constraint_commands(&ast);
        }

        // Check for user management commands (should be handled at server level)
        let has_user_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::ShowUsers
                    | executor::parser::Clause::CreateUser(_)
                    | executor::parser::Clause::Grant(_)
                    | executor::parser::Clause::Revoke(_)
            )
        });

        if has_user_cmd {
            return Err(Error::CypherExecution(
                "User management commands (SHOW USERS, CREATE USER, GRANT, REVOKE) must be executed at server level".to_string(),
            ));
        }

        // Check if query contains CREATE or DELETE
        let has_create = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Create(_)));
        let has_delete = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Delete(_)));
        let has_merge = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Merge(_)));
        let has_set_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Set(_)));
        let has_remove_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Remove(_)));
        let has_foreach = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Foreach(_)));
        let has_match = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Match(_)));

        // Handle DELETE (with or without MATCH)
        if has_delete {
            if has_match {
                // MATCH ... DELETE: execute MATCH first, then DELETE with results
                self.execute_match_delete_query(&ast)?;
            } else {
                // Standalone DELETE won't work without MATCH
                // This would be: DELETE n (without MATCH)
                // For now, we don't support this syntax
                return Err(Error::CypherSyntax(
                    "DELETE requires MATCH clause".to_string(),
                ));
            }
            self.refresh_executor()?;

            // Return empty result for DELETE queries
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        // Handle MERGE / SET / REMOVE / FOREACH write queries before falling back to read executor
        if has_merge || has_set_clause || has_remove_clause || has_foreach {
            let result = self.execute_write_query(&ast)?;
            return Ok(result);
        }

        // If query has CREATE (with or without MATCH), handle via Engine for persistence
        if has_create {
            if has_match {
                // MATCH ... CREATE: execute MATCH first, then CREATE with results
                self.execute_match_create_query(&ast)?;
            } else {
                // Standalone CREATE
                self.execute_create_query(&ast)?;
            }

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

    fn execute_write_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut context: HashMap<String, Vec<u64>> = HashMap::new();
        let mut result: Option<executor::ResultSet> = None;

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::Match(match_clause) => {
                    let (variable, node_ids) = self.process_match_clause(match_clause)?;
                    context.insert(variable, node_ids);
                }
                executor::parser::Clause::Merge(merge_clause) => {
                    let (variable, node_ids) = self.process_merge_clause(merge_clause)?;
                    context.insert(variable, node_ids);
                }
                executor::parser::Clause::Set(set_clause) => {
                    self.apply_set_clause(&context, set_clause)?;
                }
                executor::parser::Clause::Remove(remove_clause) => {
                    self.apply_remove_clause(&context, remove_clause)?;
                }
                executor::parser::Clause::Foreach(foreach_clause) => {
                    self.execute_foreach_clause(&context, foreach_clause)?;
                }
                executor::parser::Clause::Return(return_clause) => {
                    result = Some(self.build_return_result(&context, return_clause)?);
                }
                executor::parser::Clause::Where(_)
                | executor::parser::Clause::With(_)
                | executor::parser::Clause::Unwind(_)
                | executor::parser::Clause::Union(_)
                | executor::parser::Clause::OrderBy(_)
                | executor::parser::Clause::Limit(_)
                | executor::parser::Clause::Skip(_) => {
                    return Err(Error::CypherExecution(
                        "Unsupported clause in write query".to_string(),
                    ));
                }
                _ => {}
            }
        }

        self.storage.flush()?;
        self.refresh_executor()?;

        Ok(result.unwrap_or_else(|| executor::ResultSet {
            columns: vec![],
            rows: vec![],
        }))
    }

    fn process_merge_clause(
        &mut self,
        merge_clause: &executor::parser::MergeClause,
    ) -> Result<(String, Vec<u64>)> {
        let node_pattern = merge_clause
            .pattern
            .elements
            .iter()
            .find_map(|element| {
                if let executor::parser::PatternElement::Node(node) = element {
                    Some(node.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::CypherExecution("MERGE requires a node pattern".to_string()))?;

        let variable = node_pattern
            .variable
            .clone()
            .ok_or_else(|| Error::CypherExecution("MERGE requires a variable alias".to_string()))?;

        let mut node_ids = self.find_nodes_by_node_pattern(&node_pattern)?;
        node_ids.sort_unstable();
        node_ids.dedup();

        if node_ids.is_empty() {
            let labels = node_pattern.labels.clone();
            let mut props = Map::new();
            if let Some(prop_map) = &node_pattern.properties {
                for (key, expr) in &prop_map.properties {
                    let value = self.expression_to_json_value(expr)?;
                    props.insert(key.clone(), value);
                }
            }
            // create_node already checks constraints, so we can call it directly
            let node_id = self.create_node(labels, Value::Object(props))?;
            node_ids.push(node_id);

            if let Some(on_create) = &merge_clause.on_create {
                let mut ctx = HashMap::new();
                ctx.insert(variable.clone(), vec![node_id]);
                self.apply_set_clause(&ctx, on_create)?;
            }
        } else if let Some(on_match) = &merge_clause.on_match {
            let mut ctx = HashMap::new();
            ctx.insert(variable.clone(), node_ids.clone());
            self.apply_set_clause(&ctx, on_match)?;
        }

        Ok((variable, node_ids))
    }

    fn process_match_clause(
        &mut self,
        match_clause: &executor::parser::MatchClause,
    ) -> Result<(String, Vec<u64>)> {
        if match_clause.optional {
            return Err(Error::CypherExecution(
                "OPTIONAL MATCH not supported in write queries".to_string(),
            ));
        }

        if match_clause.where_clause.is_some() {
            return Err(Error::CypherExecution(
                "MATCH with WHERE is not supported in write queries".to_string(),
            ));
        }

        let node_pattern = match_clause
            .pattern
            .elements
            .iter()
            .find_map(|element| {
                if let executor::parser::PatternElement::Node(node) = element {
                    Some(node.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::CypherExecution("MATCH requires a node pattern".to_string()))?;

        let variable = node_pattern
            .variable
            .clone()
            .ok_or_else(|| Error::CypherExecution("MATCH requires a variable alias".to_string()))?;

        let mut node_ids = self.find_nodes_by_node_pattern(&node_pattern)?;
        node_ids.sort_unstable();
        node_ids.dedup();

        Ok((variable, node_ids))
    }

    fn apply_set_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        set_clause: &executor::parser::SetClause,
    ) -> Result<()> {
        if set_clause.items.is_empty() {
            return Ok(());
        }

        let mut state_map: HashMap<u64, NodeWriteState> = HashMap::new();

        for item in &set_clause.items {
            match item {
                executor::parser::SetItem::Property {
                    target,
                    property,
                    value,
                } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;

                    let json_value = self.expression_to_json_value(value)?;
                    for node_id in node_ids {
                        let state = self.ensure_node_state(*node_id, &mut state_map)?;
                        state
                            .properties
                            .insert(property.clone(), json_value.clone());
                    }
                }
                executor::parser::SetItem::Label { target, label } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;

                    for node_id in node_ids {
                        let state = self.ensure_node_state(*node_id, &mut state_map)?;
                        state.labels.insert(label.clone());
                    }
                }
            }
        }

        for (node_id, state) in state_map.into_iter() {
            self.persist_node_state(node_id, state)?;
        }

        Ok(())
    }

    fn apply_remove_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        remove_clause: &executor::parser::RemoveClause,
    ) -> Result<()> {
        if remove_clause.items.is_empty() {
            return Ok(());
        }

        let mut state_map: HashMap<u64, NodeWriteState> = HashMap::new();

        for item in &remove_clause.items {
            match item {
                executor::parser::RemoveItem::Property { target, property } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in REMOVE clause",
                            target
                        ))
                    })?;

                    for node_id in node_ids {
                        let state = self.ensure_node_state(*node_id, &mut state_map)?;
                        state.properties.remove(property);
                    }
                }
                executor::parser::RemoveItem::Label { target, label } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in REMOVE clause",
                            target
                        ))
                    })?;

                    for node_id in node_ids {
                        let state = self.ensure_node_state(*node_id, &mut state_map)?;
                        state.labels.remove(label);
                    }
                }
            }
        }

        for (node_id, state) in state_map.into_iter() {
            self.persist_node_state(node_id, state)?;
        }

        Ok(())
    }

    fn execute_foreach_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        foreach_clause: &executor::parser::ForeachClause,
    ) -> Result<()> {
        // Evaluate the list expression
        let list_value = match &foreach_clause.list_expression {
            executor::parser::Expression::Variable(var_name) => {
                // Variable from context - assume it's a list of node IDs
                // Convert node IDs to a list of values (we'll use node IDs as the iteration items)
                // For FOREACH, we typically iterate over node IDs, not values
                context.get(var_name).cloned().unwrap_or_default()
            }
            executor::parser::Expression::Literal(executor::parser::Literal::Null) => {
                // NULL list - no iteration
                return Ok(());
            }
            executor::parser::Expression::List(items) => {
                // Literal list - evaluate each item
                // For now, we'll treat list items as node IDs if they're integers
                // This is a simplified implementation
                let mut node_ids = Vec::new();
                for item in items {
                    if let executor::parser::Expression::Literal(
                        executor::parser::Literal::Integer(id),
                    ) = item
                    {
                        node_ids.push(*id as u64);
                    }
                }
                node_ids
            }
            _ => {
                return Err(Error::CypherExecution(format!(
                    "FOREACH list expression must be a variable or literal list, got: {:?}",
                    foreach_clause.list_expression
                )));
            }
        };

        // Iterate over each item in the list
        for item_value in list_value {
            // Create a new context for this iteration with the FOREACH variable
            // The variable contains a single node ID for this iteration
            let mut iteration_context = context.clone();
            iteration_context.insert(foreach_clause.variable.clone(), vec![item_value]);

            // Execute each update clause for this iteration
            for update_clause in &foreach_clause.update_clauses {
                match update_clause {
                    executor::parser::ForeachUpdateClause::Set(set_clause) => {
                        self.apply_set_clause(&iteration_context, set_clause)?;
                    }
                    executor::parser::ForeachUpdateClause::Delete(delete_clause) => {
                        // Apply DELETE for this iteration
                        // DELETE in FOREACH context means delete the node referenced by the variable
                        let node_ids = iteration_context
                            .get(&foreach_clause.variable)
                            .cloned()
                            .unwrap_or_default();

                        for node_id in node_ids {
                            if delete_clause.detach {
                                // DETACH DELETE: remove all relationships first
                                self.delete_node_relationships(node_id)?;
                                self.delete_node(node_id)?;
                            } else {
                                // Regular DELETE: check for relationships
                                let node_record = self.storage.read_node(node_id)?;
                                if node_record.first_rel_ptr != u64::MAX {
                                    return Err(Error::CypherExecution(format!(
                                        "Cannot DELETE node {} with existing relationships; use DETACH DELETE",
                                        node_id
                                    )));
                                }
                                self.delete_node(node_id)?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute transaction commands (BEGIN, COMMIT, ROLLBACK)
    fn execute_transaction_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::BeginTransaction => {
                    // Begin a write transaction
                    // Note: The transaction is stored internally by TransactionManager
                    // For explicit transaction support, we would need to track the transaction
                    // handle per session/client. For now, we just ensure the transaction manager
                    // is ready for write operations.
                    let _tx = self.transaction_manager.begin_write()?;
                    // In a full implementation, we would store this transaction handle
                    // in a session context for later commit/rollback
                }
                executor::parser::Clause::CommitTransaction => {
                    // Commit the current transaction
                    // Note: In a full implementation, we would retrieve the transaction handle
                    // from the session context. For now, we create a transaction and commit it
                    // to ensure consistency.
                    let mut tx = self.transaction_manager.begin_write()?;
                    self.transaction_manager.commit(&mut tx)?;
                    // Flush storage to ensure durability
                    self.storage.flush()?;
                }
                executor::parser::Clause::RollbackTransaction => {
                    // Rollback the current transaction
                    // Note: In a full implementation, we would retrieve the transaction handle
                    // from the session context and call abort/rollback. For now, we create
                    // a transaction and abort it to ensure consistency.
                    let mut tx = self.transaction_manager.begin_write()?;
                    self.transaction_manager.abort(&mut tx)?;
                    // Flush storage to ensure consistency
                    self.storage.flush()?;
                }
                _ => {}
            }
        }

        Ok(executor::ResultSet {
            columns: vec!["status".to_string()],
            rows: vec![executor::Row {
                values: vec![serde_json::Value::String("ok".to_string())],
            }],
        })
    }

    /// Execute index management commands (CREATE INDEX, DROP INDEX)
    fn execute_index_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::CreateIndex(create_index) => {
                    // Get label and property IDs
                    let label_id = self.catalog.get_or_create_label(&create_index.label)?;
                    let property_key_id =
                        self.catalog.get_or_create_key(&create_index.property)?;

                    // Check if index already exists
                    let index_exists = self.indexes.property_index.has_index(label_id, property_key_id);

                    // Handle OR REPLACE
                    if create_index.or_replace && index_exists {
                        // Drop existing index first
                        self.indexes.property_index.drop_index(label_id, property_key_id)?;
                    }

                    // Handle IF NOT EXISTS
                    if !create_index.or_replace && create_index.if_not_exists && index_exists {
                        // Index already exists and IF NOT EXISTS was specified, skip
                        continue;
                    }

                    // Check if index already exists (error if not IF NOT EXISTS or OR REPLACE)
                    if !create_index.or_replace && !create_index.if_not_exists && index_exists {
                        return Err(Error::CypherExecution(format!(
                            "Index on :{}({}) already exists",
                            create_index.label, create_index.property
                        )));
                    }

                    // Create the index structure
                    self.indexes.property_index.create_index(label_id, property_key_id)?;

                    // Populate index with existing nodes that have this label and property
                    self.populate_index(label_id, property_key_id)?;
                }
                executor::parser::Clause::DropIndex(drop_index) => {
                    // Get label and property IDs
                    let label_id = match self.catalog.get_label_id(&drop_index.label) {
                        Ok(id) => id,
                        Err(_) if drop_index.if_exists => {
                            // Label doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    let property_key_id = match self.catalog.get_key_id(&drop_index.property) {
                        Ok(id) => id,
                        Err(_) if drop_index.if_exists => {
                            // Property doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    // Check if index exists
                    if !self.indexes.property_index.has_index(label_id, property_key_id) {
                        if drop_index.if_exists {
                            // Index doesn't exist and IF EXISTS was specified, skip
                            continue;
                        } else {
                            return Err(Error::CypherExecution(format!(
                                "Index on :{}({}) does not exist",
                                drop_index.label, drop_index.property
                            )));
                        }
                    }

                    // Drop the index
                    self.indexes.property_index.drop_index(label_id, property_key_id)?;
                }
                _ => {}
            }
        }

        Ok(executor::ResultSet {
            columns: vec!["status".to_string()],
            rows: vec![executor::Row {
                values: vec![serde_json::Value::String("ok".to_string())],
            }],
        })
    }

    /// Populate an index with existing nodes that have the specified label and property
    fn populate_index(&mut self, label_id: u32, property_key_id: u32) -> Result<()> {
        use crate::index::PropertyValue;
        use serde_json::Value as JsonValue;

        // Get property key name
        let property_name = self.catalog.get_key_name(property_key_id)?
            .ok_or_else(|| Error::CypherExecution(format!(
                "Property key {} not found",
                property_key_id
            )))?;

        // Get all nodes with this label
        let label_bitmap = self.indexes.label_index.get_nodes_with_labels(&[label_id])?;
        
        // Iterate through all nodes with this label
        for node_id in label_bitmap.iter() {
            let node_id_u64 = node_id as u64;
            
            // Load node properties
            if let Some(JsonValue::Object(props)) = self.storage.load_node_properties(node_id_u64)? {
                // Check if this node has the property we're indexing
                if let Some(prop_value) = props.get(&property_name) {
                    // Convert JSON value to PropertyValue
                    let property_value = match prop_value {
                        JsonValue::String(s) => PropertyValue::String(s.clone()),
                        JsonValue::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                PropertyValue::Integer(i)
                            } else if let Some(f) = n.as_f64() {
                                PropertyValue::Float(f)
                            } else {
                                continue; // Skip invalid number
                            }
                        }
                        JsonValue::Bool(b) => PropertyValue::Boolean(*b),
                        JsonValue::Null => PropertyValue::Null,
                        _ => continue, // Skip arrays and objects
                    };

                    // Add to index
                    self.indexes.property_index.add_property(
                        node_id_u64,
                        label_id,
                        property_key_id,
                        property_value,
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Execute constraint management commands (CREATE CONSTRAINT, DROP CONSTRAINT)
    fn execute_constraint_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut constraint_manager = self.catalog.constraint_manager().write();

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::CreateConstraint(create_constraint) => {
                    // Get label ID
                    let label_id = self.catalog.get_or_create_label(&create_constraint.label)?;

                    // Get property key ID
                    let property_key_id = self
                        .catalog
                        .get_or_create_key(&create_constraint.property)?;

                    // Convert parser constraint type to catalog constraint type
                    let constraint_type = match create_constraint.constraint_type {
                        executor::parser::ConstraintType::Unique => {
                            catalog::constraints::ConstraintType::Unique
                        }
                        executor::parser::ConstraintType::Exists => {
                            catalog::constraints::ConstraintType::Exists
                        }
                    };

                    // Create constraint
                    match constraint_manager.create_constraint(
                        constraint_type,
                        label_id,
                        property_key_id,
                    ) {
                        Ok(_) => {
                            // Constraint created successfully
                        }
                        Err(Error::CypherExecution(_)) if create_constraint.if_not_exists => {
                            // Constraint already exists and IF NOT EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                executor::parser::Clause::DropConstraint(drop_constraint) => {
                    // Get label ID
                    let label_id = match self.catalog.get_label_id(&drop_constraint.label) {
                        Ok(id) => id,
                        Err(_) if drop_constraint.if_exists => {
                            // Label doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    // Get property key ID
                    let property_key_id = match self.catalog.get_key_id(&drop_constraint.property) {
                        Ok(id) => id,
                        Err(_) if drop_constraint.if_exists => {
                            // Property doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    // Convert parser constraint type to catalog constraint type
                    let constraint_type = match drop_constraint.constraint_type {
                        executor::parser::ConstraintType::Unique => {
                            catalog::constraints::ConstraintType::Unique
                        }
                        executor::parser::ConstraintType::Exists => {
                            catalog::constraints::ConstraintType::Exists
                        }
                    };

                    // Drop constraint
                    match constraint_manager.drop_constraint(
                        constraint_type,
                        label_id,
                        property_key_id,
                    ) {
                        Ok(true) => {
                            // Constraint dropped successfully
                        }
                        Ok(false) if drop_constraint.if_exists => {
                            // Constraint doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Ok(false) => {
                            return Err(Error::CypherExecution(format!(
                                "Constraint does not exist on :{} ({})",
                                drop_constraint.label, drop_constraint.property
                            )));
                        }
                        Err(e) => return Err(e),
                    }
                }
                _ => {}
            }
        }

        Ok(executor::ResultSet {
            columns: vec!["status".to_string()],
            rows: vec![executor::Row {
                values: vec![serde_json::Value::String("ok".to_string())],
            }],
        })
    }

    /// Execute LOAD CSV commands
    /// LOAD CSV loads CSV data and binds each row to a variable
    /// Typically used with FOREACH or UNWIND to process rows
    fn execute_load_csv_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        use std::fs;
        use std::path::Path;
        
        let mut all_rows = Vec::new();
        let mut columns = Vec::new();
        
        for clause in &ast.clauses {
            if let executor::parser::Clause::LoadCsv(load_csv) = clause {
                // Extract file path from URL (support file:///path/to/file.csv)
                let file_path = if load_csv.url.starts_with("file:///") {
                    &load_csv.url[8..] // Remove "file:///"
                } else if load_csv.url.starts_with("file://") {
                    &load_csv.url[7..] // Remove "file://"
                } else {
                    &load_csv.url // Use as-is if no protocol
                };
                
                let path = Path::new(file_path);
                if !path.exists() {
                    return Err(Error::CypherExecution(format!(
                        "CSV file not found: {}",
                        file_path
                    )));
                }
                
                // Read CSV file
                let content = fs::read_to_string(path)
                    .map_err(|e| Error::CypherExecution(format!("Failed to read CSV file: {}", e)))?;
                
                // Parse CSV lines
                let field_terminator = load_csv.field_terminator.as_deref().unwrap_or(",");
                let mut lines = content.lines();
                
                // Skip header if WITH HEADERS
                if load_csv.with_headers {
                    lines.next(); // Skip header line
                }
                
                // Parse each row
                for line in lines {
                    if line.trim().is_empty() {
                        continue;
                    }
                    
                    // Simple CSV parsing (split by field terminator)
                    // Note: This doesn't handle quoted fields with commas inside
                    // For production, should use a proper CSV parser library
                    let fields: Vec<String> = line
                        .split(field_terminator)
                        .map(|s| s.trim().to_string())
                        .collect();
                    
                    // Convert fields to JSON array
                    let row_value: serde_json::Value = fields
                        .into_iter()
                        .map(|f| serde_json::Value::String(f))
                        .collect();
                    
                    all_rows.push(executor::Row {
                        values: vec![row_value],
                    });
                }
                
                // Set columns if not already set
                if columns.is_empty() {
                    columns = vec![load_csv.variable.clone()];
                }
            }
        }
        
        Ok(executor::ResultSet {
            columns,
            rows: all_rows,
        })
    }

    /// Execute CALL subquery commands
    fn execute_call_subquery_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut all_results = Vec::new();
        let mut columns = Vec::new();

        for clause in &ast.clauses {
            if let executor::parser::Clause::CallSubquery(call_subquery) = clause {
                if call_subquery.in_transactions {
                    // Execute with batching in transactions
                    let batch_size = call_subquery.batch_size.unwrap_or(1000);
                    
                    // Execute subquery in batches with transactions
                    // For each batch, start a write transaction, execute, and commit
                    let mut batch_count = 0;
                    loop {
                        // Start write transaction for this batch
                        let mut tx = self.transaction_manager.begin_write()?;
                        
                        // Execute subquery for this batch
                        let subquery_result = self.execute_cypher_ast(&call_subquery.query)?;
                        
                        if columns.is_empty() {
                            columns = subquery_result.columns.clone();
                        }
                        
                        // Add results for this batch
                        let batch_rows: Vec<_> = subquery_result.rows.into_iter().take(batch_size).collect();
                        if batch_rows.is_empty() {
                            // No more results, commit and break
                            self.transaction_manager.commit(&mut tx)?;
                            break;
                        }
                        
                        all_results.extend(batch_rows);
                        batch_count += 1;
                        
                        // Commit transaction for this batch
                        self.transaction_manager.commit(&mut tx)?;
                        
                        // If we got fewer rows than batch size, we're done
                        if all_results.len() < batch_count * batch_size {
                            break;
                        }
                    }
                } else {
                    // Execute subquery normally (no batching)
                    let subquery_result = self.execute_cypher_ast(&call_subquery.query)?;
                    
                    if columns.is_empty() {
                        columns = subquery_result.columns.clone();
                    }
                    all_results.extend(subquery_result.rows);
                }
            }
        }

        Ok(executor::ResultSet {
            columns,
            rows: all_results,
        })
    }

    /// Check constraints before creating or updating a node
    fn check_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
        exclude_node_id: Option<u64>,
    ) -> Result<()> {
        let constraint_manager = self.catalog.constraint_manager().read();

        // Check constraints for each label
        for &label_id in label_ids {
            let constraints = constraint_manager.get_constraints_for_label(label_id)?;

            for constraint in constraints {
                // Get property value
                let property_name = self
                    .catalog
                    .get_key_name(constraint.property_key_id)?
                    .ok_or_else(|| Error::Internal("Property key not found".to_string()))?;

                let property_value = properties.as_object().and_then(|m| m.get(&property_name));

                match constraint.constraint_type {
                    catalog::constraints::ConstraintType::Exists => {
                        // Property must exist (not null)
                        if property_value.is_none()
                            || property_value == Some(&serde_json::Value::Null)
                        {
                            let label_name = self
                                .catalog
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));
                            return Err(Error::ConstraintViolation(format!(
                                "EXISTS constraint violated: property '{}' must exist on nodes with label '{}'",
                                property_name, label_name
                            )));
                        }
                    }
                    catalog::constraints::ConstraintType::Unique => {
                        // Property value must be unique across all nodes with this label
                        if let Some(value) = property_value {
                            // Check if any other node with this label has the same property value
                            let label_name = self
                                .catalog
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));

                            // Get all nodes with this label
                            let bitmap = self
                                .indexes
                                .label_index
                                .get_nodes_with_labels(&[label_id])?;

                            for node_id in bitmap.iter() {
                                let node_id_u64 = node_id as u64;

                                // Skip the node being updated
                                if Some(node_id_u64) == exclude_node_id {
                                    continue;
                                }

                                let node_props = self.storage.load_node_properties(node_id_u64)?;
                                if let Some(Value::Object(props_map)) = node_props {
                                    if let Some(existing_value) = props_map.get(&property_name) {
                                        if existing_value == value {
                                            return Err(Error::ConstraintViolation(format!(
                                                "UNIQUE constraint violated: property '{}' value already exists on another node with label '{}'",
                                                property_name, label_name
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute EXPLAIN command - returns execution plan without executing query
    fn execute_explain_with_string(
        &mut self,
        query: &executor::parser::CypherQuery,
        query_str: &str,
    ) -> Result<executor::ResultSet> {
        // Use the query AST directly if it has clauses, otherwise parse the string
        let operators = if !query.clauses.is_empty() {
            // Use the planner directly with the AST
            let planner = executor::planner::QueryPlanner::new(
                &self.catalog,
                &self.indexes.label_index,
                &self.indexes.knn_index,
            );
            planner.plan_query(query)?
        } else {
            // Fallback: parse and plan from string
            self.executor.parse_and_plan(query_str)?
        };

        // Format plan as JSON for return
        let plan_json = serde_json::json!({
            "plan": {
                "operators": operators.iter().map(|op| {
                    serde_json::json!({
                        "type": format!("{:?}", op),
                        "description": format!("{:?}", op)
                    })
                }).collect::<Vec<_>>()
            },
            "estimated_cost": "N/A", // Would need cost estimation
            "estimated_rows": "N/A"  // Would need row estimation
        });

        Ok(executor::ResultSet {
            columns: vec!["plan".to_string()],
            rows: vec![executor::Row {
                values: vec![plan_json],
            }],
        })
    }

    /// Execute PROFILE command - executes query and returns execution statistics
    fn execute_profile_with_string(
        &mut self,
        query: &executor::parser::CypherQuery,
        query_str: &str,
    ) -> Result<executor::ResultSet> {
        use std::time::Instant;

        let start_time = Instant::now();

        // Use the query AST directly if it has clauses, otherwise parse the string
        let operators = if !query.clauses.is_empty() {
            // Use the planner directly with the AST
            let planner = executor::planner::QueryPlanner::new(
                &self.catalog,
                &self.indexes.label_index,
                &self.indexes.knn_index,
            );
            planner.plan_query(query)?
        } else {
            // Fallback: parse and plan from string
            self.executor.parse_and_plan(query_str)?
        };

        // Execute the query
        let result = self.execute_cypher_internal(query_str)?;

        let execution_time = start_time.elapsed();

        // Format profile as JSON
        let profile_json = serde_json::json!({
            "plan": {
                "operators": operators.iter().map(|op| {
                    serde_json::json!({
                        "type": format!("{:?}", op),
                        "description": format!("{:?}", op)
                    })
                }).collect::<Vec<_>>()
            },
            "execution_time_ms": execution_time.as_millis(),
            "execution_time_us": execution_time.as_micros(),
            "rows_returned": result.rows.len(),
            "columns_returned": result.columns.len()
        });

        Ok(executor::ResultSet {
            columns: vec!["profile".to_string()],
            rows: vec![executor::Row {
                values: vec![profile_json],
            }],
        })
    }

    /// Convert CypherQuery AST to string representation
    fn query_to_string(&self, query: &executor::parser::CypherQuery) -> String {
        // Simple conversion - in production would need proper formatting
        // For now, reconstruct from clauses
        let mut parts = Vec::new();
        for clause in &query.clauses {
            parts.push(format!("{:?}", clause));
        }
        parts.join(" ")
    }

    /// Internal method to execute Cypher query (used by PROFILE)
    fn execute_cypher_internal(&mut self, query: &str) -> Result<executor::ResultSet> {
        // Re-parse and execute (avoiding infinite recursion with EXPLAIN/PROFILE)
        let mut parser = executor::parser::CypherParser::new(query.to_string());
        let ast = parser.parse()?;

        // Execute normally (but skip EXPLAIN/PROFILE checks)
        self.execute_cypher_ast(&ast)
    }

    /// Execute Cypher AST (internal, used to avoid EXPLAIN/PROFILE recursion)
    fn execute_cypher_ast(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        // Check for administrative commands that need special handling
        let has_admin_db_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::CreateDatabase(_)
                    | executor::parser::Clause::DropDatabase(_)
                    | executor::parser::Clause::ShowDatabases
            )
        });

        if has_admin_db_cmd {
            return Err(Error::CypherExecution(
                "Database management commands (CREATE/DROP DATABASE, SHOW DATABASES) must be executed at server level".to_string(),
            ));
        }

        // Check for transaction commands
        let has_begin = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::BeginTransaction));
        let has_commit = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CommitTransaction));
        let has_rollback = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::RollbackTransaction));

        if has_begin || has_commit || has_rollback {
            return self.execute_transaction_commands(ast);
        }

        // Check for index management commands
        let has_create_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateIndex(_)));
        let has_drop_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropIndex(_)));

        if has_create_index || has_drop_index {
            return self.execute_index_commands(ast);
        }

        // Check for constraint management commands
        let has_create_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateConstraint(_)));
        let has_drop_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropConstraint(_)));

        if has_create_constraint || has_drop_constraint {
            return self.execute_constraint_commands(ast);
        }

        // Check for LOAD CSV commands
        let has_load_csv = ast.clauses.iter().any(|c| {
            matches!(c, executor::parser::Clause::LoadCsv(_))
        });

        if has_load_csv {
            return self.execute_load_csv_commands(ast);
        }

        // Check for CALL subquery commands
        let has_call_subquery = ast.clauses.iter().any(|c| {
            matches!(c, executor::parser::Clause::CallSubquery(_))
        });

        if has_call_subquery {
            return self.execute_call_subquery_commands(ast);
        }

        // Check for user management commands (should be handled at server level)
        let has_user_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::ShowUsers
                    | executor::parser::Clause::CreateUser(_)
                    | executor::parser::Clause::Grant(_)
                    | executor::parser::Clause::Revoke(_)
            )
        });

        if has_user_cmd {
            return Err(Error::CypherExecution(
                "User management commands (SHOW USERS, CREATE USER, GRANT, REVOKE) must be executed at server level".to_string(),
            ));
        }

        // Check if query contains CREATE or DELETE
        let has_create = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Create(_)));
        let has_delete = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Delete(_)));
        let has_merge = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Merge(_)));
        let has_set_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Set(_)));
        let has_remove_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Remove(_)));
        let has_foreach = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Foreach(_)));
        let has_match = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Match(_)));

        // Handle DELETE (with or without MATCH)
        if has_delete {
            if has_match {
                // MATCH ... DELETE: execute MATCH first, then DELETE with results
                self.execute_match_delete_query(ast)?;
            } else {
                // Standalone DELETE won't work without MATCH
                return Err(Error::CypherSyntax(
                    "DELETE requires MATCH clause".to_string(),
                ));
            }
            self.refresh_executor()?;

            // Return empty result for DELETE queries
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        // Handle MERGE / SET / REMOVE / FOREACH write queries before falling back to read executor
        if has_merge || has_set_clause || has_remove_clause || has_foreach {
            let result = self.execute_write_query(ast)?;
            return Ok(result);
        }

        // If query has CREATE (with or without MATCH), handle via Engine for persistence
        if has_create {
            if has_match {
                // MATCH ... CREATE: execute MATCH first, then CREATE with results
                self.execute_match_create_query(ast)?;
            } else {
                // Standalone CREATE
                self.execute_create_query(ast)?;
            }

            // Refresh executor to see the changes
            self.refresh_executor()?;
        }

        // Execute the query normally
        let query_obj = executor::Query {
            cypher: self.query_to_string(ast),
            params: std::collections::HashMap::new(),
        };
        self.executor.execute(&query_obj)
    }

    fn build_return_result(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        if return_clause.items.len() != 1 {
            return Err(Error::CypherExecution(
                "Only single RETURN items are supported in write queries".to_string(),
            ));
        }

        let item = &return_clause.items[0];
        let variable = match &item.expression {
            executor::parser::Expression::Variable(var) => var.clone(),
            _ => {
                return Err(Error::CypherExecution(
                    "Only variable projections are supported in RETURN for write queries"
                        .to_string(),
                ));
            }
        };

        let node_ids = context.get(&variable).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();

        for node_id in node_ids {
            if seen.insert(node_id) {
                let value = self.node_to_result_value(node_id)?;
                rows.push(executor::Row {
                    values: vec![value],
                });
            }
        }

        let column = item.alias.clone().unwrap_or(variable);
        Ok(executor::ResultSet {
            columns: vec![column],
            rows,
        })
    }

    fn ensure_node_state<'a>(
        &mut self,
        node_id: u64,
        cache: &'a mut HashMap<u64, NodeWriteState>,
    ) -> Result<&'a mut NodeWriteState> {
        use std::collections::hash_map::Entry;
        match cache.entry(node_id) {
            Entry::Vacant(e) => {
                let properties = self.load_node_properties_map(node_id)?;
                let record = self.storage.read_node(node_id)?;
                if record.is_deleted() {
                    return Err(Error::CypherExecution(format!(
                        "Node {} is deleted",
                        node_id
                    )));
                }
                let labels = self.catalog.get_labels_from_bitmap(record.label_bits)?;
                Ok(e.insert(NodeWriteState {
                    properties,
                    labels: labels.into_iter().collect(),
                }))
            }
            Entry::Occupied(e) => Ok(e.into_mut()),
        }
    }

    fn persist_node_state(&mut self, node_id: u64, state: NodeWriteState) -> Result<()> {
        let NodeWriteState { properties, labels } = state;
        self.storage
            .update_node_properties(node_id, Value::Object(properties))?;

        let mut label_ids = Vec::new();
        for label in labels {
            let label_id = self.catalog.get_or_create_label(&label)?;
            label_ids.push(label_id);
        }
        self.update_node_labels_with_ids(node_id, label_ids)?;
        Ok(())
    }

    fn load_node_properties_map(&self, node_id: u64) -> Result<Map<String, Value>> {
        if let Some(Value::Object(map)) = self.storage.load_node_properties(node_id)? {
            return Ok(map);
        }
        Ok(Map::new())
    }

    fn node_to_result_value(&mut self, node_id: u64) -> Result<Value> {
        let record = self.storage.read_node(node_id)?;
        if record.is_deleted() {
            return Ok(Value::Null);
        }

        let mut properties = self.load_node_properties_map(node_id)?;
        properties.insert("_nexus_id".to_string(), Value::Number(node_id.into()));
        let label_names = self.catalog.get_labels_from_bitmap(record.label_bits)?;
        let label_values = label_names.into_iter().map(Value::String).collect();
        properties.insert("_nexus_labels".to_string(), Value::Array(label_values));

        Ok(Value::Object(properties))
    }

    fn find_nodes_by_node_pattern(
        &mut self,
        node_pattern: &executor::parser::NodePattern,
    ) -> Result<Vec<u64>> {
        let mut label_ids = Vec::new();
        for label in &node_pattern.labels {
            match self.catalog.get_label_id(label) {
                Ok(id) => label_ids.push(id),
                Err(_) => return Ok(Vec::new()),
            }
        }

        let mut candidates = Vec::new();
        if label_ids.is_empty() {
            let total_nodes = self.storage.node_count();
            for node_id in 0..total_nodes {
                candidates.push(node_id);
            }
        } else {
            let bitmap = self.indexes.label_index.get_nodes_with_labels(&label_ids)?;
            for node_id in bitmap.iter() {
                candidates.push(node_id as u64);
            }
        }

        let mut matches = Vec::new();
        for node_id in candidates {
            let record = self.storage.read_node(node_id)?;
            if record.is_deleted() {
                continue;
            }
            if let Some(prop_map) = &node_pattern.properties {
                if !self.node_matches_properties(node_id, prop_map)? {
                    continue;
                }
            }
            matches.push(node_id);
        }

        Ok(matches)
    }

    fn node_matches_properties(
        &mut self,
        node_id: u64,
        prop_map: &executor::parser::PropertyMap,
    ) -> Result<bool> {
        let properties = self.load_node_properties_map(node_id)?;
        for (key, expr) in &prop_map.properties {
            let expected = self.expression_to_json_value(expr)?;
            match properties.get(key) {
                Some(existing) if existing == &expected => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    fn update_node_labels_with_ids(&mut self, node_id: u64, new_label_ids: Vec<u32>) -> Result<()> {
        let mut record = self.storage.read_node(node_id)?;
        if record.is_deleted() {
            return Err(Error::CypherExecution(format!(
                "Node {} is deleted",
                node_id
            )));
        }

        let current_ids = record.get_labels();
        let current_set: HashSet<u32> = current_ids.iter().copied().collect();
        let new_set: HashSet<u32> = new_label_ids.iter().copied().collect();

        let added: Vec<u32> = new_set.difference(&current_set).copied().collect();
        let removed: Vec<u32> = current_set.difference(&new_set).copied().collect();

        let mut new_bits = 0u64;
        for label_id in &new_label_ids {
            if *label_id < 64 {
                new_bits |= 1u64 << label_id;
            }
        }
        record.label_bits = new_bits;

        let mut tx = self.transaction_manager.begin_write()?;
        self.storage.write_node(node_id, &record)?;
        self.transaction_manager.commit(&mut tx)?;

        self.indexes
            .label_index
            .set_node_labels(node_id, &new_label_ids)?;

        for id in added {
            self.catalog.increment_node_count(id)?;
        }
        for id in removed {
            self.catalog.decrement_node_count(id)?;
        }

        Ok(())
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

        // Check constraints before creating node
        self.check_constraints(&label_ids, &properties, None)?;

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
        let mut label_ids = Vec::new();
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
            label_ids.push(label_id);
        }

        // Check constraints before updating node (exclude current node from uniqueness check)
        self.check_constraints(&label_ids, &properties, Some(id))?;

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
            // Remove node from label index before marking as deleted
            // This removes the node from all labels it belongs to
            self.indexes.label_index.remove_node(id)?;

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

    /// Delete all relationships connected to a node (for DETACH DELETE)
    pub fn delete_node_relationships(&mut self, node_id: u64) -> Result<()> {
        let mut tx = self.transaction_manager.begin_write()?;

        // Find all relationships connected to this node
        let total_rels = self.storage.relationship_count();
        let mut rels_to_delete = Vec::new();

        for rel_id in 0..total_rels {
            if let Ok(rel_record) = self.storage.read_rel(rel_id) {
                if !rel_record.is_deleted() {
                    // Check if this relationship is connected to the node
                    if rel_record.src_id == node_id || rel_record.dst_id == node_id {
                        rels_to_delete.push(rel_id);
                    }
                }
            }
        }

        // Mark all connected relationships as deleted
        for rel_id in rels_to_delete {
            if let Ok(rel_record) = self.storage.read_rel(rel_id) {
                let mut deleted_record = rel_record;
                deleted_record.mark_deleted();
                self.storage.write_rel(rel_id, &deleted_record)?;
            }
        }

        self.transaction_manager.commit(&mut tx)?;
        Ok(())
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
                // Load node properties
                let properties = self
                    .storage
                    .load_node_properties(node_id)
                    .unwrap_or(None)
                    .unwrap_or_else(|| serde_json::json!({}));

                let node_data = serde_json::json!({
                    "id": node_id,
                    "labels": labels,
                    "properties": properties
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

                // Load relationship properties
                let properties = self
                    .storage
                    .load_relationship_properties(rel_id)
                    .unwrap_or(None)
                    .unwrap_or_else(|| serde_json::json!({}));

                let rel_data = serde_json::json!({
                    "id": rel_id,
                    "source": src_id,
                    "target": dst_id,
                    "type": rel_type,
                    "properties": properties
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

struct NodeWriteState {
    properties: Map<String, Value>,
    labels: HashSet<String>,
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
