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
pub mod dynamic_labels;
pub mod graph_scope;
pub mod maintenance;
pub mod stats;
pub mod typed_collections;

#[cfg(test)]
mod tests;

pub use config::{EngineConfig, GraphStatistics};
pub use stats::{EngineStats, HealthState, HealthStatus};

// `NodeWriteState` lives in `crud.rs` alongside the CRUD methods
// that build and consume it; re-import under the short name so the
// cypher-execution code in this file can keep referring to it.
use crud::NodeWriteState;

/// Narrow type-name helper used by constraint error messages.
fn json_type_label(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "NULL",
        serde_json::Value::Bool(_) => "BOOLEAN",
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "INTEGER"
            } else {
                "FLOAT"
            }
        }
        serde_json::Value::String(_) => "STRING",
        serde_json::Value::Array(_) => "LIST",
        serde_json::Value::Object(_) => {
            if crate::executor::eval::bytes::is_bytes_value(v) {
                "BYTES"
            } else {
                "MAP"
            }
        }
    }
}

/// Shared `serde_json::Value → PropertyValue` mapping used by the
/// NODE KEY enforcement path when probing the composite B-tree.
fn json_to_property_value(v: &serde_json::Value) -> crate::index::PropertyValue {
    use crate::index::PropertyValue;
    match v {
        serde_json::Value::Null => PropertyValue::Null,
        serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Null
            }
        }
        serde_json::Value::String(s) => PropertyValue::String(s.clone()),
        _ => PropertyValue::Null,
    }
}

/// Strip a leading `GRAPH[<name>]` preamble from a Cypher query
/// (phase6_opencypher-advanced-types §6). Used when routing a
/// scoped query to a sibling engine via `DatabaseManager` so the
/// target engine does not re-resolve the same preamble and loop.
///
/// Whitespace-tolerant; returns the original string unchanged when
/// no preamble is present. Bracket-aware (handles identifier +
/// optional surrounding spaces inside `[...]`). Uses a single scan
/// so the common case (no preamble) pays only a peek.
fn strip_graph_preamble(query: &str) -> String {
    let trimmed = query.trim_start();
    let leading_ws_len = query.len() - trimmed.len();
    if !trimmed[..trimmed.len().min(5)].eq_ignore_ascii_case("GRAPH") {
        return query.to_string();
    }
    let after_kw = trimmed[5..].trim_start();
    if !after_kw.starts_with('[') {
        return query.to_string();
    }
    if let Some(close_rel) = after_kw[1..].find(']') {
        // close_rel is the offset of `]` inside `after_kw[1..]`.
        let consumed_within_trimmed = (trimmed.len() - after_kw.len()) + 1 + close_rel + 1;
        let total_prefix = leading_ws_len + consumed_within_trimmed;
        return query[total_prefix..].trim_start().to_string();
    }
    query.to_string()
}

#[cfg(test)]
mod graph_preamble_tests {
    use super::strip_graph_preamble;

    #[test]
    fn strips_basic_preamble() {
        assert_eq!(
            strip_graph_preamble("GRAPH[analytics] MATCH (n) RETURN n"),
            "MATCH (n) RETURN n"
        );
    }

    #[test]
    fn tolerates_whitespace_around_brackets() {
        assert_eq!(
            strip_graph_preamble("  GRAPH  [  analytics  ]  MATCH (n) RETURN n"),
            "MATCH (n) RETURN n"
        );
    }

    #[test]
    fn leaves_query_without_preamble_untouched() {
        let q = "MATCH (n:Person) RETURN n";
        assert_eq!(strip_graph_preamble(q), q);
    }

