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

// Extracted impl-block modules (engine/mod.rs split).
mod constraints;
mod ddl;
mod match_exec;
mod query_pipeline;
mod transactions;
mod write_exec;

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
    /// Mutation counters for the currently-executing top-level Cypher
    /// query. Reset at the same point as `current_params` (query
    /// entry), incremented by the write-path CRUD helpers (e.g.
    /// `create_node_inner`), and copied onto the returned
    /// `ResultSet::side_effects` before the entry-point call returns.
    /// Mirrors `current_params`'s reset/clear lifecycle.
    pub(crate) side_effects: executor::types::SideEffects,
    /// Per-iteration UNWIND row bindings for the write path (issue #13).
    /// `UNWIND [...] AS row MERGE/SET ...` runs the downstream write
    /// clauses once per row; this map binds the loop variable (e.g.
    /// `row` -> `{"id":"a"}`) for the current iteration so the write-path
    /// expression evaluators can resolve `row` / `row.id`. Empty outside an
    /// UNWIND-write iteration. Mirrors `current_params`.
    pub(crate) unwind_bindings: HashMap<String, Value>,
    /// Set when an in-memory relationship-index update fails (issue #18) so
    /// the index may have a missing `(src,type,dst)` entry. The next
    /// `find_relationship_between` lazily rebuilds the relationship index from
    /// storage and clears the flag, restoring the O(1) exact-edge fast path
    /// (correctness is preserved meanwhile by the authoritative chain-walk
    /// fallback). Avoids rebuilding mid-write.
    pub(crate) relationship_index_dirty: std::sync::atomic::AtomicBool,
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
    /// External-id reservations made during the current session write
    /// transaction.  Each entry is `(internal_id, external_id)` as recorded
    /// inside `create_node_inner` when `put_if_absent` succeeded.
    ///
    /// On session-transaction abort, `rollback_external_id_reservations`
    /// iterates this list and removes each mapping from the catalog index so
    /// no dangling forward/reverse entries are left behind.  The field is
    /// cleared (drained) by both the commit and abort paths.
    pub(crate) pending_external_ids: Vec<(u64, crate::storage::external_id::ExternalId)>,
    /// Deferred temporary-directory cleanup guard.
    ///
    /// For an engine built on a self-cleaning temporary store
    /// ([`Self::new`]) this holds an independent clone of the store's
    /// `Arc<TempDirGuard>`. Declared LAST so the compiler-generated drop
    /// glue runs it after every other field — `catalog` (LMDB), `wal`,
    /// `indexes` (Tantivy full-text), `executor` (which holds its own
    /// store/index clones), and `page_cache` — has dropped and released
    /// every file handle inside the temp directory. The guard's
    /// `remove_dir_all` only fires when its last `Arc` clone drops, so
    /// tying that last clone to Engine's final field guarantees removal
    /// runs with no open handles left in the tree — required on Windows,
    /// where a still-open catalog/WAL/index handle blocked removal and left
    /// a small residual of temp dirs behind (the tail of
    /// `phase0_fix-tempdir-record-store-leak`). `None` for persistent
    /// engines ([`Self::with_data_dir`], [`Self::with_isolated_catalog`]),
    /// which must never auto-delete a caller-provided data directory.
    _temp_dir_cleanup: Option<Arc<storage::TempDirGuard>>,
}

impl Engine {
    /// Create a new engine instance with all components
    /// Uses temporary directory (for backward compatibility)
    ///
    /// The temporary directory will be automatically cleaned up when the Engine is dropped.
    /// For persistent storage, use `Engine::with_data_dir()` instead.
    pub fn new() -> Result<Self> {
        // Self-cleaning temp directory: `RecordStore::new_temporary()`
        // attaches a reference-counted cleanup guard to the store (see
        // its doc comment) that removes the directory once every clone
        // of the store — including the one held by `self.executor` — has
        // dropped, i.e. when this `Engine` itself drops. No dedicated
        // `TempDir` field is needed on `Engine` anymore: removal is tied
        // to the store's own lifetime, not to a timer, so a long-running
        // server process is unaffected until shutdown.
        let storage = storage::RecordStore::new_temporary()?;
        let data_dir = storage.path().to_path_buf();
        Self::bootstrap_with_storage(&data_dir, EngineConfig::default(), storage)
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

        // Initialize record store (persistent — never auto-deletes `data_dir`)
        let storage = storage::RecordStore::new(data_dir)?;

        Self::bootstrap_with_storage(data_dir, config, storage)
    }

