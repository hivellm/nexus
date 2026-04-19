//! Nexus graph engine: the `Engine` struct that owns the catalog, record
//! store, indexes, WAL, page cache, and execution frontend. All top-level
//! Cypher execution entry points (`execute_cypher`, write-path handlers,
//! index/constraint/function/ACL command dispatch) live here.
//!
//! This module was split out of `lib.rs` to tame a 5.5k-line crate root;
//! future refactors will further slice it by responsibility.

use crate::graph::clustering::{
    ClusteringAlgorithm, ClusteringConfig, ClusteringEngine, ClusteringResult, DistanceMetric,
    FeatureStrategy,
};
use crate::{
    Error, Graph, Result, ValidationResult, auth, cache, catalog, database, execution, executor,
    geospatial, graph, index, loader, memory_management, page_cache, query_cache, relationship,
    security, session, storage, transaction, udf, validation, wal,
};
use parking_lot::RwLock;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub mod clustering;
pub mod config;
pub mod crud;
pub mod maintenance;
pub mod stats;

#[cfg(test)]
mod tests;

pub use config::{EngineConfig, GraphStatistics};
pub use stats::{EngineStats, HealthState, HealthStatus};

// `NodeWriteState` lives in `crud.rs` alongside the CRUD methods
// that build and consume it; re-import under the short name so the
// cypher-execution code in this file can keep referring to it.
use crud::NodeWriteState;

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
    /// Asynchronous WAL writer for improved performance
    pub async_wal_writer: Option<wal::AsyncWalWriter>,
    /// Transaction manager for MVCC (shared with SessionManager via Arc)
    pub transaction_manager: Arc<RwLock<transaction::TransactionManager>>,
    /// Session manager for transaction context
    pub session_manager: session::SessionManager,
    /// Index subsystem
    pub indexes: index::IndexManager,
    /// Query executor
    pub executor: executor::Executor,
    /// Multi-layer cache system for performance optimization
    pub cache: cache::MultiLayerCache,
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

    /// Create a new engine instance with a specific data directory, using
    /// the default [`EngineConfig`]. This allows persistent storage instead
    /// of temporary directories.
    pub fn with_data_dir<P: AsRef<std::path::Path>>(data_dir: P) -> Result<Self> {
        Self::with_data_dir_and_config(data_dir, EngineConfig::default())
    }

    /// Create a new engine instance with a specific data directory and
    /// caller-supplied [`EngineConfig`]. Used by callers that load values
    /// from YAML or CLI flags.
    pub fn with_data_dir_and_config<P: AsRef<std::path::Path>>(
        data_dir: P,
        config: EngineConfig,
    ) -> Result<Self> {
        let data_dir = data_dir.as_ref();

        // Ensure data directory exists
        std::fs::create_dir_all(data_dir)?;

        // Initialize catalog
        let catalog = catalog::Catalog::new(data_dir.join("catalog.mdb"))?;

        // Initialize record stores
        let storage = storage::RecordStore::new(data_dir)?;

        // Initialize page cache
        let page_cache = page_cache::PageCache::new(config.page_cache_capacity)?;

        // Initialize WAL
        let wal = wal::Wal::new(data_dir.join("wal.log"))?;

        // Initialize async WAL writer (optional - can be disabled for testing)
        let async_wal_writer = Some(wal::AsyncWalWriter::new(
            wal.clone(),
            wal::AsyncWalConfig::default(),
        )?);

        // Initialize transaction manager (shared between Engine and SessionManager)
        let transaction_manager = transaction::TransactionManager::new()?;
        let transaction_manager_arc = Arc::new(RwLock::new(transaction_manager));

        // Initialize session manager (shares TransactionManager Arc)
        let session_manager = session::SessionManager::new(transaction_manager_arc.clone());

        // Initialize index manager
        let indexes = index::IndexManager::new(data_dir.join("indexes"))?;

        // Initialize executor
        let executor =
            executor::Executor::new(&catalog, &storage, &indexes.label_index, &indexes.knn_index)?;

        // Initialize multi-layer cache system
        let cache_config = cache::CacheConfig::default();
        let cache = cache::MultiLayerCache::new(cache_config)?;

        // Only warm cache if explicitly requested (not by default)
        // This prevents performance regression on engine startup
        // Cache will warm up naturally during query execution

        // Engine shares the same TransactionManager Arc with SessionManager
        let mut engine = Engine {
            catalog,
            storage,
            page_cache,
            wal,
            async_wal_writer,
            transaction_manager: transaction_manager_arc,
            session_manager,
            indexes,
            executor,
            cache,
            _temp_dir: None,
        };

        // Configure cache in executor for relationship index access
        // Note: In a production implementation, we'd need proper interior mutability
        // For now, the executor will use the cache when available via direct access

        engine.rebuild_indexes_from_storage()?;

        Ok(engine)
    }

    /// Create engine with isolated catalog (bypasses test sharing)
    ///
    /// WARNING: Use sparingly - each call creates a new LMDB environment.
    /// Only use for tests that absolutely require data isolation.
    /// This is available for both unit tests and integration tests.
    pub fn with_isolated_catalog<P: AsRef<std::path::Path>>(data_dir: P) -> Result<Self> {
        let data_dir = data_dir.as_ref();
        std::fs::create_dir_all(data_dir)?;

        // Initialize catalog with isolated path
        let catalog = catalog::Catalog::with_isolated_path(
            data_dir.join("catalog.mdb"),
            catalog::CATALOG_MMAP_INITIAL_SIZE,
        )?;

        // Initialize record stores
        let storage = storage::RecordStore::new(data_dir)?;

        // Initialize page cache
        let page_cache = page_cache::PageCache::new(1024)?;

        // Initialize WAL
        let wal = wal::Wal::new(data_dir.join("wal.log"))?;

        // Initialize async WAL writer
        let async_wal_writer = Some(wal::AsyncWalWriter::new(
            wal.clone(),
            wal::AsyncWalConfig::default(),
        )?);

        // Initialize transaction manager
        let transaction_manager = transaction::TransactionManager::new()?;
        let transaction_manager_arc = Arc::new(RwLock::new(transaction_manager));

        // Initialize session manager
        let session_manager = session::SessionManager::new(transaction_manager_arc.clone());

        // Initialize index manager
        let indexes = index::IndexManager::new(data_dir.join("indexes"))?;

        // Initialize executor
        let executor =
            executor::Executor::new(&catalog, &storage, &indexes.label_index, &indexes.knn_index)?;

        // Initialize multi-layer cache system
        let cache_config = cache::CacheConfig::default();
        let cache = cache::MultiLayerCache::new(cache_config)?;

        let mut engine = Engine {
            catalog,
            storage,
            page_cache,
            wal,
            async_wal_writer,
            transaction_manager: transaction_manager_arc,
            session_manager,
            indexes,
            executor,
            cache,
            _temp_dir: None,
        };

        engine.rebuild_indexes_from_storage()?;

        Ok(engine)
    }

    /// Warm up the cache system for better initial performance
    /// This should be called after engine creation if cache warming is desired
    /// Note: This can be expensive and should be done in background for production
    pub fn warm_cache(&mut self) -> Result<()> {
        self.cache.warm_cache()?;
        Ok(())
    }

    fn rebuild_indexes_from_storage(&mut self) -> Result<()> {
        // Clear the index first to ensure we start fresh
        self.indexes.label_index.clear()?;

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
    /// Returns the number of nodes deleted
    fn execute_match_delete_query(&mut self, ast: &executor::parser::CypherQuery) -> Result<u64> {
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

        // Count deleted nodes
        let mut deleted_count = 0u64;

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
                                        if node_record.first_rel_ptr != 0 {
                                            return Err(Error::CypherExecution(
                                                "Cannot DELETE node with existing relationships; use DETACH DELETE"
                                                    .to_string(),
                                            ));
                                        }
                                        self.delete_node(node_id)?;
                                    }
                                    deleted_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(deleted_count)
    }

    /// Execute MATCH ... CREATE query
    fn execute_match_create_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
        query_str_opt: Option<&str>,
    ) -> Result<executor::ResultSet> {
        // FIXED: Don't split the query - let the executor handle MATCH...CREATE as a single operation
        // The executor's CREATE operator (execute_create_with_context) will correctly handle
        // creating relationships using the MATCH results

        let cypher = if let Some(qs) = query_str_opt {
            qs.to_string()
        } else {
            self.query_to_string(ast)
        };

        let query_obj = executor::Query {
            cypher,
            params: ast.params.clone(),
        };

        // Execute and return result
        self.executor.execute(&query_obj)
    }

    /// Create from pattern with existing node context
    fn create_from_pattern_with_context(
        &mut self,
        pattern: &executor::parser::Pattern,
        node_vars: &std::collections::HashMap<String, u64>,
    ) -> Result<()> {
        let mut tx_ref: Option<&mut transaction::Transaction> = None;
        self.create_from_pattern_with_context_and_transaction(pattern, node_vars, &mut tx_ref, None)
    }

    /// Create from pattern with existing node context and optional transaction
    fn create_from_pattern_with_context_and_transaction(
        &mut self,
        pattern: &executor::parser::Pattern,
        node_vars: &std::collections::HashMap<String, u64>,
        session_tx: &mut Option<&mut transaction::Transaction>,
        mut created_nodes_tracker: Option<&mut Vec<u64>>,
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

                            let node_id = if let Some(ref mut tracker) = created_nodes_tracker {
                                self.create_node_with_transaction(
                                    node.labels.clone(),
                                    properties,
                                    session_tx,
                                    Some(tracker),
                                )?
                            } else {
                                self.create_node_with_transaction(
                                    node.labels.clone(),
                                    properties,
                                    session_tx,
                                    None,
                                )?
                            };
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

                            self.create_relationship_with_transaction(
                                source_id,
                                target_id,
                                rel_type.clone(),
                                rel_properties,
                                session_tx,
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
        // Get session and check if it has an active transaction
        let session_id = "default";

        // Get session once and check if it has an active transaction
        let mut session = self.session_manager.get_session(&session_id.to_string());

        if let Some(ref mut sess) = session {
            if sess.has_active_transaction() {
                // Extract transaction from session
                if let Some(mut tx) = sess.active_transaction.take() {
                    // Execute CREATE operations with this transaction
                    let mut tx_ref: Option<&mut transaction::Transaction> = Some(&mut tx);
                    let result =
                        self.execute_create_query_with_transaction(ast, &mut tx_ref, Some(sess));

                    // Put transaction back in session and update session with tracked nodes
                    sess.active_transaction = Some(tx);
                    self.session_manager.update_session(sess.clone());

                    return result;
                }
            }
        }

        // No active transaction, execute normally (will create own transactions)
        let mut tx_ref: Option<&mut transaction::Transaction> = None;
        self.execute_create_query_with_transaction(ast, &mut tx_ref, None)
    }

    /// Execute CREATE query with optional transaction
    fn execute_create_query_with_transaction(
        &mut self,
        ast: &executor::parser::CypherQuery,
        session_tx: &mut Option<&mut transaction::Transaction>,
        mut session: Option<&mut session::Session>,
    ) -> Result<()> {
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

                            // Create node using Engine API with session transaction if available
                            let node_id = self.create_node_with_transaction(
                                node.labels.clone(),
                                properties,
                                session_tx,
                                session.as_mut().map(|s| &mut s.created_nodes),
                            )?;

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

                                    // Create target node with session transaction if available
                                    let tid = self.create_node_with_transaction(
                                        target_node.labels.clone(),
                                        target_properties,
                                        session_tx,
                                        session.as_mut().map(|s| &mut s.created_nodes),
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

                            // Create relationship using Engine API with session transaction if available
                            self.create_relationship_with_transaction(
                                source_id,
                                target_id,
                                rel_type.to_string(),
                                rel_properties,
                                session_tx,
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
                executor::parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Evaluate expression for SET clause with node context
    fn evaluate_set_expression(
        &self,
        expr: &executor::parser::Expression,
        target_var: &str,
        node_props: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match expr {
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::String(s) => Ok(serde_json::Value::String(s.clone())),
                executor::parser::Literal::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
                executor::parser::Literal::Float(f) => serde_json::Number::from_f64(*f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| Error::CypherExecution(format!("Invalid float: {}", f))),
                executor::parser::Literal::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
                executor::parser::Literal::Null => Ok(serde_json::Value::Null),
                executor::parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            executor::parser::Expression::PropertyAccess { variable, property } => {
                if variable == target_var {
                    Ok(node_props
                        .get(property)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            executor::parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_set_expression(left, target_var, node_props)?;
                let right_val = self.evaluate_set_expression(right, target_var, node_props)?;
                match op {
                    executor::parser::BinaryOperator::Add => {
                        self.json_add_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Subtract => {
                        self.json_subtract_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Multiply => {
                        self.json_multiply_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Divide => {
                        self.json_divide_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Modulo => {
                        self.json_modulo_values(&left_val, &right_val)
                    }
                    _ => Err(Error::CypherExecution(format!(
                        "Unsupported binary operator in SET: {:?}",
                        op
                    ))),
                }
            }
            executor::parser::Expression::UnaryOp { op, operand } => {
                let val = self.evaluate_set_expression(operand, target_var, node_props)?;
                match op {
                    executor::parser::UnaryOperator::Minus => {
                        if let Some(n) = val.as_i64() {
                            Ok(serde_json::Value::Number((-n).into()))
                        } else if let Some(n) = val.as_f64() {
                            serde_json::Number::from_f64(-n)
                                .map(serde_json::Value::Number)
                                .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    executor::parser::UnaryOperator::Not => val
                        .as_bool()
                        .map(|b| serde_json::Value::Bool(!b))
                        .ok_or_else(|| Error::CypherExecution("Invalid bool".to_string())),
                    _ => Ok(serde_json::Value::Null),
                }
            }
            _ => Err(Error::CypherExecution(
                "Unsupported expression type in SET clause".to_string(),
            )),
        }
    }

    fn json_add_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li + ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf + rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            (serde_json::Value::String(l), serde_json::Value::String(r)) => {
                Ok(serde_json::Value::String(format!("{}{}", l, r)))
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_subtract_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li - ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf - rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_multiply_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li * ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf * rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_divide_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    if rf == 0.0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        serde_json::Number::from_f64(lf / rf)
                            .map(serde_json::Value::Number)
                            .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_modulo_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    if ri == 0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        Ok(serde_json::Value::Number((li % ri).into()))
                    }
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    if rf == 0.0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        serde_json::Number::from_f64(lf % rf)
                            .map(serde_json::Value::Number)
                            .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
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
    pub fn stats(&mut self) -> Result<EngineStats> {
        Ok(EngineStats {
            nodes: self.storage.node_count(),
            relationships: self.storage.relationship_count(),
            labels: self.catalog.label_count(),
            rel_types: self.catalog.rel_type_count(),
            page_cache_hits: self.page_cache.hit_count(),
            page_cache_misses: self.page_cache.miss_count(),
            wal_entries: self.wal.entry_count(),
            active_transactions: self.transaction_manager.read().active_count(),
            cache_stats: self.cache.stats().clone(),
        })
    }

    /// Write a WAL entry asynchronously (if async writer is enabled)
    /// Falls back to synchronous WAL if async writer is not available
    pub fn write_wal_async(&mut self, entry: wal::WalEntry) -> Result<()> {
        if let Some(ref writer) = self.async_wal_writer {
            writer.append(entry)?;
            Ok(())
        } else {
            // Fallback to synchronous WAL
            self.wal.append(&entry)?;
            self.wal.flush()?;
            Ok(())
        }
    }

    /// Force flush all pending async WAL entries
    pub fn flush_async_wal(&mut self) -> Result<()> {
        if let Some(ref writer) = self.async_wal_writer {
            writer.flush()?;
        }
        Ok(())
    }

    /// Get async WAL statistics (if available)
    pub fn async_wal_stats(&self) -> Option<wal::AsyncWalStatsSnapshot> {
        self.async_wal_writer.as_ref().map(|w| w.stats())
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
        // These commands (CREATE/DROP DATABASE, SHOW DATABASES, USE DATABASE) should be handled at server level
        // as Engine doesn't have access to DatabaseManager
        let has_admin_db_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::CreateDatabase(_)
                    | executor::parser::Clause::DropDatabase(_)
                    | executor::parser::Clause::ShowDatabases
                    | executor::parser::Clause::UseDatabase(_)
            )
        });

        if has_admin_db_cmd {
            return Err(Error::CypherExecution(
                "Database management commands (CREATE/DROP DATABASE, SHOW DATABASES, USE DATABASE) must be executed at server level".to_string(),
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
            return self.execute_transaction_commands(&ast, None);
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

        // Check for function management commands
        let has_show_functions = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::ShowFunctions));
        let has_show_constraints = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::ShowConstraints));
        let has_create_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateFunction(_)));
        let has_drop_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropFunction(_)));

        if has_show_functions || has_show_constraints || has_create_function || has_drop_function {
            return self.execute_function_commands(&ast);
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
            let deleted_count = if has_match {
                // MATCH ... DELETE: execute MATCH first, then DELETE with results
                self.execute_match_delete_query(&ast)?
            } else {
                // Standalone DELETE won't work without MATCH
                // This would be: DELETE n (without MATCH)
                // For now, we don't support this syntax
                return Err(Error::CypherSyntax(
                    "DELETE requires MATCH clause".to_string(),
                ));
            };
            self.refresh_executor()?;

            // Check if there's a RETURN clause after DELETE
            let return_clause_opt = ast.clauses.iter().find_map(|c| {
                if let executor::parser::Clause::Return(rc) = c {
                    Some(rc)
                } else {
                    None
                }
            });

            if let Some(return_clause) = return_clause_opt {
                // Check if RETURN contains count aggregation
                let mut is_count_only = false;
                let mut count_alias = "count".to_string();

                if return_clause.items.len() == 1 {
                    let executor::parser::ReturnItem { expression, alias } =
                        &return_clause.items[0];
                    if let executor::parser::Expression::FunctionCall { name, args: _ } = expression
                    {
                        if name.to_lowercase() == "count" {
                            is_count_only = true;
                            count_alias = alias.clone().unwrap_or_else(|| "count".to_string());
                        }
                    }
                }

                if is_count_only {
                    // Return count of deleted nodes
                    return Ok(executor::ResultSet {
                        columns: vec![count_alias],
                        rows: vec![executor::Row {
                            values: vec![serde_json::Value::Number(deleted_count.into())],
                        }],
                    });
                } else {
                    // If there's a RETURN clause with other expressions, let the executor handle it
                    // The executor will process the RETURN, but since nodes are deleted,
                    // it will likely return empty results or handle it appropriately
                    let query_obj = executor::Query {
                        cypher: self.query_to_string(&ast),
                        params: ast.params.clone(),
                    };
                    return self.executor.execute(&query_obj);
                }
            } else {
                // No RETURN clause - return count of deleted nodes
                return Ok(executor::ResultSet {
                    columns: vec!["count".to_string()],
                    rows: vec![executor::Row {
                        values: vec![serde_json::Value::Number(deleted_count.into())],
                    }],
                });
            }
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
                let result = self.execute_match_create_query(&ast, Some(query))?;

                // CRITICAL: Sync executor's store back to engine's storage
                // The executor has a cloned store, so changes need to be synced back
                self.storage = self.executor.get_store();

                // NOTE: Do NOT call refresh_executor() here!
                // The caller should call refresh_executor() explicitly when ready
                // This allows batching multiple CREATE statements before refreshing

                return Ok(result);
            }

            // Standalone CREATE - execute through executor only (not through Engine)
            // This prevents duplicate node creation
            // The executor will handle CREATE internally
            // Just refresh after to see changes
            let query_obj = executor::Query {
                cypher: query.to_string(),
                params: std::collections::HashMap::new(),
            };
            let result = self.executor.execute(&query_obj)?;

            // CRITICAL: Sync executor's store back to engine's storage
            self.storage = self.executor.get_store();

            // Refresh executor to see the changes (only if not in transaction)
            let session_id = "default";
            let in_transaction = {
                let session = self.session_manager.get_session(&session_id.to_string());
                session.map(|s| s.has_active_transaction()).unwrap_or(false)
            };

            if !in_transaction {
                self.refresh_executor()?;
            }

            return Ok(result);
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
        // Track relationship bindings: variable -> (rel_id, rel_type)
        let mut rel_context: HashMap<String, (u64, String)> = HashMap::new();
        let mut result: Option<executor::ResultSet> = None;

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::Match(match_clause) => {
                    // Process all node patterns in the match clause
                    self.process_match_clause_multi(match_clause, &mut context)?;
                }
                executor::parser::Clause::Merge(merge_clause) => {
                    // Check if this is a relationship MERGE with bound variables
                    if let Some((rel_var, rel_id, rel_type)) =
                        self.process_merge_relationship(&merge_clause, &context)?
                    {
                        rel_context.insert(rel_var, (rel_id, rel_type));
                    } else {
                        // Fall back to node MERGE
                        let (variable, node_ids) = self.process_merge_clause(merge_clause)?;
                        context.insert(variable, node_ids);
                    }
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
                    result = Some(self.build_return_result_with_rels(
                        &context,
                        &rel_context,
                        return_clause,
                    )?);
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

    /// Process all node patterns in a MATCH clause (for multi-node patterns like (a), (b))
    fn process_match_clause_multi(
        &mut self,
        match_clause: &executor::parser::MatchClause,
        context: &mut HashMap<String, Vec<u64>>,
    ) -> Result<()> {
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

        // Process all node patterns in the pattern
        for element in &match_clause.pattern.elements {
            if let executor::parser::PatternElement::Node(node_pattern) = element {
                if let Some(variable) = &node_pattern.variable {
                    let mut node_ids = self.find_nodes_by_node_pattern(node_pattern)?;
                    node_ids.sort_unstable();
                    node_ids.dedup();
                    context.insert(variable.clone(), node_ids);
                }
            }
        }

        Ok(())
    }

    /// Process MERGE with relationship pattern when nodes are already bound
    /// Returns Some((rel_variable, rel_id, rel_type)) if this is a relationship MERGE
    fn process_merge_relationship(
        &mut self,
        merge_clause: &executor::parser::MergeClause,
        context: &HashMap<String, Vec<u64>>,
    ) -> Result<Option<(String, u64, String)>> {
        // Check if pattern has: Node, Relationship, Node structure
        let elements = &merge_clause.pattern.elements;
        if elements.len() != 3 {
            return Ok(None);
        }

        // Extract source node, relationship, and target node
        let src_node = match &elements[0] {
            executor::parser::PatternElement::Node(n) => n,
            _ => return Ok(None),
        };
        let rel_pattern = match &elements[1] {
            executor::parser::PatternElement::Relationship(r) => r,
            _ => return Ok(None),
        };
        let dst_node = match &elements[2] {
            executor::parser::PatternElement::Node(n) => n,
            _ => return Ok(None),
        };

        // Get source and destination variable names
        let src_var = match &src_node.variable {
            Some(v) => v,
            None => return Ok(None),
        };
        let dst_var = match &dst_node.variable {
            Some(v) => v,
            None => return Ok(None),
        };

        // Get relationship variable and type
        let rel_var = match &rel_pattern.variable {
            Some(v) => v.clone(),
            None => return Ok(None),
        };
        let rel_type = match rel_pattern.types.first() {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        // Check that source and destination nodes are already bound
        let src_ids = match context.get(src_var) {
            Some(ids) if !ids.is_empty() => ids,
            _ => return Ok(None),
        };
        let dst_ids = match context.get(dst_var) {
            Some(ids) if !ids.is_empty() => ids,
            _ => return Ok(None),
        };

        // For simplicity, use first node of each
        let src_id = src_ids[0];
        let dst_id = dst_ids[0];

        // Check if relationship already exists
        let existing_rel = self.find_relationship_between(src_id, dst_id, &rel_type)?;

        let rel_id = if let Some(rid) = existing_rel {
            // Relationship exists - run ON MATCH if present
            if let Some(on_match) = &merge_clause.on_match {
                // ON MATCH would apply to relationship properties
                // For now, we don't support SET on relationships in this context
                let _ = on_match;
            }
            rid
        } else {
            // Create the relationship
            let props = Value::Object(Map::new());
            let new_rel_id = self.create_relationship(src_id, dst_id, rel_type.clone(), props)?;

            // Run ON CREATE if present
            if let Some(on_create) = &merge_clause.on_create {
                // ON CREATE would apply to relationship properties
                // For now, we don't support SET on relationships in this context
                let _ = on_create;
            }
            new_rel_id
        };

        Ok(Some((rel_var, rel_id, rel_type)))
    }

    /// Find a relationship of a specific type between two nodes
    fn find_relationship_between(
        &self,
        src_id: u64,
        dst_id: u64,
        rel_type: &str,
    ) -> Result<Option<u64>> {
        // Get the type ID
        let type_id = match self.catalog.get_type_id(rel_type)? {
            Some(id) => id,
            None => return Ok(None),
        };

        // Read source node to get its relationship chain
        let src_node = self.storage.read_node(src_id)?;
        let mut rel_ptr = src_node.first_rel_ptr;

        while rel_ptr != 0 {
            let rel_record = self.storage.read_rel(rel_ptr)?;

            // Check if this is an outgoing relationship to dst_id with the right type
            if rel_record.src_id == src_id
                && rel_record.dst_id == dst_id
                && rel_record.type_id == type_id
            {
                return Ok(Some(rel_ptr));
            }

            // Move to next relationship in chain
            if rel_record.src_id == src_id {
                rel_ptr = rel_record.next_src_ptr;
            } else if rel_record.dst_id == src_id {
                rel_ptr = rel_record.next_dst_ptr;
            } else {
                break;
            }
        }

        Ok(None)
    }

    /// Build return result with support for relationship variables
    fn build_return_result_with_rels(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, (u64, String)>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        if return_clause.items.is_empty() {
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        // Check if any return item references a relationship variable
        let has_rel_refs = return_clause
            .items
            .iter()
            .any(|item| self.expression_references_rel(&item.expression, rel_context));

        if !has_rel_refs || rel_context.is_empty() {
            // No relationship references, use regular handling
            return self.build_return_result(context, return_clause);
        }

        // Build result with relationship variable support
        let mut columns = Vec::new();
        let mut row_values = Vec::new();

        for item in &return_clause.items {
            let col_name = item
                .alias
                .clone()
                .unwrap_or_else(|| self.expression_to_string(&item.expression));
            columns.push(col_name);

            let value =
                self.evaluate_return_expression_with_rels(&item.expression, context, rel_context)?;
            row_values.push(value);
        }

        Ok(executor::ResultSet {
            columns,
            rows: vec![executor::Row { values: row_values }],
        })
    }

    /// Check if an expression references a relationship variable
    fn expression_references_rel(
        &self,
        expr: &executor::parser::Expression,
        rel_context: &HashMap<String, (u64, String)>,
    ) -> bool {
        match expr {
            executor::parser::Expression::Variable(v) => rel_context.contains_key(v),
            executor::parser::Expression::FunctionCall { args, .. } => args
                .iter()
                .any(|arg| self.expression_references_rel(arg, rel_context)),
            executor::parser::Expression::PropertyAccess { variable, .. } => {
                rel_context.contains_key(variable)
            }
            _ => false,
        }
    }

    /// Evaluate a return expression with relationship variable support
    fn evaluate_return_expression_with_rels(
        &self,
        expr: &executor::parser::Expression,
        _context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, (u64, String)>,
    ) -> Result<Value> {
        match expr {
            executor::parser::Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                if func_name == "type" && args.len() == 1 {
                    // type(r) - return relationship type
                    if let executor::parser::Expression::Variable(var) = &args[0] {
                        if let Some((_rel_id, rel_type)) = rel_context.get(var) {
                            return Ok(Value::String(rel_type.clone()));
                        }
                    }
                }
                // For other functions, return null for now
                Ok(Value::Null)
            }
            executor::parser::Expression::Variable(var) => {
                if let Some((rel_id, rel_type)) = rel_context.get(var) {
                    // Return relationship as object
                    let mut obj = Map::new();
                    obj.insert("_id".to_string(), Value::Number((*rel_id).into()));
                    obj.insert("_type".to_string(), Value::String(rel_type.clone()));
                    Ok(Value::Object(obj))
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Ok(Value::Null),
        }
    }

    fn apply_set_clause(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        set_clause: &executor::parser::SetClause,
    ) -> Result<()> {
        tracing::info!(
            "[apply_set_clause] START: context={:?}, items={}",
            context,
            set_clause.items.len()
        );
        if set_clause.items.is_empty() {
            tracing::info!("[apply_set_clause] No items, returning early");
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

                    // Evaluate expression per-node to support expressions like n.value * 2
                    tracing::info!(
                        "[apply_set_clause] Property SET: target={}, property={}, node_ids={:?}",
                        target,
                        property,
                        node_ids
                    );
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        let json_value =
                            self.evaluate_set_expression(value, target, &state.properties)?;
                        tracing::info!(
                            "[apply_set_clause] node_id={}, property={}, new_value={:?}",
                            node_id,
                            property,
                            json_value
                        );
                        state.properties.insert(property.clone(), json_value);
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

        tracing::info!(
            "[apply_set_clause] About to persist {} nodes",
            state_map.len()
        );
        for (node_id, state) in state_map.into_iter() {
            tracing::info!(
                "[apply_set_clause] Persisting node_id={}, properties={:?}",
                node_id,
                state.properties
            );
            self.persist_node_state(node_id, state)?;
        }
        tracing::info!("[apply_set_clause] DONE");

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
                                if node_record.first_rel_ptr != 0 {
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
    /// Requires a session_id to track transaction context across queries
    fn execute_transaction_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
        session_id: Option<&str>,
    ) -> Result<executor::ResultSet> {
        // Use provided session_id or generate a default one
        // In a full implementation, session_id would come from HTTP headers or connection context
        let session_id = session_id.unwrap_or("default");

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::BeginTransaction => {
                    // Get or create session
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());

                    // Begin transaction for this session
                    session.begin_transaction()?;

                    // Update session in manager
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::CommitTransaction => {
                    // Get session
                    let mut session = self
                        .session_manager
                        .get_session(&session_id.to_string())
                        .ok_or_else(|| {
                            Error::transaction(format!(
                                "Session {} not found or expired",
                                session_id
                            ))
                        })?;

                    // Apply pending index updates in batch before commit (Phase 1 optimization)
                    self.apply_pending_index_updates(&mut session)?;

                    // Commit transaction
                    session.commit_transaction()?;

                    // Flush storage to ensure durability
                    self.storage.flush()?;

                    // Rebuild indexes from storage after commit
                    // This ensures indexes reflect committed changes and are not affected by rollback
                    self.rebuild_indexes_from_storage()?;

                    // Refresh executor to see the updated indexes
                    self.refresh_executor()?;

                    // Update session in manager
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::RollbackTransaction => {
                    // Get session
                    let mut session = self
                        .session_manager
                        .get_session(&session_id.to_string())
                        .ok_or_else(|| {
                            Error::transaction(format!(
                                "Session {} not found or expired",
                                session_id
                            ))
                        })?;

                    // CRITICAL: Clone created_nodes list before marking as deleted
                    // because get_session may return a cloned session
                    let nodes_to_delete = session.created_nodes.clone();
                    let rels_to_delete = session.created_relationships.clone();

                    // Remove nodes from index and mark as deleted in storage BEFORE rollback
                    // This ensures we clean up nodes that were written to storage (mmap writes immediately)
                    for node_id in &nodes_to_delete {
                        // First, mark as deleted in storage (this prevents reads from returning the node)
                        if let Err(e) = self.storage.delete_node(*node_id) {
                            tracing::warn!("Failed to delete node {} from storage: {}", node_id, e);
                        }

                        // Read node properties before deletion to remove from property index
                        if let Ok(Some(properties)) = self.storage.load_node_properties(*node_id) {
                            if let serde_json::Value::Object(props) = properties {
                                let property_index = self.cache.property_index_manager();
                                for prop_name in props.keys() {
                                    if let Err(e) =
                                        property_index.remove_property(prop_name, *node_id)
                                    {
                                        // Property index may not exist for this property, ignore error
                                        let _ = e;
                                    }
                                }
                            }
                        }

                        // Remove from label index AFTER marking as deleted
                        // remove_node removes the node from all label bitmaps
                        if let Err(e) = self.indexes.label_index.remove_node(*node_id) {
                            tracing::warn!(
                                "Failed to remove node {} from label index: {}",
                                node_id,
                                e
                            );
                        }
                    }

                    // Mark all relationships created during this transaction as deleted
                    for rel_id in &rels_to_delete {
                        if let Err(e) = self.storage.delete_rel(*rel_id) {
                            tracing::warn!(
                                "Failed to delete relationship {} from storage: {}",
                                rel_id,
                                e
                            );
                        }
                    }

                    // Flush storage to ensure consistency (must be done before rollback)
                    if let Err(e) = self.storage.flush() {
                        tracing::warn!("Failed to flush storage: {}", e);
                    }

                    // Rollback transaction (abort the transaction)
                    session.rollback_transaction()?;

                    // Clear tracking lists after rollback
                    session.created_nodes.clear();
                    session.created_relationships.clear();
                    // Clear pending index updates (they should not be applied on rollback)
                    session.pending_index_updates.clear();

                    // Update session in manager BEFORE refreshing executor
                    // This ensures the session state is saved before executor refresh
                    self.session_manager.update_session(session);

                    // Refresh executor to see the updated indexes
                    // Note: We don't rebuild indexes here because we've already removed
                    // nodes from indexes manually above. Rebuilding would be redundant and
                    // could potentially reintroduce deleted nodes if there's a timing issue.
                    self.refresh_executor()?;
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
        let mut result_rows = Vec::new();
        let columns = vec!["index".to_string(), "message".to_string()];

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::CreateIndex(create_index) => {
                    // Get label and property IDs
                    let label_id = self.catalog.get_or_create_label(&create_index.label)?;
                    let property_key_id = self.catalog.get_or_create_key(&create_index.property)?;

                    // Check if index already exists
                    let index_exists = self
                        .indexes
                        .property_index
                        .has_index(label_id, property_key_id);

                    // Handle OR REPLACE
                    if create_index.or_replace && index_exists {
                        // Drop existing index first
                        self.indexes
                            .property_index
                            .drop_index(label_id, property_key_id)?;
                    }

                    // Handle IF NOT EXISTS
                    if !create_index.or_replace && create_index.if_not_exists && index_exists {
                        // Index already exists and IF NOT EXISTS was specified, skip
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(format!(
                                    ":{}({})",
                                    create_index.label, create_index.property
                                )),
                                serde_json::Value::String(
                                    "Index already exists, skipped".to_string(),
                                ),
                            ],
                        });
                        continue;
                    }

                    // Check if index already exists (error if not IF NOT EXISTS or OR REPLACE)
                    if !create_index.or_replace && !create_index.if_not_exists && index_exists {
                        return Err(Error::CypherExecution(format!(
                            "Index on :{}({}) already exists",
                            create_index.label, create_index.property
                        )));
                    }

                    // Check if this is a spatial index
                    let is_spatial = create_index.index_type.as_deref() == Some("spatial");

                    if is_spatial {
                        // Spatial indexes are handled by the executor
                        // Create the spatial index through executor
                        self.executor.execute_create_index(
                            &create_index.label,
                            &create_index.property,
                            Some("spatial"),
                            create_index.if_not_exists,
                            create_index.or_replace,
                        )?;

                        // Return success message
                        let index_name =
                            format!(":{}({})", create_index.label, create_index.property);

                        // Check if index was replaced (we need to check executor's spatial_indexes)
                        // For now, assume it was created unless or_replace was used
                        let message = if create_index.or_replace {
                            format!("Spatial index {} replaced", index_name)
                        } else {
                            format!("Spatial index {} created", index_name)
                        };
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(index_name),
                                serde_json::Value::String(message),
                            ],
                        });
                    } else {
                        // Create the property index structure
                        self.indexes
                            .property_index
                            .create_index(label_id, property_key_id)?;

                        // Populate index with existing nodes that have this label and property
                        self.populate_index(label_id, property_key_id)?;

                        // Return success message
                        let index_name =
                            format!(":{}({})", create_index.label, create_index.property);
                        let message = if create_index.or_replace && index_exists {
                            format!("Index {} replaced", index_name)
                        } else {
                            format!("Index {} created", index_name)
                        };
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(index_name),
                                serde_json::Value::String(message),
                            ],
                        });
                    }
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
                    if !self
                        .indexes
                        .property_index
                        .has_index(label_id, property_key_id)
                    {
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
                    self.indexes
                        .property_index
                        .drop_index(label_id, property_key_id)?;

                    // Return success message
                    let index_name = format!(":{}({})", drop_index.label, drop_index.property);
                    let index_name_clone = index_name.clone();
                    result_rows.push(executor::Row {
                        values: vec![
                            serde_json::Value::String(index_name),
                            serde_json::Value::String(format!(
                                "Index {} dropped",
                                index_name_clone
                            )),
                        ],
                    });
                }
                _ => {}
            }
        }

        // If no rows were added (all commands were skipped), return empty result
        if result_rows.is_empty() {
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        Ok(executor::ResultSet {
            columns,
            rows: result_rows,
        })
    }

    /// Populate an index with existing nodes that have the specified label and property
    fn populate_index(&mut self, label_id: u32, property_key_id: u32) -> Result<()> {
        use crate::index::PropertyValue;
        use serde_json::Value as JsonValue;

        // Get property key name
        let property_name = self.catalog.get_key_name(property_key_id)?.ok_or_else(|| {
            Error::CypherExecution(format!("Property key {} not found", property_key_id))
        })?;

        // Get all nodes with this label
        let label_bitmap = self
            .indexes
            .label_index
            .get_nodes_with_labels(&[label_id])?;

        // Iterate through all nodes with this label
        for node_id in label_bitmap.iter() {
            let node_id_u64 = node_id as u64;

            // Load node properties
            if let Some(JsonValue::Object(props)) =
                self.storage.load_node_properties(node_id_u64)?
            {
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
        let mut result_rows = Vec::new();
        let columns = vec!["constraint".to_string(), "message".to_string()];

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

                    // Check if constraint already exists
                    let constraint_exists = constraint_manager
                        .has_constraint(constraint_type, label_id, property_key_id)
                        .unwrap_or(false);

                    // Handle IF NOT EXISTS
                    if create_constraint.if_not_exists && constraint_exists {
                        // Constraint already exists and IF NOT EXISTS was specified, skip
                        let constraint_name = format!(
                            ":{}({}) IS {}",
                            create_constraint.label,
                            create_constraint.property,
                            match constraint_type {
                                catalog::constraints::ConstraintType::Unique => "UNIQUE",
                                catalog::constraints::ConstraintType::Exists => "EXISTS",
                            }
                        );
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(constraint_name.clone()),
                                serde_json::Value::String(
                                    "Constraint already exists, skipped".to_string(),
                                ),
                            ],
                        });
                        continue;
                    }

                    // Create constraint
                    match constraint_manager.create_constraint(
                        constraint_type,
                        label_id,
                        property_key_id,
                    ) {
                        Ok(_) => {
                            // Constraint created successfully
                            let constraint_name = format!(
                                ":{}({}) IS {}",
                                create_constraint.label,
                                create_constraint.property,
                                match constraint_type {
                                    catalog::constraints::ConstraintType::Unique => "UNIQUE",
                                    catalog::constraints::ConstraintType::Exists => "EXISTS",
                                }
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(constraint_name.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {} created",
                                        constraint_name
                                    )),
                                ],
                            });
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
                            let constraint_name = format!(
                                ":{}({}) IS {}",
                                drop_constraint.label,
                                drop_constraint.property,
                                match constraint_type {
                                    catalog::constraints::ConstraintType::Unique => "UNIQUE",
                                    catalog::constraints::ConstraintType::Exists => "EXISTS",
                                }
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(constraint_name.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {} dropped",
                                        constraint_name
                                    )),
                                ],
                            });
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

        // If no rows were added (all commands were skipped), return empty result
        if result_rows.is_empty() {
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        Ok(executor::ResultSet {
            columns,
            rows: result_rows,
        })
    }

    /// Execute function management commands (SHOW FUNCTIONS, CREATE FUNCTION, DROP FUNCTION)
    fn execute_function_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut result_rows = Vec::new();
        let columns = vec!["function".to_string(), "message".to_string()];

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::ShowFunctions => {
                    // List all registered UDFs
                    let udf_names = self.executor.udf_registry().list();

                    // Also get UDFs from catalog (signatures only)
                    let catalog_udfs = self.catalog.list_udfs().unwrap_or_default();

                    // Combine and deduplicate
                    let mut all_functions: std::collections::HashSet<String> =
                        udf_names.into_iter().collect();
                    for name in catalog_udfs {
                        all_functions.insert(name);
                    }

                    // Sort for consistent output
                    let mut sorted_functions: Vec<String> = all_functions.into_iter().collect();
                    sorted_functions.sort();

                    for func_name in sorted_functions {
                        // Try to get signature from catalog
                        let description = if let Ok(Some(sig)) = self.catalog.get_udf(&func_name) {
                            sig.description
                                .unwrap_or_else(|| format!("Function {} registered", func_name))
                        } else {
                            format!("Function {} registered", func_name)
                        };

                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(func_name),
                                serde_json::Value::String(description),
                            ],
                        });
                    }

                    // If no functions, return empty result
                    if result_rows.is_empty() {
                        return Ok(executor::ResultSet {
                            columns: vec!["function".to_string()],
                            rows: vec![],
                        });
                    }
                }
                executor::parser::Clause::ShowConstraints => {
                    // Get all constraints from catalog
                    let constraint_mgr = self.catalog.constraint_manager();
                    let constraints = constraint_mgr.read().get_all_constraints()?;

                    // Sort by label_id and property_key_id for consistent output
                    let mut sorted_constraints: Vec<_> = constraints.into_iter().collect();
                    sorted_constraints.sort_by(|a, b| {
                        a.0.cmp(&b.0) // Sort by (label_id, property_key_id) tuple
                    });

                    for ((label_id, prop_key_id), constraint) in sorted_constraints {
                        // Get label name
                        let label_name = self
                            .catalog
                            .get_label_name(label_id)?
                            .unwrap_or_else(|| format!("Label_{}", label_id));

                        // Get property key name
                        let prop_name = self
                            .catalog
                            .get_key_name(prop_key_id)?
                            .unwrap_or_else(|| format!("Property_{}", prop_key_id));

                        // Determine constraint type string
                        let constraint_type = match constraint.constraint_type {
                            catalog::constraints::ConstraintType::Unique => "UNIQUE",
                            catalog::constraints::ConstraintType::Exists => "EXISTS",
                        };

                        // Create description in Neo4j format
                        let description = match constraint.constraint_type {
                            catalog::constraints::ConstraintType::Unique => {
                                format!(
                                    "CONSTRAINT ON (n:{}) ASSERT n.{} IS UNIQUE",
                                    label_name, prop_name
                                )
                            }
                            catalog::constraints::ConstraintType::Exists => {
                                format!(
                                    "CONSTRAINT ON (n:{}) ASSERT exists(n.{})",
                                    label_name, prop_name
                                )
                            }
                        };

                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(label_name),
                                serde_json::Value::String(prop_name),
                                serde_json::Value::String(constraint_type.to_string()),
                                serde_json::Value::String(description),
                            ],
                        });
                    }

                    // Return result with appropriate columns
                    return Ok(executor::ResultSet {
                        columns: vec![
                            "label".to_string(),
                            "property".to_string(),
                            "type".to_string(),
                            "description".to_string(),
                        ],
                        rows: result_rows,
                    });
                }
                executor::parser::Clause::CreateFunction(create_function) => {
                    // Check if function already exists
                    let function_exists =
                        self.executor.udf_registry().contains(&create_function.name)
                            || self
                                .catalog
                                .get_udf(&create_function.name)
                                .unwrap_or(None)
                                .is_some();

                    if function_exists {
                        if create_function.if_not_exists {
                            // Function already exists and IF NOT EXISTS was specified, skip
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(create_function.name.clone()),
                                    serde_json::Value::String(
                                        "Function already exists, skipped".to_string(),
                                    ),
                                ],
                            });
                            continue;
                        } else {
                            return Err(Error::CypherExecution(format!(
                                "Function '{}' already exists",
                                create_function.name
                            )));
                        }
                    }

                    // Convert parser UdfParameter to udf::UdfParameter
                    let udf_parameters: Vec<crate::udf::UdfParameter> = create_function
                        .parameters
                        .iter()
                        .map(|p| crate::udf::UdfParameter {
                            name: p.name.clone(),
                            param_type: p.param_type.clone(),
                            required: p.required,
                            default: p.default.clone(),
                        })
                        .collect();

                    // Create UDF signature
                    let signature = crate::udf::UdfSignature {
                        name: create_function.name.clone(),
                        parameters: udf_parameters,
                        return_type: create_function.return_type.clone(),
                        description: create_function.description.clone(),
                    };

                    // Store signature in catalog
                    self.catalog.store_udf(&signature)?;

                    // Note: The actual function implementation must be registered via API/plugin system
                    // CREATE FUNCTION only stores the signature
                    result_rows.push(executor::Row {
                        values: vec![
                            serde_json::Value::String(create_function.name.clone()),
                            serde_json::Value::String(format!(
                                "Function signature '{}' stored. Implementation must be registered via API/plugin system.",
                                create_function.name
                            )),
                        ],
                    });
                }
                executor::parser::Clause::DropFunction(drop_function) => {
                    // Check if function exists
                    let function_exists =
                        self.executor.udf_registry().contains(&drop_function.name)
                            || self
                                .catalog
                                .get_udf(&drop_function.name)
                                .unwrap_or(None)
                                .is_some();

                    if !function_exists {
                        if drop_function.if_exists {
                            // Function doesn't exist and IF EXISTS was specified, skip
                            continue;
                        } else {
                            return Err(Error::CypherExecution(format!(
                                "Function '{}' does not exist",
                                drop_function.name
                            )));
                        }
                    }

                    // Remove from UDF registry if registered
                    if self.executor.udf_registry().contains(&drop_function.name) {
                        self.executor
                            .udf_registry_mut()
                            .unregister(&drop_function.name)?;
                    }

                    // Remove from catalog
                    self.catalog.remove_udf(&drop_function.name)?;

                    result_rows.push(executor::Row {
                        values: vec![
                            serde_json::Value::String(drop_function.name.clone()),
                            serde_json::Value::String(format!(
                                "Function '{}' dropped",
                                drop_function.name
                            )),
                        ],
                    });
                }
                _ => {}
            }
        }

        // If no rows were added (all commands were skipped), return empty result
        if result_rows.is_empty() {
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        Ok(executor::ResultSet {
            columns,
            rows: result_rows,
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
                    let path = &load_csv.url[8..]; // Remove "file:///"
                    // On Windows, if path starts with /C:/, remove the leading / to get C:/
                    #[cfg(windows)]
                    {
                        if path.len() >= 3
                            && path.chars().nth(0) == Some('/')
                            && path.chars().nth(1).map(|c| c.is_ascii_alphabetic()) == Some(true)
                            && path.chars().nth(2) == Some(':')
                        {
                            &path[1..]
                        } else {
                            path
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        path
                    }
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
                let content = fs::read_to_string(path).map_err(|e| {
                    Error::CypherExecution(format!("Failed to read CSV file: {}", e))
                })?;

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
                    let row_value: serde_json::Value =
                        fields.into_iter().map(serde_json::Value::String).collect();

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
                        let mut tx = self.transaction_manager.write().begin_write()?;

                        // Execute subquery for this batch
                        let subquery_result = self.execute_cypher_ast(&call_subquery.query)?;

                        if columns.is_empty() {
                            columns = subquery_result.columns.clone();
                        }

                        // Add results for this batch
                        let batch_rows: Vec<_> =
                            subquery_result.rows.into_iter().take(batch_size).collect();
                        if batch_rows.is_empty() {
                            // No more results, commit and break
                            self.transaction_manager.write().commit(&mut tx)?;
                            break;
                        }

                        all_results.extend(batch_rows);
                        batch_count += 1;

                        // Commit transaction for this batch
                        self.transaction_manager.write().commit(&mut tx)?;

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
                                if let Some(serde_json::Value::Object(props_map)) = node_props {
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
            let mut planner = executor::planner::QueryPlanner::new(
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
            let mut planner = executor::planner::QueryPlanner::new(
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
                    | executor::parser::Clause::UseDatabase(_)
            )
        });

        if has_admin_db_cmd {
            return Err(Error::CypherExecution(
                "Database management commands (CREATE/DROP DATABASE, SHOW DATABASES, USE DATABASE) must be executed at server level".to_string(),
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
            return self.execute_transaction_commands(ast, None);
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

        // Check for function management commands
        let has_show_functions = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::ShowFunctions));
        let has_create_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateFunction(_)));
        let has_drop_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropFunction(_)));

        if has_show_functions || has_create_function || has_drop_function {
            return self.execute_function_commands(ast);
        }

        // Check for LOAD CSV commands
        let has_load_csv = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::LoadCsv(_)));

        if has_load_csv {
            return self.execute_load_csv_commands(ast);
        }

        // Check for CALL subquery commands
        let has_call_subquery = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CallSubquery(_)));

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
            let deleted_count = if has_match {
                // MATCH ... DELETE: execute MATCH first, then DELETE with results
                self.execute_match_delete_query(ast)?
            } else {
                // Standalone DELETE won't work without MATCH
                return Err(Error::CypherSyntax(
                    "DELETE requires MATCH clause".to_string(),
                ));
            };
            self.refresh_executor()?;

            // Check if there's a RETURN clause after DELETE
            let return_clause_opt = ast.clauses.iter().find_map(|c| {
                if let executor::parser::Clause::Return(rc) = c {
                    Some(rc)
                } else {
                    None
                }
            });

            if let Some(return_clause) = return_clause_opt {
                // Check if RETURN contains count aggregation
                let mut is_count_only = false;
                let mut count_alias = "count".to_string();

                if return_clause.items.len() == 1 {
                    let executor::parser::ReturnItem { expression, alias } =
                        &return_clause.items[0];
                    if let executor::parser::Expression::FunctionCall { name, args: _ } = expression
                    {
                        if name.to_lowercase() == "count" {
                            is_count_only = true;
                            count_alias = alias.clone().unwrap_or_else(|| "count".to_string());
                        }
                    }
                }

                if is_count_only {
                    // Return count of deleted nodes
                    return Ok(executor::ResultSet {
                        columns: vec![count_alias],
                        rows: vec![executor::Row {
                            values: vec![serde_json::Value::Number(deleted_count.into())],
                        }],
                    });
                } else {
                    // If there's a RETURN clause with other expressions, let the executor handle it
                    // The executor will process the RETURN, but since nodes are deleted,
                    // it will likely return empty results or handle it appropriately
                    let query_obj = executor::Query {
                        cypher: self.query_to_string(ast),
                        params: ast.params.clone(),
                    };
                    return self.executor.execute(&query_obj);
                }
            } else {
                // No RETURN clause - return count of deleted nodes
                return Ok(executor::ResultSet {
                    columns: vec!["count".to_string()],
                    rows: vec![executor::Row {
                        values: vec![serde_json::Value::Number(deleted_count.into())],
                    }],
                });
            }
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
                let result = self.execute_match_create_query(ast, None)?;

                // CRITICAL: Sync executor's store back to engine's storage
                self.storage = self.executor.get_store();

                // Refresh executor to see the changes
                self.refresh_executor()?;

                return Ok(result);
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
        if return_clause.items.is_empty() {
            return Ok(executor::ResultSet {
                columns: vec![],
                rows: vec![],
            });
        }

        // Check if we have any complex expressions (function calls, aggregations)
        // If so, delegate to the full executor by converting to a query
        let has_complex_expressions = return_clause.items.iter().any(|item| {
            !matches!(
                &item.expression,
                executor::parser::Expression::Variable(_)
                    | executor::parser::Expression::PropertyAccess { .. }
            )
        });

        if has_complex_expressions {
            // For complex expressions, we need to use the full executor
            // Build a complete query with the context data materialized
            return self.build_return_result_with_executor(context, return_clause);
        }

        // Simple case: only variables and property access
        // Determine which variable(s) we need nodes from
        let mut var_for_iteration: Option<String> = None;
        let mut columns = Vec::new();

        for item in &return_clause.items {
            let (var, col_name) = match &item.expression {
                executor::parser::Expression::Variable(var) => {
                    let col = item.alias.clone().unwrap_or_else(|| var.clone());
                    (var.clone(), col)
                }
                executor::parser::Expression::PropertyAccess { variable, property } => {
                    let col = item
                        .alias
                        .clone()
                        .unwrap_or_else(|| format!("{}.{}", variable, property));
                    (variable.clone(), col)
                }
                _ => unreachable!("Complex expressions should be handled above"),
            };

            if var_for_iteration.is_none() {
                var_for_iteration = Some(var.clone());
            } else if var_for_iteration.as_ref() != Some(&var) {
                return Err(Error::CypherExecution(
                    "Multiple different variables in RETURN not supported for write queries"
                        .to_string(),
                ));
            }
            columns.push(col_name);
        }

        let var_name = match var_for_iteration {
            Some(v) => v,
            None => {
                return Ok(executor::ResultSet {
                    columns,
                    rows: vec![],
                });
            }
        };

        let node_ids = context.get(&var_name).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();

        for node_id in node_ids {
            if seen.insert(node_id) {
                let mut row_values = Vec::new();

                for item in &return_clause.items {
                    let value = match &item.expression {
                        executor::parser::Expression::Variable(_) => {
                            self.node_to_result_value(node_id)?
                        }
                        executor::parser::Expression::PropertyAccess { property, .. } => {
                            // Get the property value from the node
                            let props = self.storage.load_node_properties(node_id)?;
                            tracing::info!(
                                "[build_return_result] node_id={}, loaded props={:?}",
                                node_id,
                                props
                            );
                            if let Some(Value::Object(map)) = props {
                                let result = map.get(property).cloned().unwrap_or(Value::Null);
                                tracing::info!(
                                    "[build_return_result] property={}, result={:?}",
                                    property,
                                    result
                                );
                                result
                            } else {
                                tracing::info!(
                                    "[build_return_result] property={}, no props found",
                                    property
                                );
                                Value::Null
                            }
                        }
                        _ => Value::Null,
                    };
                    row_values.push(value);
                }

                rows.push(executor::Row { values: row_values });
            }
        }

        Ok(executor::ResultSet { columns, rows })
    }

    fn build_return_result_with_executor(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        // For complex expressions, convert the context into a MATCH query
        // and let the full executor handle it

        // Find the variable name from context
        let var_name = context.keys().next().ok_or_else(|| {
            Error::CypherExecution("No context variable for complex RETURN".to_string())
        })?;

        let node_ids = context.get(var_name).cloned().unwrap_or_default();

        if node_ids.is_empty() {
            // Build empty result with correct columns
            let columns = return_clause
                .items
                .iter()
                .map(|item| item.alias.clone().unwrap_or_else(|| "?column?".to_string()))
                .collect();
            return Ok(executor::ResultSet {
                columns,
                rows: vec![],
            });
        }

        // Build a query like: MATCH (var) WHERE id(var) IN [ids] RETURN ...
        let ids_str = node_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let return_str = return_clause
            .items
            .iter()
            .map(|item| {
                let expr_str = self.expression_to_string(&item.expression);
                if let Some(alias) = &item.alias {
                    format!("{} AS {}", expr_str, alias)
                } else {
                    expr_str
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let query_str = format!(
            "MATCH ({}) WHERE id({}) IN [{}] RETURN {}",
            var_name, var_name, ids_str, return_str
        );

        // Execute through the full executor
        let query_obj = executor::Query {
            cypher: query_str,
            params: std::collections::HashMap::new(),
        };

        self.executor.execute(&query_obj)
    }

    fn expression_to_string(&self, expr: &executor::parser::Expression) -> String {
        match expr {
            executor::parser::Expression::Variable(v) => v.clone(),
            executor::parser::Expression::PropertyAccess { variable, property } => {
                format!("{}.{}", variable, property)
            }
            executor::parser::Expression::FunctionCall { name, args } => {
                let args_str = args
                    .iter()
                    .map(|arg| self.expression_to_string(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, args_str)
            }
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::Integer(n) => n.to_string(),
                executor::parser::Literal::Float(f) => f.to_string(),
                executor::parser::Literal::String(s) => format!("'{}'", s),
                executor::parser::Literal::Boolean(b) => b.to_string(),
                executor::parser::Literal::Null => "null".to_string(),
                _ => "?".to_string(),
            },
            // For other complex expressions, just return a placeholder
            // The full executor will handle them properly
            _ => "?".to_string(),
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new().expect("Failed to create default engine")
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Ensure async WAL writer is properly shut down
        if let Some(ref mut writer) = self.async_wal_writer {
            if let Err(e) = writer.shutdown() {
                tracing::warn!("Failed to shutdown async WAL writer: {}", e);
            }
        }
    }
}