    #[test]
    fn is_case_insensitive_on_keyword() {
        assert_eq!(
            strip_graph_preamble("graph[analytics] RETURN 1"),
            "RETURN 1"
        );
    }
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
    /// Optional cluster-mode quota provider. When set AND a
    /// `UserContext` is supplied to
    /// [`Self::execute_cypher_with_context`], the engine gates
    /// write queries on `check_storage` before execution and
    /// records `record_usage` after a successful write. Standalone
    /// deployments leave this `None` and pay zero overhead.
    ///
    /// Kept as a trait object so the same wire-up works for the
    /// in-process `LocalQuotaProvider` and (eventually) a
    /// HiveHub-backed implementation without touching any of the
    /// code that consults it.
    pub(crate) quota_provider: Option<Arc<dyn crate::cluster::QuotaProvider>>,
    /// Parameters of the currently-executing Cypher query.
    ///
    /// Set by [`Self::execute_cypher_with_params`] before dispatching
    /// the query down the write path (and cleared on the RAII guard in
    /// Drop), read by `apply_set_clause`, `apply_remove_clause`, and
    /// the CREATE-node helpers when resolving `:$param` dynamic labels
    /// (phase6_opencypher-advanced-types §2).
    ///
    /// Empty for callers that use [`Self::execute_cypher`] without
    /// parameters — in that case a query containing a `:$param` label
    /// is rejected with `ERR_INVALID_LABEL`.
    pub(crate) current_params: HashMap<String, Value>,
    /// In-memory registry of `LIST<T>` property-type constraints
    /// (phase6_opencypher-advanced-types §4.3). Keyed by
    /// `(label_id, property_key_id)`; check_constraints consults this
    /// map and rejects writes whose list does not match via
    /// `ERR_CONSTRAINT_VIOLATED`. Persistence through LMDB is a
    /// follow-up that ships with the constraint-DDL grammar extension.
    pub(crate) typed_list_constraints: HashMap<(u32, u32), typed_collections::ListElemType>,
    /// NODE KEY constraints (phase6_opencypher-constraint-enforcement §5).
    pub(crate) node_key_constraints: Vec<crate::constraints::NodeKeyConstraint>,
    /// Relationship NOT NULL constraints (§6).
    pub(crate) rel_not_null_constraints: Vec<crate::constraints::RelNotNullConstraint>,
    /// Property-type constraints — `REQUIRE n.p IS :: <TYPE>` (§7).
    pub(crate) property_type_constraints: Vec<crate::constraints::PropertyTypeConstraint>,
    /// Compatibility flag — when `true`, violations downgrade to a
    /// `warn` log instead of rejecting the write (§10). Default
    /// `false`; scheduled for removal at v1.5.
    pub(crate) relaxed_constraint_enforcement: bool,
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
            quota_provider: None,
            current_params: HashMap::new(),
            typed_list_constraints: HashMap::new(),
            node_key_constraints: Vec::new(),
            rel_not_null_constraints: Vec::new(),
            property_type_constraints: Vec::new(),
            relaxed_constraint_enforcement: false,
            _temp_dir: None,
        };

        // Configure cache in executor for relationship index access
        // Note: In a production implementation, we'd need proper interior mutability
        // For now, the executor will use the cache when available via direct access

        engine.rebuild_indexes_from_storage()?;

        // phase6_opencypher-advanced-types §3.5 — install the
        // composite-B-tree registry on the executor so `db.indexes()`
        // and any future composite-seek planner pass can see it even
        // before the first `refresh_executor` fires.
        engine
            .executor
            .install_composite_btree(engine.indexes.composite_btree.clone());
        engine
            .executor
            .install_fulltext(engine.indexes.fulltext.clone());

        Ok(engine)
    }

    /// Install a cluster-mode quota provider on this engine.
    ///
    /// When set, [`Self::execute_cypher_with_context`] will, for
    /// every request that carries a `UserContext`:
    ///
    /// 1. If the query contains a write clause (CREATE / MERGE /
    ///    SET / DELETE / REMOVE / UNWIND-that-writes), consult
    ///    `provider.check_storage(ns, 0)` to see whether the tenant
    ///    has already exhausted its storage budget. A denial is
    ///    surfaced as [`Error::QuotaExceeded`] before the query
    ///    runs — no wasted work, no partial write.
    /// 2. After a successful execution, charge
    ///    `provider.record_usage(ns, delta)` with a storage-byte
    ///    estimate and a request count of 1.
    ///
    /// Read queries and standalone-mode queries (no `UserContext`)
    /// skip both checks entirely — the Option field stays `None`
    /// on the hot path for non-cluster deployments.
    pub fn with_quota_provider(mut self, provider: Arc<dyn crate::cluster::QuotaProvider>) -> Self {
        self.quota_provider = Some(provider);
        self
    }

    /// Set (or clear) the quota provider after construction —
    /// mirror of [`Self::with_quota_provider`] for callers that
    /// already hold a `&mut Engine` (the server wires the provider
    /// in after NexusServer bootstrapping, not at construction).
    pub fn set_quota_provider(&mut self, provider: Option<Arc<dyn crate::cluster::QuotaProvider>>) {
        self.quota_provider = provider;
    }

    /// Whether this engine has a quota provider installed.
    /// Cheap accessor used by the write-path gate to short-circuit
    /// the entire check when standalone.
    pub fn has_quota_provider(&self) -> bool {
        self.quota_provider.is_some()
    }

    /// Approximate storage bytes owned by a specific tenant
    /// namespace. Sums `node_count * NODE_RECORD_SIZE` across every
    /// label whose catalog name carries the `ns` prefix, plus
    /// `rel_count * REL_RECORD_SIZE` across every scoped
    /// relationship type.
    ///
    /// Returns the raw record-byte total only — property-chain
    /// bytes are NOT included because the catalog has no per-
    /// namespace key index and walking every node's property list
    /// would be O(N) in node count. The post-write flat-rate
    /// heuristic in `execute_cypher_with_context` already
    /// approximates property bytes; this helper is the truthful
    /// lower bound used by reconcilers and admin-level audits to
    /// verify the heuristic hasn't drifted.
    ///
    /// **Caveat on relationship counts.** The current CREATE
    /// operator batches node-count catalog updates but does NOT
    /// increment `catalog.rel_counts` when a relationship is
    /// created (see `executor::operators::create` —
    /// `batch_increment_node_counts` is called, the rel-type
    /// equivalent is not). As a result this function's node total
    /// is accurate but the relationship total is a lower bound,
    /// typically zero. Fixing create.rs to also batch
    /// `increment_rel_count` is a separate follow-up; once that
    /// lands the calculation here needs no change — it's
    /// already summing both columns.
    ///
    /// Under [`crate::cluster::TenantIsolationMode::None`] (or when
    /// the namespace has no catalog entries yet) this returns 0
    /// cleanly — no scan, no error.
    pub fn storage_bytes_for_namespace(&self, ns: &crate::cluster::UserNamespace) -> Result<u64> {
        let ns_prefix = ns.prefix();
        let mut total: u64 = 0;

        for (label_id, name) in self.catalog.list_all_labels() {
            if !name.starts_with(&ns_prefix) {
                continue;
            }
            let count = self.catalog.get_node_count(label_id)?;
            total = total.saturating_add(count.saturating_mul(storage::NODE_RECORD_SIZE as u64));
        }

        for (type_id, name) in self.catalog.list_all_types() {
            if !name.starts_with(&ns_prefix) {
                continue;
            }
            let count = self.catalog.get_rel_count(type_id)?;
            total = total.saturating_add(count.saturating_mul(storage::REL_RECORD_SIZE as u64));
        }

        Ok(total)
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
            quota_provider: None,
            current_params: HashMap::new(),
            typed_list_constraints: HashMap::new(),
            node_key_constraints: Vec::new(),
            rel_not_null_constraints: Vec::new(),
            property_type_constraints: Vec::new(),
            relaxed_constraint_enforcement: false,
            _temp_dir: None,
        };

        engine.rebuild_indexes_from_storage()?;

        // phase6_opencypher-advanced-types §3.5 — install the
        // composite-B-tree registry on the executor so `db.indexes()`
        // and any future composite-seek planner pass can see it even
        // before the first `refresh_executor` fires.
        engine
            .executor
            .install_composite_btree(engine.indexes.composite_btree.clone());
        engine
            .executor
            .install_fulltext(engine.indexes.fulltext.clone());

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
            graph_scope: ast.graph_scope.clone(),
        };

        // Collect all node variables from MATCH and CREATE clauses.
        // phase6 §8 — also consider CREATE-bound variables so patterns
        // like `CREATE (n:BenchCycle) WITH n DELETE n` resolve (the
        // outer caller now admits queries with CREATE-or-MATCH + DELETE).
        let mut node_variables = Vec::new();
        for clause in &match_query.clauses {
            let pattern_opt = match clause {
                executor::parser::Clause::Match(mc) => Some(&mc.pattern),
                executor::parser::Clause::Create(cc) => Some(&cc.pattern),
                _ => None,
            };
            if let Some(pattern) = pattern_opt {
                for element in &pattern.elements {
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

        // Build a synthetic RETURN clause that projects every
        // matched node variable, then attach it to the MATCH-only
        // AST and hand the whole thing to the executor as a
        // preparsed override. Going through an AST override avoids
        // ever re-serialising the scoped label strings (e.g.
        // `ns:alice:Person`) into Cypher and re-parsing them — that
        // round-trip would split on `:` into three separate labels
        // and break cluster-mode isolation on `MATCH … DELETE`.
        //
        // Pre-cluster-mode deployments (`mode = None` in
        // `execute_cypher_with_context`) end up here too, with
        // unscoped labels, and the override path handles them
        // identically — one code path, two modes.
        let return_items: Vec<executor::parser::ReturnItem> = node_variables
            .iter()
            .map(|var| executor::parser::ReturnItem {
                expression: executor::parser::Expression::Variable(var.clone()),
                alias: Some(var.clone()),
            })
            .collect();
        let mut match_query_with_return = match_query.clone();
        match_query_with_return
            .clauses
            .push(executor::parser::Clause::Return(
                executor::parser::ReturnClause {
                    items: return_items,
                    distinct: false,
                },
            ));

        // RAII guard clears the override on every return path so a
        // leftover override cannot leak into an unrelated caller.
        struct OverrideGuard {
            executor: executor::Executor,
        }
        impl Drop for OverrideGuard {
            fn drop(&mut self) {
                self.executor.install_preparsed_ast_override(None);
            }
        }
        self.executor
            .install_preparsed_ast_override(Some(match_query_with_return));
        let _override_guard = OverrideGuard {
            executor: self.executor.clone(),
        };

        let query_obj = executor::Query {
            cypher: String::new(),
            params: ast.params.clone(),
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
                executor::parser::PatternElement::QuantifiedGroup(_) => {
                    return Err(Error::CypherExecution(
                        "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                         are read-only; use a MATCH clause instead"
                            .to_string(),
                    ));
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
                        executor::parser::PatternElement::QuantifiedGroup(_) => {
                            return Err(Error::CypherExecution(
                                "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                                 are read-only; use a MATCH clause instead"
                                    .to_string(),
                            ));
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
            // phase6_opencypher-quickwins §6 — Map literal in SET RHS.
            // Needed for `SET n += {city: 'Berlin'}`; the merge operator
            // evaluates the whole map first, then consults it key-by-key.
            executor::parser::Expression::Map(entries) => {
                let mut out = serde_json::Map::with_capacity(entries.len());
                for (k, v) in entries.iter() {
                    let val = self.evaluate_set_expression(v, target_var, node_props)?;
                    out.insert(k.clone(), val);
                }
                Ok(serde_json::Value::Object(out))
            }
            // Parameter placeholders surface as NULL in this narrow
            // evaluator — parameter-binding lives on the executor side.
            // Treating them as NULL keeps `SET n += $missing` safely a
            // no-op when the parameter is absent.
            executor::parser::Expression::Parameter(_) => Ok(serde_json::Value::Null),
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
        // phase6_opencypher-advanced-types §3.5 — share the composite
        // B-tree registry so `db.indexes()` sees it and the planner can
        // later consult it for seeks.
        self.executor
            .install_composite_btree(self.indexes.composite_btree.clone());
        self.executor
            .install_fulltext(self.indexes.fulltext.clone());
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

    /// Execute a Cypher query with no tenant scoping — the
    /// pre-cluster-mode entry point. Standalone deployments and the
    /// internal test suite use this directly.
    pub fn execute_cypher(&mut self, query: &str) -> Result<executor::ResultSet> {
        self.execute_cypher_with_context(query, None, crate::cluster::TenantIsolationMode::None)
    }

    /// Execute a Cypher query with a client-supplied parameter map.
    ///
    /// The parameters are made visible to every write-path operator
    /// through `self.current_params` for the duration of the call and
    /// cleared on exit (RAII guard, so panics and early-return errors
    /// still release the slot). Currently consumed by the write-side
    /// dynamic-label resolver
    /// ([`dynamic_labels::resolve_labels`]); read-side operators
    /// receive parameters through the existing
    /// [`executor::Query::params`] path.
    pub fn execute_cypher_with_params(
        &mut self,
        query: &str,
        params: HashMap<String, Value>,
    ) -> Result<executor::ResultSet> {
        // Install the parameter map on `self.current_params` for the
        // duration of the call. A RAII guard can't borrow `self`
        // because we also need `&mut self` for the nested call, so
        // we clear manually after — wrapping the call in a closure
        // lets us route both Ok and Err through the same cleanup
        // path without the borrow-checker conflict.
        self.current_params = params;
        let result = self.execute_cypher_with_context(
            query,
            None,
            crate::cluster::TenantIsolationMode::None,
        );
        self.current_params.clear();
        result
    }

    /// Resolve a parser-emitted label list (which may contain `:$param`
    /// sentinels encoded as leading `$` strings) against the current
    /// query parameters. Returns `ERR_INVALID_LABEL` if any sentinel
    /// cannot be resolved. Static-only inputs are returned unchanged.
    /// Register a `LIST<T>` property-type constraint
    /// (phase6_opencypher-advanced-types §4.3).
    ///
    /// Adds an in-memory assertion that every node carrying `label`
    /// and writing `property` must supply a typed list whose element
    /// type matches `elem_type`. Empty lists and NULL always pass
    /// (see spec §4.4). Violations surface as
    /// `ERR_CONSTRAINT_VIOLATED`.
    ///
    /// Registering the same `(label, property)` twice replaces the
    /// previous element type, matching the idempotent shape of other
    /// constraint APIs.
    /// Engine-side CREATE entry-point used when the executor path
    /// cannot be trusted with a dynamic-label pattern
    /// (phase6_opencypher-advanced-types §2). Walks the CREATE
    /// pattern, resolves each node's `:$param` sentinels via
    /// `resolve_dynamic_labels`, and funnels through the engine's
    /// own `create_node` — which re-runs the resolver (fast-path
    /// on static-only inputs) and performs the catalog write.
    ///
    /// Relationships in the pattern are ignored at this entry point
    /// for now; today the dynamic-label feature is scoped to node
    /// labels only, and the CREATE patterns the tests exercise are
    /// node-only.
    fn execute_create_via_engine(&mut self, ast: &executor::parser::CypherQuery) -> Result<()> {
        for clause in &ast.clauses {
            if let executor::parser::Clause::Create(cc) = clause {
                for element in &cc.pattern.elements {
                    if let executor::parser::PatternElement::Node(node) = element {
                        let resolved = self.resolve_dynamic_labels(&node.labels)?;
                        let mut props = serde_json::Map::new();
                        if let Some(pm) = &node.properties {
                            for (k, expr) in &pm.properties {
                                let v = self.expression_to_json_value(expr)?;
                                props.insert(k.clone(), v);
                            }
                        }
                        self.create_node(resolved, serde_json::Value::Object(props))?;
                    }
                }
            }
        }
        self.refresh_executor()?;
        Ok(())
    }

    // ────────── phase6 constraint-enforcement — programmatic APIs ──────────

    /// Flip the relaxed-enforcement flag at runtime.
    /// When `true`, every violation from `check_constraints` /
    /// `enforce_rel_constraints` downgrades to a `warn` log and the
    /// write succeeds. Intended only for data-migration windows;
    /// scheduled for removal at v1.5.
    pub fn set_relaxed_constraint_enforcement(&mut self, relaxed: bool) {
        if relaxed {
            tracing::warn!(
                "relaxed_constraint_enforcement=true — constraint violations will be logged \
                 only, not rejected. This flag is scheduled for removal at v1.5."
            );
        }
        self.relaxed_constraint_enforcement = relaxed;
    }

    /// Register a `REQUIRE (n.p1, n.p2, ...) IS NODE KEY` constraint.
    /// Creates (or reuses) a UNIQUE composite B-tree over the property
    /// list and backfills from existing nodes — CREATE aborts with an
    /// offending-row report if any existing tuple violates uniqueness
    /// or has a NULL component.
    pub fn add_node_key_constraint(
        &mut self,
        label: &str,
        property_keys: &[&str],
        name: Option<&str>,
    ) -> Result<()> {
        if property_keys.is_empty() {
            return Err(Error::CypherSyntax(
                "NODE KEY requires at least one property".to_string(),
            ));
        }
        let label_id = self.catalog.get_or_create_label(label)?;
        for p in property_keys {
            let _ = self.catalog.get_or_create_key(p)?;
        }
        let property_keys: Vec<String> = property_keys.iter().map(|s| s.to_string()).collect();

        // Backfill scan — validate existing data before registering.
        self.backfill_node_key(label_id, label, &property_keys)?;

        // Register the composite index (UNIQUE flag on).
        self.indexes.composite_btree.register(
            label_id,
            property_keys.clone(),
            true,
            name.map(|s| s.to_string()),
            true,
        )?;
        // Track the logical constraint separately so db.constraints()
        // can report it and enforcement checks can route through a
        // single lookup rather than grovelling through the index
        // registry.
        self.node_key_constraints
            .push(crate::constraints::NodeKeyConstraint {
                name: name.map(|s| s.to_string()),
                label_id,
                property_keys,
            });
        Ok(())
    }

    /// Register a `REQUIRE r.p IS NOT NULL` constraint for relationships
    /// of a given type. Backfill rejects existing rels that lack the
    /// property.
    pub fn add_rel_not_null_constraint(
        &mut self,
        rel_type: &str,
        property_key: &str,
        name: Option<&str>,
    ) -> Result<()> {
        let rel_type_id = self.catalog.get_or_create_type(rel_type)?;
        let _ = self.catalog.get_or_create_key(property_key)?;
        self.backfill_rel_not_null(rel_type_id, rel_type, property_key)?;
        self.rel_not_null_constraints
            .push(crate::constraints::RelNotNullConstraint {
                name: name.map(|s| s.to_string()),
                rel_type_id,
                property_key: property_key.to_string(),
            });
        Ok(())
    }

    /// Register a `REQUIRE n.p IS :: <TYPE>` constraint on a node
    /// label. Backfill rejects existing nodes whose value is present
    /// but of a different type.
    pub fn add_property_type_constraint(
        &mut self,
        label: &str,
        property_key: &str,
        ty: crate::constraints::ScalarType,
        name: Option<&str>,
    ) -> Result<()> {
        let label_id = self.catalog.get_or_create_label(label)?;
        let _ = self.catalog.get_or_create_key(property_key)?;
        self.backfill_property_type(label_id, label, property_key, ty)?;
        self.property_type_constraints
            .push(crate::constraints::PropertyTypeConstraint {
                name: name.map(|s| s.to_string()),
                label_id: Some(label_id),
                rel_type_id: None,
                property_key: property_key.to_string(),
                ty,
            });
        Ok(())
    }

    /// Property-type constraint for relationships (`()-[r:T]-()` form).
    pub fn add_rel_property_type_constraint(
        &mut self,
        rel_type: &str,
        property_key: &str,
        ty: crate::constraints::ScalarType,
        name: Option<&str>,
    ) -> Result<()> {
        let rel_type_id = self.catalog.get_or_create_type(rel_type)?;
        let _ = self.catalog.get_or_create_key(property_key)?;
        self.property_type_constraints
            .push(crate::constraints::PropertyTypeConstraint {
                name: name.map(|s| s.to_string()),
                label_id: None,
                rel_type_id: Some(rel_type_id),
                property_key: property_key.to_string(),
                ty,
            });
        Ok(())
    }

    // ────────── Backfill validators (§8) ──────────

    /// Verify every existing node with `label_id` has non-NULL values
    /// for each property in `props`, AND that the tuple is globally
    /// unique. Returns a violation error when the report isn't empty.
    fn backfill_node_key(&self, label_id: u32, label: &str, props: &[String]) -> Result<()> {
        let bitmap = self
            .indexes
            .label_index
            .get_nodes_with_labels(&[label_id])?;
        let mut report = crate::constraints::BackfillReport::default();
        let mut seen: std::collections::HashMap<Vec<String>, u64> =
            std::collections::HashMap::new();
        for nid in bitmap.iter() {
            let nid = nid as u64;
            report.total_scanned += 1;
            let props_value = self.storage.load_node_properties(nid)?;
            let obj = match props_value {
                Some(serde_json::Value::Object(m)) => m,
                _ => {
                    report.record(nid, format!("missing properties on :{label}"));
                    continue;
                }
            };
            let mut tuple: Vec<String> = Vec::with_capacity(props.len());
            let mut bad = false;
            for p in props {
                match obj.get(p) {
                    Some(serde_json::Value::Null) | None => {
                        report.record(nid, format!("property {p:?} is NULL"));
                        bad = true;
                        break;
                    }
                    Some(v) => tuple.push(v.to_string()),
                }
            }
            if bad {
                continue;
            }
            if let Some(prev) = seen.insert(tuple.clone(), nid) {
                report.record(
                    nid,
                    format!("duplicate tuple already present at node {prev}"),
                );
            }
        }
        if report.has_violations() {
            return Err(report.into_error("NODE_KEY"));
        }
        Ok(())
    }

    fn backfill_rel_not_null(
        &self,
        rel_type_id: u32,
        rel_type: &str,
        property_key: &str,
    ) -> Result<()> {
        let mut report = crate::constraints::BackfillReport::default();
        let total = self.storage.relationship_count();
        for rid in 0..total {
            let rec = match self.storage.read_rel(rid) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rec.is_deleted() || rec.type_id != rel_type_id {
                continue;
            }
            report.total_scanned += 1;
            let props = self
                .storage
                .load_relationship_properties(rid)
                .ok()
                .flatten();
            let ok = matches!(
                props.as_ref().and_then(|v| v.as_object()).and_then(|m| m.get(property_key)),
                Some(v) if !matches!(v, serde_json::Value::Null)
            );
            if !ok {
                report.record(
                    rid,
                    format!("rel :{rel_type} missing property {property_key:?}"),
                );
            }
        }
        if report.has_violations() {
            return Err(report.into_error("RELATIONSHIP_PROPERTY_EXISTENCE"));
        }
        Ok(())
    }

    fn backfill_property_type(
        &self,
        label_id: u32,
        label: &str,
        property_key: &str,
        ty: crate::constraints::ScalarType,
    ) -> Result<()> {
        let bitmap = self
            .indexes
            .label_index
            .get_nodes_with_labels(&[label_id])?;
        let mut report = crate::constraints::BackfillReport::default();
        for nid in bitmap.iter() {
            let nid = nid as u64;
            report.total_scanned += 1;
            let props = match self.storage.load_node_properties(nid)? {
                Some(serde_json::Value::Object(m)) => m,
                _ => continue,
            };
            if let Some(v) = props.get(property_key) {
                // NULL is treated as "absent" here — the NOT NULL
                // constraint handles null separately.
                if matches!(v, serde_json::Value::Null) {
                    continue;
                }
                if !ty.accepts(v) {
                    report.record(
                        nid,
                        format!(
                            "node :{label}.{property_key} is {got}, expected {want}",
                            got = json_type_label(v),
                            want = ty.name()
                        ),
                    );
                }
            }
        }
        if report.has_violations() {
            return Err(report.into_error("PROPERTY_TYPE"));
        }
        Ok(())
    }

    // ────────── Write-path enforcement hooks ──────────

    /// Extra constraint checks that run alongside the legacy
    /// `check_constraints` path. Called from every site that writes
    /// node properties. Applies property-type constraints and NODE
    /// KEY uniqueness/NOT-NULL.
    pub(crate) fn enforce_extended_node_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
        exclude_node_id: Option<u64>,
    ) -> Result<()> {
        // Property-type checks (node-scoped).
        if let Some(props) = properties.as_object() {
            for c in &self.property_type_constraints {
                let Some(label_id) = c.label_id else {
                    continue;
                };
                if !label_ids.contains(&label_id) {
                    continue;
                }
                if let Some(v) = props.get(&c.property_key) {
                    if matches!(v, serde_json::Value::Null) {
                        continue;
                    }
                    if !c.ty.accepts(v) {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE property={:?} \
                             expected={} got={}",
                            c.property_key,
                            c.ty.name(),
                            json_type_label(v),
                        ));
                    }
                }
            }
        }

        // NODE KEY: each property present + non-null, tuple unique.
        for nk in &self.node_key_constraints {
            if !label_ids.contains(&nk.label_id) {
                continue;
            }
            let obj = match properties.as_object() {
                Some(m) => m,
                None => continue,
            };
            let mut tuple_vals: Vec<crate::index::PropertyValue> = Vec::new();
            for p in &nk.property_keys {
                match obj.get(p) {
                    None | Some(serde_json::Value::Null) => {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY property={p:?} is NULL"
                        ));
                    }
                    Some(v) => tuple_vals.push(json_to_property_value(v)),
                }
            }
            // Uniqueness against the composite B-tree registry.
            if let Some(idx) = self
                .indexes
                .composite_btree
                .find(nk.label_id, &nk.property_keys)
            {
                let hits = idx.read().seek_exact(&tuple_vals);
                if hits.iter().any(|id| Some(*id) != exclude_node_id) {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY tuple={:?} not unique",
                        nk.property_keys,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Fire extra enforcement for relationship writes. Applies
    /// relationship NOT NULL + property-type constraints.
    pub(crate) fn enforce_rel_constraints(
        &self,
        rel_type_id: u32,
        properties: &serde_json::Value,
    ) -> Result<()> {
        let obj = properties.as_object();
        for c in &self.rel_not_null_constraints {
            if c.rel_type_id != rel_type_id {
                continue;
            }
            let v = obj.and_then(|m| m.get(&c.property_key));
            if !matches!(v, Some(v) if !matches!(v, serde_json::Value::Null)) {
                return self.maybe_violation(format!(
                    "ERR_CONSTRAINT_VIOLATED: kind=RELATIONSHIP_PROPERTY_EXISTENCE \
                     property={:?} must be non-null",
                    c.property_key,
                ));
            }
        }
        if let Some(obj) = obj {
            for c in &self.property_type_constraints {
                let Some(target) = c.rel_type_id else {
                    continue;
                };
                if target != rel_type_id {
                    continue;
                }
                if let Some(v) = obj.get(&c.property_key) {
                    if matches!(v, serde_json::Value::Null) {
                        continue;
                    }
                    if !c.ty.accepts(v) {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE (rel) \
                             property={:?} expected={} got={}",
                            c.property_key,
                            c.ty.name(),
                            json_type_label(v),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Reject writes that would remove a required property / set it to
    /// NULL. Called from `apply_set_clause` / `apply_remove_clause`.
    pub(crate) fn enforce_not_null_on_prop_change(
        &self,
        label_ids: &[u32],
        property_key: &str,
        new_value: Option<&serde_json::Value>,
    ) -> Result<()> {
        // Legacy EXISTS constraint via the catalog.
        let mgr = self.catalog.constraint_manager().read();
        for label_id in label_ids {
            let cs = mgr.get_constraints_for_label(*label_id)?;
            for c in cs {
                if matches!(
                    c.constraint_type,
                    catalog::constraints::ConstraintType::Exists
                ) {
                    let name = self
                        .catalog
                        .get_key_name(c.property_key_id)?
                        .unwrap_or_default();
                    if name == property_key
                        && matches!(new_value, None | Some(serde_json::Value::Null))
                    {
                        return self.maybe_violation(format!(
                            "ERR_CONSTRAINT_VIOLATED: kind=NODE_PROPERTY_EXISTENCE \
                             property={property_key:?} must be non-null",
                        ));
                    }
                }
            }
        }
        // NODE KEY: each component is implicitly NOT NULL.
        for nk in &self.node_key_constraints {
            if !label_ids.contains(&nk.label_id) {
                continue;
            }
            if nk.property_keys.iter().any(|p| p == property_key)
                && matches!(new_value, None | Some(serde_json::Value::Null))
            {
                return self.maybe_violation(format!(
                    "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY component={property_key:?} \
                     cannot be NULL",
                ));
            }
        }
        Ok(())
    }

    /// Reject a label-add when the constraint on that label is
    /// unsatisfied by the current property map.
    pub(crate) fn enforce_add_label_constraints(
        &self,
        label: &str,
        properties: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<()> {
        let label_id = match self.catalog.get_label_id(label) {
            Ok(id) => id,
            Err(_) => return Ok(()), // label not catalogued yet → no constraint can target it
        };
        // Legacy EXISTS constraints.
        let mgr = self.catalog.constraint_manager().read();
        let cs = mgr.get_constraints_for_label(label_id)?;
        for c in cs {
            if matches!(
                c.constraint_type,
                catalog::constraints::ConstraintType::Exists
            ) {
                let prop = self
                    .catalog
                    .get_key_name(c.property_key_id)?
                    .unwrap_or_default();
                if !matches!(properties.get(&prop), Some(v) if !matches!(v, serde_json::Value::Null))
                {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=NODE_PROPERTY_EXISTENCE label={label:?} \
                         property={prop:?} missing while adding label",
                    ));
                }
            }
        }
        drop(mgr);
        // NODE KEY constraints.
        for nk in &self.node_key_constraints {
            if nk.label_id != label_id {
                continue;
            }
            for p in &nk.property_keys {
                if !matches!(properties.get(p), Some(v) if !matches!(v, serde_json::Value::Null)) {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=NODE_KEY label={label:?} component={p:?} \
                         missing while adding label",
                    ));
                }
            }
        }
        // Property-type constraints scoped to this label.
        for c in &self.property_type_constraints {
            if c.label_id != Some(label_id) {
                continue;
            }
            if let Some(v) = properties.get(&c.property_key) {
                if matches!(v, serde_json::Value::Null) {
                    continue;
                }
                if !c.ty.accepts(v) {
                    return self.maybe_violation(format!(
                        "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE label={label:?} \
                         property={:?} expected={} got={}",
                        c.property_key,
                        c.ty.name(),
                        json_type_label(v),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Resolve the pending label-set on a `NodeWriteState` into the
    /// catalog's u32 IDs. Missing labels are skipped (they aren't
    /// catalogued yet, so no constraint can target them).
    pub(crate) fn label_ids_for_state(&self, state: &NodeWriteState) -> Result<Vec<u32>> {
        let mut out = Vec::with_capacity(state.labels.len());
        for lbl in &state.labels {
            if let Ok(id) = self.catalog.get_label_id(lbl) {
                out.push(id);
            }
        }
        Ok(out)
    }

    fn maybe_violation(&self, message: String) -> Result<()> {
        if self.relaxed_constraint_enforcement {
            tracing::warn!("relaxed_constraint_enforcement: {message}");
            Ok(())
        } else {
            Err(Error::ConstraintViolation(message))
        }
    }

    pub fn add_typed_list_constraint(
        &mut self,
        label: &str,
        property: &str,
        elem_type: typed_collections::ListElemType,
    ) -> Result<()> {
        let label_id = self.catalog.get_or_create_label(label)?;
        let key_id = self.catalog.get_or_create_key(property)?;
        self.typed_list_constraints
            .insert((label_id, key_id), elem_type);
        Ok(())
    }

    /// Remove a previously-registered typed-list constraint.
    /// No-op when nothing is registered for the pair.
    pub fn drop_typed_list_constraint(&mut self, label: &str, property: &str) -> Result<()> {
        let Ok(label_id) = self.catalog.get_label_id(label) else {
            return Ok(());
        };
        let Ok(key_id) = self.catalog.get_key_id(property) else {
            return Ok(());
        };
        self.typed_list_constraints.remove(&(label_id, key_id));
        Ok(())
    }

    pub(super) fn resolve_dynamic_labels(&self, labels: &[String]) -> Result<Vec<String>> {
        if !dynamic_labels::contains_dynamic(labels) {
            return Ok(labels.to_vec());
        }
        dynamic_labels::resolve_labels(labels, &self.current_params)
    }

    /// Execute a Cypher query, optionally rewriting catalog-visible
    /// names to the tenant's namespaced form before planning.
    ///
    /// `ctx = None` or `mode = None` short-circuits to the
    /// pre-cluster-mode behaviour — the AST is not touched and the
    /// catalog sees unprefixed names, preserving standalone
    /// compatibility. When cluster mode is active and the
    /// `CatalogPrefix` isolation mode is selected, every label and
    /// relationship-type string in the parsed AST is rewritten
    /// through [`cluster::scope::scope_query`] so the catalog ends
    /// up with distinct IDs per tenant — data isolation follows
    /// transparently through the existing planner and storage.
    ///
    /// This is the single integration point for Phase 2 multi-tenant
    /// scoping. Every other code path inside the engine stays
    /// tenant-oblivious.
    ///
    /// [`cluster::scope::scope_query`]: crate::cluster::scope::scope_query
    pub fn execute_cypher_with_context(
        &mut self,
        query: &str,
        ctx: Option<&crate::cluster::UserContext>,
        mode: crate::cluster::TenantIsolationMode,
    ) -> Result<executor::ResultSet> {
        // Parse query to check if it contains CREATE or DELETE clauses
        let mut parser = executor::parser::CypherParser::new(query.to_string());
        let mut ast = parser.parse()?;

        // phase6_opencypher-advanced-types §6 — honour a leading
        // `GRAPH[name]` preamble. With a `DatabaseManager` wired to
        // the executor, the target database is resolved and either
        // served in place (when it matches the manager's default
        // name) or routed to the owning engine. Without a manager,
        // the scope cannot be resolved and we surface
        // `ERR_GRAPH_NOT_FOUND`.
        if let Some(requested) = ast.graph_scope.clone() {
            match crate::engine::graph_scope::resolve(self, &requested)? {
                crate::engine::graph_scope::ScopedDispatch::AcceptHere => {
                    // Fall through — the rest of this function runs
                    // against `self`, the correct engine.
                }
                crate::engine::graph_scope::ScopedDispatch::Route(target) => {
                    // Strip the preamble from the text query so the
                    // target engine doesn't loop on its own scope
                    // resolver. Parameters and cluster context flow
                    // through verbatim.
                    let cleaned = strip_graph_preamble(query);
                    let mut target_engine = target.write();
                    return target_engine.execute_cypher_with_context(&cleaned, ctx, mode);
                }
            }
        }

        // Cluster-mode scope rewrite. When a UserContext is present
        // AND the isolation mode asks for catalog-level prefixing,
        // rewrite every label / relationship-type in place, then
        // stash the rewritten AST as a one-shot override on the
        // executor. The executor's `execute()` consumes the override
        // exactly once (via `.take()`), so downstream call sites that
        // build a `Query { cypher: query.to_string(), .. }` don't
        // have to pass the scoped AST explicitly — it rides a
        // side-channel on `ExecutorShared`. Without this, the
        // executor's internal re-parse would silently discard the
        // tenant scope.
        //
        // Standalone deployments hit `should_rewrite(None) == false`
        // and the entire block is a no-op — no clone, no mutex take.
        let mut override_installed = false;
        if let Some(user_ctx) = ctx {
            if crate::cluster::scope::should_rewrite(mode) {
                crate::cluster::scope::scope_query(&mut ast, user_ctx.namespace(), mode);
                self.executor
                    .install_preparsed_ast_override(Some(ast.clone()));
                override_installed = true;
            }
        }
        // Ensure the one-shot override slot is cleared even if an
        // early-return path (EXPLAIN, PROFILE, admin command) skips
        // the normal executor.execute() that would consume it. A
        // stale override left on the slot would corrupt the NEXT
        // caller's query — fatal in cluster mode, so the cleanup
        // path uses an RAII guard. The guard owns a clone of the
        // executor (cheap — `Executor` is a thin newtype around
        // `Arc`'d `ExecutorShared`), which side-steps a borrow-
        // checker collision with the `&mut self` methods called
        // further down.
        struct OverrideGuard {
            executor: executor::Executor,
            active: bool,
        }
        impl Drop for OverrideGuard {
            fn drop(&mut self) {
                if self.active {
                    self.executor.install_preparsed_ast_override(None);
                }
            }
        }
        let _override_guard = OverrideGuard {
            executor: self.executor.clone(),
            active: override_installed,
        };

        // Cluster-mode write-path quota gate (Phase 4 §13). Fires
        // only when BOTH a UserContext AND a QuotaProvider are
        // installed — standalone deployments short-circuit on the
        // `has_quota_provider` check and never touch the provider.
        //
        // The check uses `check_storage(ns, 0)`: "is this tenant
        // already at or past its storage ceiling?". We don't yet
        // know the exact byte cost of the query (that's knowable
        // only after planning + partial execution), so the gate is
        // deliberately conservative — an already-exhausted tenant
        // can't grow further, but a tenant right at the edge may
        // sneak one more write in before the post-write
        // `record_usage` pushes them over. That's the right
        // trade-off for a first cut: never reject a write that
        // fits, always reject one that definitely does not.
        let is_write = crate::cluster::scope::is_write_query(&ast);
        if is_write {
            if let (Some(user_ctx), Some(provider)) = (ctx, self.quota_provider.as_ref()) {
                let decision = provider.check_storage(user_ctx.namespace(), 0);
                if let crate::cluster::QuotaDecision::Deny { reason, .. } = decision {
                    return Err(Error::QuotaExceeded(reason));
                }
            }
        }

        // Run the actual dispatch. We separate the post-execution
        // usage-recording step from the dispatch itself so every
        // success path feeds through a single bookkeeping point —
        // there are ~8 `return Ok(...)` sites inside the dispatcher
        // and instrumenting each individually is brittle.
        let dispatch_result = self.execute_cypher_dispatch(&ast, query);

        // Post-write usage charge (Phase 4 §13 / §14.1). Runs once,
        // after a successful write, once the RAII override guard
        // has had its chance to clear state on the error path.
        //
        // `storage_bytes` is a rough fixed heuristic for the first
        // cut: every write charges a baseline of 256 bytes against
        // the tenant. Accurate per-operation accounting (exact
        // record bytes written) needs the planner to thread a size
        // hint back up, which is tracked as a follow-up — under-
        // reporting is safer than over-reporting for the first
        // deployment.
        if is_write && dispatch_result.is_ok() {
            if let (Some(user_ctx), Some(provider)) = (ctx, self.quota_provider.as_ref()) {
                provider.record_usage(
                    user_ctx.namespace(),
                    crate::cluster::UsageDelta {
                        storage_bytes: 256,
                        requests: 1,
                    },
                );
            }
        }

        dispatch_result
    }

    /// Internal dispatcher — the original body of
    /// [`Self::execute_cypher_with_context`] minus the cluster-mode
    /// pre-check and post-record. Split out so the outer function
    /// can bracket every success path with a single
    /// `record_usage` call instead of instrumenting each of the
    /// ~8 `return Ok(...)` sites inside.
    fn execute_cypher_dispatch(
        &mut self,
        ast: &executor::parser::CypherQuery,
        query: &str,
    ) -> Result<executor::ResultSet> {
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
        // phase6_opencypher-advanced-types §5 — route savepoint
        // statements through the transaction-command path so they
        // share session resolution and return a uniform `status`
        // column.
        let has_savepoint_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::Savepoint(_)
                    | executor::parser::Clause::RollbackToSavepoint(_)
                    | executor::parser::Clause::ReleaseSavepoint(_)
            )
        });

        if has_begin || has_commit || has_rollback || has_savepoint_cmd {
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
        // phase6 §8 — a CREATE clause binds node variables too, so
        // `CREATE (n) WITH n DELETE n` (the bench's create-delete
        // cycle) is legal per openCypher even with no MATCH.
        let has_create_bound_vars = ast.clauses.iter().any(|c| {
            if let executor::parser::Clause::Create(cc) = c {
                cc.pattern.elements.iter().any(|el| {
                    if let executor::parser::PatternElement::Node(node) = el {
                        node.variable.is_some()
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        // Handle DELETE (with or without MATCH)
        if has_delete {
            let deleted_count = if has_match || has_create_bound_vars {
                // MATCH ... DELETE or CREATE ... DELETE: execute the
                // upstream pattern first, then DELETE with results.
                self.execute_match_delete_query(&ast)?
            } else {
                // Standalone DELETE won't work without an upstream
                // binding. DELETE n with no MATCH / CREATE / WITH to
                // produce `n` is genuinely invalid.
                return Err(Error::CypherSyntax(
                    "DELETE requires an upstream MATCH, CREATE, or WITH".to_string(),
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
                    // phase6 §8.2 — build an AST for the RETURN tail and
                    // install it as the executor's preparsed-AST override.
                    // Previously this path round-tripped the full AST
                    // through `query_to_string`, whose `format!("{:?}",
                    // clause)` implementation emits the Rust debug shape
                    // (`Create(CreateClause { pattern: ... })`), not
                    // valid Cypher. The executor then re-parsed that
                    // gibberish and failed with a mid-token syntax
                    // error. By handing the executor a pre-built AST
                    // we skip the re-parse entirely, so the CREATE +
                    // DELETE + RETURN shape (bench's
                    // `write.create_delete_cycle`) executes cleanly.
                    let tail_ast = executor::parser::CypherQuery {
                        clauses: vec![executor::parser::Clause::Return(return_clause.clone())],
                        params: ast.params.clone(),
                        graph_scope: ast.graph_scope.clone(),
                    };
                    struct OverrideGuard {
                        executor: executor::Executor,
                    }
                    impl Drop for OverrideGuard {
                        fn drop(&mut self) {
                            self.executor.install_preparsed_ast_override(None);
                        }
                    }
                    self.executor.install_preparsed_ast_override(Some(tail_ast));
                    let _guard = OverrideGuard {
                        executor: self.executor.clone(),
                    };
                    let query_obj = executor::Query {
                        cypher: String::new(),
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
            // Just refresh after to see changes. Attach the scoped AST
            // via `preparsed_ast` so cluster-mode label rewrites survive
            // the executor's parse step.
            //
            // phase6_opencypher-advanced-types §2 — if the CREATE
            // pattern contains a `:$param` dynamic-label sentinel, we
            // can't hand it to the executor's CREATE operator because
            // that path would register `"$ident"` as a literal label
            // in the catalog. Instead, route through the engine's
            // own write path which resolves the sentinel against
            // `self.current_params` before reaching the catalog.
            let has_dynamic_labels = ast.clauses.iter().any(|c| {
                if let executor::parser::Clause::Create(cc) = c {
                    cc.pattern.elements.iter().any(|e| {
                        if let executor::parser::PatternElement::Node(n) = e {
                            crate::engine::dynamic_labels::contains_dynamic(&n.labels)
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            });
            if has_dynamic_labels {
                self.execute_create_via_engine(&ast)?;
                return Ok(executor::ResultSet {
                    columns: vec!["status".to_string()],
                    rows: vec![executor::Row {
                        values: vec![serde_json::Value::String("ok".to_string())],
                    }],
                });
            }
            let query_obj = executor::Query {
                cypher: query.to_string(),
                params: self.current_params.clone(),
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

        // Execute the query normally. Attach the scoped AST so the
        // cluster-mode label rewrite (performed at the top of this
        // function) survives the executor's re-parse.
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

        // Async flush — matches the CREATE / executor-side write paths,
        // which use `flush_async` as well. The SYNC `flush()` here used
        // to dominate write-query latency (5-10ms per call on spinning
        // media; 2-3ms even on NVMe) because mmap page syncs are
        // OS-level operations. With the WAL already providing
        // durability on commit, this full sync is redundant on the hot
        // path — causing the bench's `write.set_property` and
        // `constraint.not_null_set` to run 2× slower than Neo4j. Callers
        // that genuinely need on-disk durability can issue an explicit
        // `flush()` after the write.
        self.storage.flush_async()?;
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
                        // phase6_opencypher-constraint-enforcement —
                        // run NOT NULL guard for this node's labels
                        // (existing + staged), and the property-type
                        // check against the new value.
                        let label_ids = self.label_ids_for_state(state)?;
                        self.enforce_not_null_on_prop_change(
                            &label_ids,
                            property,
                            Some(&json_value),
                        )?;
                        // Check property-type constraint against the
                        // specific value being written.
                        if !matches!(json_value, serde_json::Value::Null) {
                            for c in &self.property_type_constraints {
                                if c.property_key != *property {
                                    continue;
                                }
                                let Some(label_id) = c.label_id else { continue };
                                if !label_ids.contains(&label_id) {
                                    continue;
                                }
                                if !c.ty.accepts(&json_value) {
                                    return Err(Error::ConstraintViolation(format!(
                                        "ERR_CONSTRAINT_VIOLATED: kind=PROPERTY_TYPE \
                                         property={:?} expected={} got={}",
                                        c.property_key,
                                        c.ty.name(),
                                        json_type_label(&json_value),
                                    )));
                                }
                            }
                        }
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

                    // phase6_opencypher-advanced-types §2 — resolve
                    // `:$param` in SET position. A single parser-emitted
                    // label may fan out to multiple names when the
                    // parameter is a `LIST<STRING>`.
                    let resolved = self.resolve_dynamic_labels(std::slice::from_ref(label))?;
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        for lbl in &resolved {
                            // phase6_opencypher-constraint-enforcement §4 —
                            // adding a label whose NOT NULL constraint is
                            // not satisfied by the current property bag
                            // must fail before the label lands on the
                            // pending state.
                            self.enforce_add_label_constraints(lbl, &state.properties)?;
                            state.labels.insert(lbl.clone());
                        }
                    }
                }
                // phase6_opencypher-quickwins §6 — `SET lhs += mapExpr`.
                executor::parser::SetItem::MapMerge { target, map } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        let evaluated =
                            self.evaluate_set_expression(map, target, &state.properties)?;
                        match evaluated {
                            Value::Null => {
                                // NULL RHS is a no-op — preserves current bag.
                            }
                            Value::Object(rhs) => {
                                for (k, v) in rhs.into_iter() {
                                    if matches!(v, Value::Null) {
                                        state.properties.remove(&k);
                                    } else {
                                        state.properties.insert(k, v);
                                    }
                                }
                            }
                            other => {
                                return Err(Error::CypherExecution(format!(
                                    "ERR_SET_NON_MAP: SET {} += <rhs> requires a MAP or NULL \
                                     (got {})",
                                    target,
                                    match other {
                                        Value::Bool(_) => "BOOLEAN",
                                        Value::Number(n) => {
                                            if n.is_i64() || n.is_u64() {
                                                "INTEGER"
                                            } else {
                                                "FLOAT"
                                            }
                                        }
                                        Value::String(_) => "STRING",
                                        Value::Array(_) => "LIST",
                                        _ => "?",
                                    }
                                )));
                            }
                        }
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
                        // phase6_opencypher-constraint-enforcement §4/§5 —
                        // reject REMOVE of a NOT NULL / NODE KEY
                        // component before mutating the pending
                        // property bag.
                        let label_ids = self.label_ids_for_state(state)?;
                        self.enforce_not_null_on_prop_change(&label_ids, property, None)?;
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

                    // phase6_opencypher-advanced-types §2 — resolve
                    // `:$param` in REMOVE position (same semantics as
                    // SET, inverted operation).
                    let resolved = self.resolve_dynamic_labels(std::slice::from_ref(label))?;
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        for lbl in &resolved {
                            state.labels.remove(lbl);
                        }
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
                // phase6_opencypher-advanced-types §5 — savepoint
                // lifecycle statements. All three require an active
                // explicit transaction; outside one they raise
                // ERR_SAVEPOINT_NO_TX.
                executor::parser::Clause::Savepoint(s) => {
                    // phase6_opencypher-advanced-types §5 — SAVEPOINT
                    // outside an explicit tx must return ERR_SAVEPOINT_NO_TX,
                    // not a generic session-not-found error. Autovivify
                    // a session here so the no-tx check runs even for
                    // first-call clients.
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());
                    if !session.has_active_transaction() {
                        return Err(Error::CypherExecution(
                            "ERR_SAVEPOINT_NO_TX: SAVEPOINT outside an explicit transaction"
                                .to_string(),
                        ));
                    }
                    session.savepoints.push(
                        &s.name,
                        transaction::SavepointMarker {
                            undo_log_offset: session.created_nodes.len(),
                            staged_ops_offset: session.created_relationships.len(),
                        },
                    );
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::RollbackToSavepoint(s) => {
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());
                    if !session.has_active_transaction() {
                        return Err(Error::CypherExecution(
                            "ERR_SAVEPOINT_NO_TX: ROLLBACK TO SAVEPOINT outside an explicit \
                             transaction"
                                .to_string(),
                        ));
                    }
                    let marker = session.savepoints.rollback_to(&s.name)?;
                    // Replay node undo-log: every node created after
                    // the marker's offset gets marked deleted and
                    // pulled from the label index. Relationships
                    // follow the same pattern.
                    let to_undo_nodes: Vec<u64> = session
                        .created_nodes
                        .drain(marker.undo_log_offset..)
                        .collect();
                    let to_undo_rels: Vec<u64> = session
                        .created_relationships
                        .drain(marker.staged_ops_offset..)
                        .collect();
                    for node_id in &to_undo_nodes {
                        let _ = self.storage.delete_node(*node_id);
                        if let Ok(Some(serde_json::Value::Object(props))) =
                            self.storage.load_node_properties(*node_id)
                        {
                            let property_index = self.cache.property_index_manager();
                            for prop_name in props.keys() {
                                let _ = property_index.remove_property(prop_name, *node_id);
                            }
                        }
                        if let Ok(_record) = self.storage.read_node(*node_id) {
                            let _ = self.indexes.label_index.remove_node(*node_id);
                        }
                    }
                    for rel_id in &to_undo_rels {
                        let _ = self.storage.delete_rel(*rel_id);
                    }
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::ReleaseSavepoint(s) => {
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());
                    if !session.has_active_transaction() {
                        return Err(Error::CypherExecution(
                            "ERR_SAVEPOINT_NO_TX: RELEASE SAVEPOINT outside an explicit \
                             transaction"
                                .to_string(),
                        ));
                    }
                    session.savepoints.release(&s.name)?;
                    self.session_manager.update_session(session);
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
                    // phase6_opencypher-advanced-types §3 — composite
                    // B-tree: any index defined over 2+ properties goes
                    // to the dedicated composite registry, not the
                    // single-column property index.
                    if create_index.properties.len() > 1 {
                        let label_id = self.catalog.get_or_create_label(&create_index.label)?;
                        for prop in &create_index.properties {
                            let _ = self.catalog.get_or_create_key(prop)?;
                        }
                        self.indexes.composite_btree.register(
                            label_id,
                            create_index.properties.clone(),
                            false,
                            create_index.name.clone(),
                            create_index.if_not_exists,
                        )?;
                        let joined = create_index.properties.join(", ");
                        let index_name = format!(":{}({})", create_index.label, joined);
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(index_name),
                                serde_json::Value::String("Composite index created".to_string()),
                            ],
                        });
                        continue;
                    }
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
        // Note on locking: the legacy UNIQUE / EXISTS path takes the
        // constraint-manager write lock lazily inside each branch so
        // the extended-kind path (NODE_KEY / PROPERTY_TYPE /
        // RELATIONSHIP_PROPERTY_EXISTENCE) can take `&mut self` for
        // the programmatic registration APIs without a borrow clash.
        let mut result_rows = Vec::new();
        let columns = vec!["constraint".to_string(), "message".to_string()];

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::CreateConstraint(create_constraint) => {
                    // phase6_opencypher-constraint-enforcement — NODE
                    // KEY, relationship NOT NULL, and property-type
                    // constraints route through the extended
                    // registration APIs. The legacy UNIQUE / EXISTS
                    // path stays on the LMDB-backed constraint
                    // manager below.
                    match create_constraint.constraint_type {
                        executor::parser::ConstraintType::NodeKey => {
                            let props: Vec<&str> = create_constraint
                                .properties
                                .iter()
                                .map(|s| s.as_str())
                                .collect();
                            self.add_node_key_constraint(
                                &create_constraint.label,
                                &props,
                                create_constraint.name.as_deref(),
                            )?;
                            let display = format!(
                                "NODE_KEY :{} ({})",
                                create_constraint.label,
                                create_constraint.properties.join(", "),
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(display.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {display} created"
                                    )),
                                ],
                            });
                            continue;
                        }
                        executor::parser::ConstraintType::PropertyType => {
                            let ty_name =
                                create_constraint.property_type.clone().unwrap_or_default();
                            let ty = crate::constraints::ScalarType::parse(&ty_name)?;
                            match create_constraint.entity {
                                executor::parser::ConstraintEntity::Node => {
                                    self.add_property_type_constraint(
                                        &create_constraint.label,
                                        &create_constraint.property,
                                        ty,
                                        create_constraint.name.as_deref(),
                                    )?;
                                }
                                executor::parser::ConstraintEntity::Relationship => {
                                    self.add_rel_property_type_constraint(
                                        &create_constraint.label,
                                        &create_constraint.property,
                                        ty,
                                        create_constraint.name.as_deref(),
                                    )?;
                                }
                            }
                            let display = format!(
                                "PROPERTY_TYPE :{}({}) IS :: {}",
                                create_constraint.label,
                                create_constraint.property,
                                ty.name()
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(display.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {display} created"
                                    )),
                                ],
                            });
                            continue;
                        }
                        executor::parser::ConstraintType::Exists
                            if matches!(
                                create_constraint.entity,
                                executor::parser::ConstraintEntity::Relationship
                            ) =>
                        {
                            self.add_rel_not_null_constraint(
                                &create_constraint.label,
                                &create_constraint.property,
                                create_constraint.name.as_deref(),
                            )?;
                            let display = format!(
                                "RELATIONSHIP_PROPERTY_EXISTENCE :{}({})",
                                create_constraint.label, create_constraint.property,
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(display.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {display} created"
                                    )),
                                ],
                            });
                            continue;
                        }
                        _ => {}
                    }
                    // Get label ID
                    let label_id = self.catalog.get_or_create_label(&create_constraint.label)?;

                    // Get property key ID
                    let property_key_id = self
                        .catalog
                        .get_or_create_key(&create_constraint.property)?;

                    // Convert parser constraint type to catalog constraint type.
                    // NODE_KEY and PROPERTY_TYPE were already handled
                    // above; only UNIQUE and (node-scope) EXISTS reach
                    // this point.
                    let constraint_type = match create_constraint.constraint_type {
                        executor::parser::ConstraintType::Unique => {
                            catalog::constraints::ConstraintType::Unique
                        }
                        executor::parser::ConstraintType::Exists => {
                            catalog::constraints::ConstraintType::Exists
                        }
                        executor::parser::ConstraintType::NodeKey
                        | executor::parser::ConstraintType::PropertyType => {
                            unreachable!("handled above")
                        }
                    };

                    // Take the constraint-manager write lock only
                    // for the legacy path — the extended-kind
                    // registration above needs &mut self and can't
                    // share the lock.
                    let mut constraint_manager = self.catalog.constraint_manager().write();

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
                        // NODE_KEY / PROPERTY_TYPE drop is a no-op in
                        // this release — the in-memory extended
                        // registry is recreated per engine lifetime
                        // and DROP CONSTRAINT wiring for the new
                        // kinds lands alongside the LMDB persistence
                        // follow-up. Report success so DDL scripts
                        // stay idempotent.
                        executor::parser::ConstraintType::NodeKey
                        | executor::parser::ConstraintType::PropertyType => {
                            continue;
                        }
                    };

                    let mut constraint_manager = self.catalog.constraint_manager().write();

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
                    // phase6_opencypher-subquery-transactions — the
                    // extended suffix clauses (IN CONCURRENT, ON ERROR
                    // non-FAIL, REPORT STATUS) land with the planner
                    // operator in a later slice of the task. Reject
                    // loudly here instead of silently ignoring fields
                    // the caller spelled out, so production users do
                    // not get FAIL semantics when they asked for
                    // RETRY / CONTINUE / BREAK.
                    if call_subquery.concurrency.is_some() {
                        return Err(Error::CypherExecution(
                            "ERR_CALL_IN_TX_NOT_IMPLEMENTED: \
                             IN CONCURRENT TRANSACTIONS lands with \
                             the planner operator in a follow-up \
                             slice of phase6_opencypher-subquery-\
                             transactions"
                                .to_string(),
                        ));
                    }
                    if !matches!(
                        call_subquery.on_error,
                        executor::parser::OnErrorPolicy::Fail
                    ) {
                        return Err(Error::CypherExecution(
                            "ERR_CALL_IN_TX_NOT_IMPLEMENTED: \
                             ON ERROR CONTINUE / BREAK / RETRY \
                             lands with the planner operator in a \
                             follow-up slice of \
                             phase6_opencypher-subquery-transactions"
                                .to_string(),
                        ));
                    }
                    if call_subquery.status_var.is_some() {
                        return Err(Error::CypherExecution(
                            "ERR_CALL_IN_TX_NOT_IMPLEMENTED: \
                             REPORT STATUS AS <var> lands with the \
                             planner operator in a follow-up slice \
                             of phase6_opencypher-subquery-\
                             transactions"
                                .to_string(),
                        ));
                    }
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
        // phase6_opencypher-advanced-types §4.3 — typed-list
        // constraint enforcement. Run first so a clearly-typed
        // violation short-circuits before we touch the single-column
        // UNIQUE / EXISTS machinery.
        if !self.typed_list_constraints.is_empty() {
            if let Some(props) = properties.as_object() {
                for &label_id in label_ids {
                    for ((lbl, key_id), elem_type) in &self.typed_list_constraints {
                        if *lbl != label_id {
                            continue;
                        }
                        let key_name = match self.catalog.get_key_name(*key_id)? {
                            Some(n) => n,
                            None => continue,
                        };
                        if let Some(val) = props.get(&key_name) {
                            typed_collections::validate_list(val, *elem_type)?;
                        }
                    }
                }
            }
        }

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
        let has_savepoint_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::Savepoint(_)
                    | executor::parser::Clause::RollbackToSavepoint(_)
                    | executor::parser::Clause::ReleaseSavepoint(_)
            )
        });

        if has_begin || has_commit || has_rollback || has_savepoint_cmd {
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
        // phase6 §8 — CREATE-bound variables satisfy DELETE's context
        // requirement too, matching openCypher semantics.
        let has_create_bound_vars = ast.clauses.iter().any(|c| {
            if let executor::parser::Clause::Create(cc) = c {
                cc.pattern.elements.iter().any(|el| {
                    if let executor::parser::PatternElement::Node(node) = el {
                        node.variable.is_some()
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        // Handle DELETE (with or without MATCH)
        if has_delete {
            let deleted_count = if has_match || has_create_bound_vars {
                // MATCH ... DELETE or CREATE ... DELETE: execute the
                // upstream pattern first, then DELETE with results.
                self.execute_match_delete_query(ast)?
            } else {
                return Err(Error::CypherSyntax(
                    "DELETE requires an upstream MATCH, CREATE, or WITH".to_string(),
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