    /// Shared bootstrap tail for [`Self::new`] (temp-directory,
    /// self-cleaning) and [`Self::with_data_dir_and_config`] (persistent):
    /// builds the catalog, page cache, WAL, transaction/session managers,
    /// indexes, executor, and cache system given an already-open
    /// `storage`, so the two entry points cannot drift apart.
    fn bootstrap_with_storage(
        data_dir: &std::path::Path,
        config: EngineConfig,
        storage: storage::RecordStore,
    ) -> Result<Self> {
        // Initialize catalog
        let catalog = catalog::Catalog::new(data_dir.join("catalog.mdb"))?;

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

        // Capture the store's temp-dir cleanup guard (an `Arc` clone; `None`
        // for a persistent store) BEFORE `storage` is moved into the struct,
        // so it can be stashed as Engine's last-dropped field and the
        // directory removed only after catalog/WAL/index handles close.
        let temp_dir_cleanup = storage.temp_dir_guard();
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
            side_effects: executor::types::SideEffects::default(),
            unwind_bindings: HashMap::new(),
            relationship_index_dirty: std::sync::atomic::AtomicBool::new(false),
            typed_list_constraints: HashMap::new(),
            node_key_constraints: Vec::new(),
            rel_not_null_constraints: Vec::new(),
            property_type_constraints: Vec::new(),
            relaxed_constraint_enforcement: false,
            pending_external_ids: Vec::new(),
            _temp_dir_cleanup: temp_dir_cleanup,
        };

        // Configure cache in executor for relationship index access
        // Note: In a production implementation, we'd need proper interior mutability
        // For now, the executor will use the cache when available via direct access

        engine.rebuild_indexes_from_storage()?;
        engine.recover_external_ids_from_wal()?;

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
        // phase6_spatial-index-autopopulate §1.2 — install the R-tree
        // registry at construction so spatial DDL and queries work even
        // before the first `refresh_executor` fires.
        engine.executor.install_rtree(engine.indexes.rtree.clone());
        // phase6_fix-read-match-index-seek §2 — install the typed property
        // index (Arc-shared) at construction so a `CREATE INDEX` followed by
        // a read `MATCH (n:L {p: v})` uses the index seek even before the
        // first `refresh_executor` fires. The clone shares state, so later
        // index registrations are visible without re-installing.
        engine
            .executor
            .install_property_index(engine.indexes.property_index.clone());

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

        // Persistent store — this yields `None`; captured before `storage`
        // is moved so the last-dropped `_temp_dir_cleanup` field is always
        // populated from the store's actual guard.
        let temp_dir_cleanup = storage.temp_dir_guard();
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
            side_effects: executor::types::SideEffects::default(),
            unwind_bindings: HashMap::new(),
            relationship_index_dirty: std::sync::atomic::AtomicBool::new(false),
            typed_list_constraints: HashMap::new(),
            node_key_constraints: Vec::new(),
            rel_not_null_constraints: Vec::new(),
            property_type_constraints: Vec::new(),
            relaxed_constraint_enforcement: false,
            pending_external_ids: Vec::new(),
            _temp_dir_cleanup: temp_dir_cleanup,
        };

        engine.rebuild_indexes_from_storage()?;
        engine.recover_external_ids_from_wal()?;

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
        engine.executor.install_rtree(engine.indexes.rtree.clone());
        // phase6_fix-read-match-index-seek §2 — install the typed property
        // index (Arc-shared) at construction so read-side index seeks work
        // before the first `refresh_executor` fires.
        engine
            .executor
            .install_property_index(engine.indexes.property_index.clone());

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

        // Rebuild the in-memory relationship index (type / node / exact-edge)
        // from storage. Without this the index is empty after a restart, so
        // the O(1) `(src, type, dst)` MERGE existence fast path never engages
        // on a re-bootstrap — precisely the write-burst scenario this index
        // exists to keep linear. Correctness does not depend on it (the
        // existence check falls back to the chain walk), but the perf win does.
        self.rebuild_relationship_index_from_storage();

        // Rebuild the typed property index from the durable definitions
        // (issue #11). `CREATE INDEX` persists each `(label_id, key_id)` pair;
        // without this rebuild they would be lost on restart and every
        // `MATCH (n:L {p:v})` / index-backed MERGE would silently fall back to
        // an O(N) label scan until the client re-issued `CREATE INDEX`.
        for (label_id, key_id) in self.catalog.list_property_indexes().unwrap_or_default() {
            if let Err(e) = self.indexes.property_index.create_index(label_id, key_id) {
                tracing::warn!(
                    "property-index rebuild: create_index({label_id},{key_id}) failed: {e}"
                );
                continue;
            }
            if let Err(e) = self.populate_index(label_id, key_id) {
                tracing::warn!("property-index rebuild: populate({label_id},{key_id}) failed: {e}");
            }
        }

        Ok(())
    }

    /// Clear and rebuild the in-memory relationship index (type / node /
    /// exact-edge) from storage. Reusable by the startup rebuild and the
    /// lazy self-heal after a failed incremental update (#18).
    fn rebuild_relationship_index_from_storage(&self) {
        let rel_index = self.cache.relationship_index();
        rel_index.clear().ok();
        let total_rels = self.storage.relationship_count();
        for rel_id in 0..total_rels {
            let rel = match self.storage.read_rel(rel_id) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rel.is_deleted() {
                continue;
            }
            // packed struct: copy fields to locals before use.
            let (src, dst, type_id) = (rel.src_id, rel.dst_id, rel.type_id);
            if let Err(e) = rel_index.add_relationship(rel_id, src, dst, type_id) {
                tracing::warn!("relationship-index rebuild: rel {rel_id} skipped: {e}");
            }
        }
    }

    /// If a prior incremental relationship-index update failed (#18), the
    /// exact-edge index may be missing entries. Rebuild it once from storage
    /// (the authoritative source) and clear the dirty flag, restoring the
    /// O(1) fast path. No-op when the index is clean.
    fn heal_relationship_index_if_dirty(&self) {
        use std::sync::atomic::Ordering;
        if self.relationship_index_dirty.swap(false, Ordering::AcqRel) {
            tracing::warn!(
                "relationship index marked dirty after a failed incremental update; \
                 rebuilding from storage to restore the exact-edge fast path (#18)"
            );
            self.rebuild_relationship_index_from_storage();
        }
    }

    /// Replay `ExternalIdAssigned` WAL entries to rebuild the catalog's
    /// external-id index after a crash.
    ///
    /// LMDB is itself crash-safe: if the LMDB write-transaction committed
    /// before the crash, the mapping is already present and
    /// `put_if_absent` will silently skip the duplicate.  If LMDB had not
    /// committed (e.g. the process was killed between the storage write
    /// and the LMDB commit), this replay re-installs the mapping from the
    /// WAL so the catalog index is consistent with the on-disk records.
    ///
    /// Called once at engine startup, after WAL flush, before any
    /// queries are served.
    pub fn recover_external_ids_from_wal(&mut self) -> Result<()> {
        use crate::catalog::external_id::ExternalId;

        // We must flush the async WAL first so all frames are on disk.
        self.flush_async_wal()?;

        // Recover WAL entries from disk.  We clone the path so the borrow
        // on `self.wal` can be released before we iterate.
        let wal_path = self.wal.path().to_path_buf();
        let mut replay_wal = wal::Wal::new(&wal_path)?;
        let entries = match replay_wal.recover() {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("external-id WAL recovery: could not read WAL: {e}");
                return Ok(());
            }
        };

        for entry in &entries {
            if let wal::WalEntry::ExternalIdAssigned {
                internal_id,
                external_id_bytes,
            } = entry
            {
                let ext = match ExternalId::from_bytes(external_id_bytes) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(
                            "external-id WAL recovery: bad bytes for node {internal_id}: {e}"
                        );
                        continue;
                    }
                };
                // put_if_absent is idempotent: if the entry is already in the
                // catalog (normal case), it returns `Some(existing_id)` and
                // we do nothing.  If absent (crash before LMDB commit), it
                // inserts the mapping.
                match self.catalog.write_txn() {
                    Ok(mut wtxn) => {
                        let idx = self.catalog.external_id_index();
                        match idx.put_if_absent(&mut wtxn, &ext, *internal_id) {
                            Ok(_) => {
                                if let Err(e) = wtxn.commit() {
                                    tracing::warn!(
                                        "external-id WAL recovery: commit failed for node {internal_id}: {e}"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "external-id WAL recovery: put_if_absent failed for node {internal_id}: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "external-id WAL recovery: could not open catalog txn for node {internal_id}: {e}"
                        );
                    }
                }
            }
        }

        Ok(())
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
        // phase6_spatial-index-autopopulate §1.2 — share the engine's
        // R-tree registry with the executor so spatial CRUD hooks and
        // query operators read and write the same in-memory state.
        self.executor.install_rtree(self.indexes.rtree.clone());
        // phase6_fix-read-match-index-seek §1 — share the property index
        // so the planner can consult it for USING INDEX seeks.
        self.executor
            .install_property_index(self.indexes.property_index.clone());
        Ok(())
    }

    /// Skip [`Self::refresh_executor`]'s ~500us executor
    /// rebuild when a write demonstrably produced no effect (the classic
    /// case: a `MERGE` that matched an existing node with no `ON MATCH
    /// SET`). `refresh_executor` runs inside the engine's `&mut self`
    /// write lock on every write query today, so on the `merge_singleton`
    /// benchmark it dominates the write ceiling even though a no-op MERGE
    /// changed nothing the executor's cloned state needs to see.
    ///
    /// `mutated` must be a locally, accurately computed "did *this* write
    /// change anything" signal — see each call site for how it derives
    /// one (a `deleted_count`/pattern-count diff, an explicit clause-kind
    /// flag, ...). This deliberately takes a plain `bool` rather than
    /// [`executor::types::SideEffects`]: today only
    /// `SideEffects::nodes_created` is ever populated in this crate (it
    /// is stitched in from [`crate::storage::RecordStore::nodes_created`]
    /// at the outermost `execute_cypher_*` entry point) — every other
    /// field (`relationships_created`, `properties_set`, `labels_added`,
    /// ...) is declared but never written anywhere in the engine or
    /// executor. Trusting a `SideEffects` snapshot at an inner call site
    /// would silently treat every SET / REMOVE / relationship-CREATE as a
    /// no-op and skip a refresh it actually needs.
    ///
    /// As a defense-in-depth belt-and-suspenders check, this also
    /// consults `self.storage.nodes_created()` — the per-top-level-query
    /// atomic counter reset in `execute_cypher_with_params` /
    /// `execute_cypher_ast_with_params` — so a node creation that a
    /// caller's local bookkeeping somehow missed still forces a refresh.
    pub(crate) fn refresh_executor_if_mutated(&mut self, mutated: bool) -> Result<()> {
        if mutated || self.storage.nodes_created() != 0 {
            self.refresh_executor()
        } else {
            Ok(())
        }
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

    /// Synchronously flush the record stores to disk.
    ///
    /// The write paths use `flush_async` on the hot path for throughput;
    /// callers that need durable on-disk state (e.g. before a controlled
    /// shutdown or reopen) issue this explicit sync flush.
    pub fn flush(&mut self) -> Result<()> {
        self.storage.flush()
    }

    /// Get async WAL statistics (if available)
    pub fn async_wal_stats(&self) -> Option<wal::AsyncWalStatsSnapshot> {
        self.async_wal_writer.as_ref().map(|w| w.stats())
    }

    /// Resolve a parser-emitted label list (which may contain `:$param`
    /// sentinels encoded as leading `$` strings) against the current
    /// query parameters. Returns `ERR_INVALID_LABEL` if any sentinel
    /// cannot be resolved. Static-only inputs are returned unchanged.
    pub(super) fn resolve_dynamic_labels(&self, labels: &[String]) -> Result<Vec<String>> {
        if !dynamic_labels::contains_dynamic(labels) {
            return Ok(labels.to_vec());
        }
        dynamic_labels::resolve_labels(labels, &self.current_params)
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
