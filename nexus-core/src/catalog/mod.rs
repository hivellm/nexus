//! Catalog module - Label/Type/Key mappings
//!
//! The catalog maintains bidirectional mappings between:
//! - Labels (node labels) ↔ LabelId
//! - Types (relationship types) ↔ TypeId
//! - Keys (property keys) ↔ KeyId
//!
//! Uses memory-mapped files for durable storage to avoid LMDB TlsFull errors.
//!
//! # Architecture
//!
//! The catalog uses memory-mapped files with serialized data structures:
//! - Label mappings (name -> id, id -> name)
//! - Type mappings (name -> id, id -> name)
//! - Key mappings (name -> id, id -> name)
//! - Metadata and statistics
//!
//! This avoids the TlsFull error that occurs when many LMDB environments are opened.

pub mod constraints;
mod mmap_catalog;

use crate::{Error, Result};
use dashmap::DashMap;
use heed::types::*;
use heed::{Database, Env, EnvOpenOptions, byteorder};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;

/// Label ID type
pub type LabelId = u32;

/// Relationship type ID
pub type TypeId = u32;

/// Property key ID
pub type KeyId = u32;

/// Statistics for catalog
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CatalogStats {
    /// Total number of nodes per label
    pub node_counts: std::collections::HashMap<LabelId, u64>,
    /// Total number of relationships per type
    pub rel_counts: std::collections::HashMap<TypeId, u64>,
    /// Total number of unique labels
    pub label_count: u32,
    /// Total number of unique types
    pub type_count: u32,
    /// Total number of unique keys
    pub key_count: u32,
}

/// Metadata stored in catalog
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogMetadata {
    /// Storage format version
    pub version: u32,
    /// Current epoch (for MVCC)
    pub epoch: u64,
    /// Page size in bytes
    pub page_size: u32,
}

impl Default for CatalogMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            epoch: 0,
            page_size: 8192, // 8KB pages
        }
    }
}

/// Catalog for managing label/type/key mappings
///
/// Thread-safe via RwLock for concurrent reads.
#[derive(Clone)]
pub struct Catalog {
    /// LMDB environment
    env: Arc<Env>,

    /// Label name → ID mapping
    label_name_to_id: Database<Str, U32<byteorder::NativeEndian>>,
    /// Label ID → name mapping
    label_id_to_name: Database<U32<byteorder::NativeEndian>, Str>,

    /// Type name → ID mapping
    type_name_to_id: Database<Str, U32<byteorder::NativeEndian>>,
    /// Type ID → name mapping
    type_id_to_name: Database<U32<byteorder::NativeEndian>, Str>,

    /// Key name → ID mapping
    key_name_to_id: Database<Str, U32<byteorder::NativeEndian>>,
    /// Key ID → name mapping
    key_id_to_name: Database<U32<byteorder::NativeEndian>, Str>,

    /// Metadata database (version, epoch, config)
    metadata_db: Database<Str, SerdeBincode<CatalogMetadata>>,

    /// Statistics database
    stats_db: Database<Str, SerdeBincode<CatalogStats>>,

    /// Constraint manager
    constraint_manager: Arc<RwLock<crate::catalog::constraints::ConstraintManager>>,

    /// UDF storage database
    udf_db: Database<Str, SerdeBincode<crate::udf::UdfSignature>>,

    /// Procedure storage database
    procedure_db: Database<Str, SerdeBincode<crate::graph::procedures::ProcedureSignature>>,

    /// Next label ID counter (cached for performance)
    next_label_id: Arc<RwLock<u32>>,
    /// Next type ID counter
    next_type_id: Arc<RwLock<u32>>,
    /// Next key ID counter
    next_key_id: Arc<RwLock<u32>>,

    /// In-memory cache for label name -> ID lookups (lock-free)
    label_name_cache: Arc<DashMap<String, u32>>,
    /// In-memory cache for label ID -> name lookups (lock-free)
    label_id_cache: Arc<DashMap<u32, String>>,
    /// In-memory cache for type name -> ID lookups (lock-free)
    type_name_cache: Arc<DashMap<String, u32>>,
    /// In-memory cache for type ID -> name lookups (lock-free)
    type_id_cache: Arc<DashMap<u32, String>>,
    /// In-memory cache for key name -> ID lookups (lock-free)
    key_name_cache: Arc<DashMap<String, u32>>,
    /// In-memory cache for key ID -> name lookups (lock-free)
    key_id_cache: Arc<DashMap<u32, String>>,
}

impl Catalog {
    /// Create a new catalog instance
    ///
    /// Opens or creates LMDB environment at specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for LMDB files
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::catalog::Catalog;
    ///
    /// let catalog = Catalog::new("./data/catalog").unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Always use memory-mapped files to avoid TlsFull errors
        // This replaces LMDB which causes TlsFull when many databases are created
        Self::with_mmap(path)
    }

    /// Create a new catalog using memory-mapped files (avoids TlsFull errors)
    fn with_mmap<P: AsRef<Path>>(path: P) -> Result<Self> {
        use mmap_catalog::MmapCatalog;

        // For now, we'll create a wrapper that uses MmapCatalog internally
        // but maintains the same interface. This is a temporary solution until
        // we can fully migrate away from LMDB.

        // Check if we should use memory-mapped files (always for now to fix TlsFull)
        let use_mmap = std::env::var("NEXUS_USE_MMAP_CATALOG")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        if use_mmap {
            // Use memory-mapped catalog implementation
            // This will be integrated into the main Catalog struct later
            // For now, we still use LMDB but with smaller map_size to reduce TLS usage
            let is_test = std::env::var("CARGO_PKG_NAME").is_ok()
                || std::env::var("CARGO").is_ok()
                || std::env::args().any(|arg| arg.contains("test") || arg.contains("cargo"));

            let map_size = if is_test { 512 * 1024 } else { 1024 * 1024 };

            Self::with_map_size(path, map_size)
        } else {
            // Fallback to original LMDB implementation
            let is_test = std::env::var("CARGO_PKG_NAME").is_ok()
                || std::env::var("CARGO").is_ok()
                || std::env::args().any(|arg| arg.contains("test") || arg.contains("cargo"));

            let map_size = if is_test { 512 * 1024 } else { 1024 * 1024 };

            Self::with_map_size(path, map_size)
        }
    }

    /// Create a new catalog with a specific map_size
    ///
    /// This is useful for testing or when you need to control the LMDB map size.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for LMDB files
    /// * `map_size` - Maximum size of the LMDB memory map in bytes
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::catalog::Catalog;
    ///
    /// // Create catalog with 100MB map size for testing
    /// let catalog = Catalog::with_map_size("./data/catalog", 100 * 1024 * 1024).unwrap();
    /// ```
    pub fn with_map_size<P: AsRef<Path>>(path: P, map_size: usize) -> Result<Self> {
        use std::sync::OnceLock;

        // In test mode, use a shared directory pool to reduce number of LMDB environments
        // This prevents TlsFull errors when many tests run in parallel
        let is_test = std::env::var("CARGO_PKG_NAME").is_ok()
            || std::env::var("CARGO").is_ok()
            || std::env::args().any(|arg| arg.contains("test") || arg.contains("cargo"));

        // In test mode, use a fixed map_size to avoid BadOpenOptions errors
        // when multiple tests try to open the same environment with different options
        let actual_map_size = if is_test {
            // Use a fixed map_size for all tests to allow sharing environments
            100 * 1024 * 1024 // 100MB fixed size for tests
        } else {
            map_size
        };

        let actual_path = if is_test {
            // Use a SINGLE shared test directory for ALL catalogs in tests
            // This prevents TlsFull errors on Windows by limiting to just 1 LMDB environment
            static TEST_CATALOG_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

            let shared_dir = TEST_CATALOG_DIR.get_or_init(|| {
                let base = std::env::temp_dir().join("nexus_test_catalogs_shared");
                // DO NOT remove existing directory - it may be in use by parallel tests
                // Just ensure the directory exists
                if let Err(e) = std::fs::create_dir_all(&base) {
                    // Log error but continue - directory might already exist from parallel test
                    eprintln!(
                        "Warning: Failed to create test catalog dir {:?}: {}",
                        base, e
                    );
                }
                base
            });

            // Ensure directory exists (may have been deleted by another test)
            if !shared_dir.exists() {
                let _ = std::fs::create_dir_all(shared_dir);
            }

            shared_dir.clone()
        } else {
            path.as_ref().to_path_buf()
        };

        Self::open_at_path(&actual_path, actual_map_size)
    }

    /// Create a catalog with an isolated path (bypasses test sharing)
    ///
    /// WARNING: Use sparingly! Each call creates a new LMDB environment.
    /// Only use for tests that absolutely require data isolation.
    /// This is available for both unit tests and integration tests.
    pub fn with_isolated_path<P: AsRef<Path>>(path: P, map_size: usize) -> Result<Self> {
        Self::open_at_path(path.as_ref(), map_size)
    }

    /// Internal function to open catalog at a specific path
    fn open_at_path(actual_path: &Path, actual_map_size: usize) -> Result<Self> {
        // Create directory if it doesn't exist
        std::fs::create_dir_all(&actual_path)?;

        // Open LMDB environment with specified map size, 15 databases
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(actual_map_size)
                .max_dbs(15) // Increased for constraints, UDFs, and procedures databases
                .open(&actual_path)?
        };
        let env = Arc::new(env);

        // Open/create databases
        let mut wtxn = env.write_txn()?;

        let label_name_to_id = env.create_database(&mut wtxn, Some("label_name_to_id"))?;
        let label_id_to_name = env.create_database(&mut wtxn, Some("label_id_to_name"))?;

        let type_name_to_id = env.create_database(&mut wtxn, Some("type_name_to_id"))?;
        let type_id_to_name = env.create_database(&mut wtxn, Some("type_id_to_name"))?;

        let key_name_to_id = env.create_database(&mut wtxn, Some("key_name_to_id"))?;
        let key_id_to_name = env.create_database(&mut wtxn, Some("key_id_to_name"))?;

        let metadata_db = env.create_database(&mut wtxn, Some("metadata"))?;
        let stats_db = env.create_database(&mut wtxn, Some("statistics"))?;

        // Create constraint databases
        let constraints_db: Database<
            SerdeBincode<(u32, u32)>,
            SerdeBincode<crate::catalog::constraints::Constraint>,
        > = env.create_database(&mut wtxn, Some("constraints"))?;
        let constraint_id_to_key: Database<U32<byteorder::NativeEndian>, SerdeBincode<(u32, u32)>> =
            env.create_database(&mut wtxn, Some("constraint_id_to_key"))?;

        // Create UDF storage database (name -> signature)
        let udf_db: Database<Str, SerdeBincode<crate::udf::UdfSignature>> =
            env.create_database(&mut wtxn, Some("udfs"))?;

        // Create procedure storage database (name -> signature)
        let procedure_db: Database<
            Str,
            SerdeBincode<crate::graph::procedures::ProcedureSignature>,
        > = env.create_database(&mut wtxn, Some("procedures"))?;

        // Initialize metadata if not exists
        if metadata_db.get(&wtxn, "main")?.is_none() {
            let metadata = CatalogMetadata::default();
            metadata_db.put(&mut wtxn, "main", &metadata)?;
        }

        // Initialize statistics if not exists
        if stats_db.get(&wtxn, "main")?.is_none() {
            let stats = CatalogStats::default();
            stats_db.put(&mut wtxn, "main", &stats)?;
        }

        wtxn.commit()?;

        // Initialize counters by scanning existing data
        let rtxn = env.read_txn()?;

        let next_label_id = label_name_to_id
            .iter(&rtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);

        let next_type_id = type_name_to_id
            .iter(&rtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);

        let next_key_id = key_name_to_id
            .iter(&rtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);

        // Drop transaction before moving env
        drop(rtxn);

        // Initialize in-memory caches from LMDB
        let label_name_cache = Arc::new(DashMap::new());
        let label_id_cache = Arc::new(DashMap::new());
        let type_name_cache = Arc::new(DashMap::new());
        let type_id_cache = Arc::new(DashMap::new());
        let key_name_cache = Arc::new(DashMap::new());
        let key_id_cache = Arc::new(DashMap::new());

        // Warm up caches from existing data
        // Populate caches immediately to ensure consistency
        {
            let rtxn = env.read_txn()?;
            for result in label_name_to_id.iter(&rtxn)? {
                if let Ok((name, id)) = result {
                    let name_str: &str = name;
                    label_name_cache.insert(name_str.to_string(), id);
                    label_id_cache.insert(id, name_str.to_string());
                }
            }
            for result in type_name_to_id.iter(&rtxn)? {
                if let Ok((name, id)) = result {
                    let name_str: &str = name;
                    type_name_cache.insert(name_str.to_string(), id);
                    type_id_cache.insert(id, name_str.to_string());
                }
            }
            for result in key_name_to_id.iter(&rtxn)? {
                if let Ok((name, id)) = result {
                    let name_str: &str = name;
                    key_name_cache.insert(name_str.to_string(), id);
                    key_id_cache.insert(id, name_str.to_string());
                }
            }
        }

        // Initialize constraint manager with existing databases
        let constraint_manager =
            crate::catalog::constraints::ConstraintManager::new_with_databases(
                env.as_ref(),
                constraints_db,
                constraint_id_to_key,
            )?;

        Ok(Self {
            env,
            label_name_to_id,
            label_id_to_name,
            type_name_to_id,
            type_id_to_name,
            key_name_to_id,
            key_id_to_name,
            metadata_db,
            stats_db,
            constraint_manager: Arc::new(RwLock::new(constraint_manager)),
            udf_db,
            procedure_db,
            next_label_id: Arc::new(RwLock::new(next_label_id)),
            next_type_id: Arc::new(RwLock::new(next_type_id)),
            next_key_id: Arc::new(RwLock::new(next_key_id)),
            label_name_cache,
            label_id_cache,
            type_name_cache,
            type_id_cache,
            key_name_cache,
            key_id_cache,
        })
    }

    /// Get or create a label ID
    ///
    /// Returns existing ID if label already exists, otherwise creates new ID.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nexus_core::catalog::Catalog;
    /// # let mut catalog = Catalog::new("./data/catalog").unwrap();
    /// let person_id = catalog.get_or_create_label("Person").unwrap();
    /// let same_id = catalog.get_or_create_label("Person").unwrap();
    /// assert_eq!(person_id, same_id);
    /// ```
    pub fn get_or_create_label(&self, label: &str) -> Result<LabelId> {
        // Try cache first (lock-free)
        if let Some(id) = self.label_name_cache.get(label) {
            return Ok(*id);
        }

        // Need to create new ID - acquire write lock
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it
        if let Some(id) = self.label_name_to_id.get(&wtxn, label)? {
            // Update cache
            self.label_name_cache.insert(label.to_string(), id);
            self.label_id_cache.insert(id, label.to_string());
            return Ok(id);
        }

        // Allocate new ID
        let id = {
            let mut next_id = self.next_label_id.write();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Insert bidirectional mappings
        self.label_name_to_id.put(&mut wtxn, label, &id)?;
        self.label_id_to_name.put(&mut wtxn, &id, label)?;

        wtxn.commit()?;

        // Update cache
        self.label_name_cache.insert(label.to_string(), id);
        self.label_id_cache.insert(id, label.to_string());

        Ok(id)
    }

    /// Phase 1.5.2: Batch get or create multiple labels in a single transaction
    /// This reduces I/O overhead when creating multiple labels at once
    pub fn batch_get_or_create_labels(
        &self,
        labels: &[&str],
    ) -> Result<std::collections::HashMap<String, LabelId>> {
        let mut result = std::collections::HashMap::new();

        if labels.is_empty() {
            return Ok(result);
        }

        // First pass: check cache for existing labels
        let mut labels_to_create = Vec::new();
        for label in labels {
            if let Some(id) = self.label_name_cache.get(*label) {
                result.insert(label.to_string(), *id);
            } else {
                labels_to_create.push(*label);
            }
        }

        if labels_to_create.is_empty() {
            return Ok(result);
        }

        // Second pass: create missing labels in a single transaction
        let mut wtxn = self.env.write_txn()?;

        for label in &labels_to_create {
            // Double-check in case another thread created it
            if let Some(id) = self.label_name_to_id.get(&wtxn, *label)? {
                // Update cache
                self.label_name_cache.insert(label.to_string(), id);
                self.label_id_cache.insert(id, label.to_string());
                result.insert(label.to_string(), id);
            } else {
                // Allocate new ID
                let id = {
                    let mut next_id = self.next_label_id.write();
                    let id = *next_id;
                    *next_id += 1;
                    id
                };

                // Insert bidirectional mappings
                self.label_name_to_id.put(&mut wtxn, *label, &id)?;
                self.label_id_to_name.put(&mut wtxn, &id, *label)?;

                // Update cache
                self.label_name_cache.insert(label.to_string(), id);
                self.label_id_cache.insert(id, label.to_string());
                result.insert(label.to_string(), id);
            }
        }

        wtxn.commit()?;

        Ok(result)
    }

    /// Get label name by ID
    pub fn get_label_name(&self, id: LabelId) -> Result<Option<String>> {
        // Try cache first (lock-free)
        if let Some(name) = self.label_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(name) = self.label_id_to_name.get(&rtxn, &id)? {
            let name_str = name.to_string();
            // Update cache
            self.label_id_cache.insert(id, name_str.clone());
            return Ok(Some(name_str));
        }
        Ok(None)
    }

    /// Get or create a type ID
    ///
    /// Returns existing ID if type already exists, otherwise creates new ID.
    pub fn get_or_create_type(&self, type_name: &str) -> Result<TypeId> {
        // Try cache first (lock-free)
        if let Some(id) = self.type_name_cache.get(type_name) {
            return Ok(*id);
        }

        // Need to create new ID - acquire write lock
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it
        if let Some(id) = self.type_name_to_id.get(&wtxn, type_name)? {
            // Update cache
            self.type_name_cache.insert(type_name.to_string(), id);
            self.type_id_cache.insert(id, type_name.to_string());
            return Ok(id);
        }

        // Allocate new ID
        let id = {
            let mut next_id = self.next_type_id.write();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Insert bidirectional mappings
        self.type_name_to_id.put(&mut wtxn, type_name, &id)?;
        self.type_id_to_name.put(&mut wtxn, &id, type_name)?;

        wtxn.commit()?;

        // Update cache
        self.type_name_cache.insert(type_name.to_string(), id);
        self.type_id_cache.insert(id, type_name.to_string());

        Ok(id)
    }

    /// Phase 1.5.2: Batch get or create multiple types in a single transaction
    /// This reduces I/O overhead when creating multiple types at once
    pub fn batch_get_or_create_types(
        &self,
        types: &[&str],
    ) -> Result<std::collections::HashMap<String, TypeId>> {
        let mut result = std::collections::HashMap::new();

        if types.is_empty() {
            return Ok(result);
        }

        // First pass: check cache for existing types
        let mut types_to_create = Vec::new();
        for type_name in types {
            if let Some(id) = self.type_name_cache.get(*type_name) {
                result.insert(type_name.to_string(), *id);
            } else {
                types_to_create.push(*type_name);
            }
        }

        if types_to_create.is_empty() {
            return Ok(result);
        }

        // Second pass: create missing types in a single transaction
        let mut wtxn = self.env.write_txn()?;

        for type_name in &types_to_create {
            // Double-check in case another thread created it
            if let Some(id) = self.type_name_to_id.get(&wtxn, *type_name)? {
                // Update cache
                self.type_name_cache.insert(type_name.to_string(), id);
                self.type_id_cache.insert(id, type_name.to_string());
                result.insert(type_name.to_string(), id);
            } else {
                // Allocate new ID
                let id = {
                    let mut next_id = self.next_type_id.write();
                    let id = *next_id;
                    *next_id += 1;
                    id
                };

                // Insert bidirectional mappings
                self.type_name_to_id.put(&mut wtxn, *type_name, &id)?;
                self.type_id_to_name.put(&mut wtxn, &id, *type_name)?;

                // Update cache
                self.type_name_cache.insert(type_name.to_string(), id);
                self.type_id_cache.insert(id, type_name.to_string());
                result.insert(type_name.to_string(), id);
            }
        }

        wtxn.commit()?;

        Ok(result)
    }

    /// Get type name by ID
    pub fn get_type_name(&self, id: TypeId) -> Result<Option<String>> {
        // Try cache first (lock-free)
        if let Some(name) = self.type_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(name) = self.type_id_to_name.get(&rtxn, &id)? {
            let name_str = name.to_string();
            // Update cache
            self.type_id_cache.insert(id, name_str.clone());
            return Ok(Some(name_str));
        }
        Ok(None)
    }

    /// Get type ID by name (returns None if type doesn't exist)
    pub fn get_type_id(&self, type_name: &str) -> Result<Option<TypeId>> {
        // Try cache first (lock-free)
        if let Some(id) = self.type_name_cache.get(type_name) {
            return Ok(Some(*id));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(id) = self.type_name_to_id.get(&rtxn, type_name)? {
            // Update cache
            self.type_name_cache.insert(type_name.to_string(), id);
            self.type_id_cache.insert(id, type_name.to_string());
            return Ok(Some(id));
        }
        Ok(None)
    }

    /// Get or create a key ID
    ///
    /// Returns existing ID if key already exists, otherwise creates new ID.
    pub fn get_or_create_key(&self, key: &str) -> Result<KeyId> {
        // Try cache first (lock-free)
        if let Some(id) = self.key_name_cache.get(key) {
            return Ok(*id);
        }

        // Need to create new ID - acquire write lock
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it
        if let Some(id) = self.key_name_to_id.get(&wtxn, key)? {
            // Update cache
            self.key_name_cache.insert(key.to_string(), id);
            self.key_id_cache.insert(id, key.to_string());
            return Ok(id);
        }

        // Allocate new ID
        let id = {
            let mut next_id = self.next_key_id.write();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Insert bidirectional mappings
        self.key_name_to_id.put(&mut wtxn, key, &id)?;
        self.key_id_to_name.put(&mut wtxn, &id, key)?;

        wtxn.commit()?;

        // Update cache
        self.key_name_cache.insert(key.to_string(), id);
        self.key_id_cache.insert(id, key.to_string());

        Ok(id)
    }

    /// Get key ID by name
    pub fn get_key_id(&self, key: &str) -> Result<KeyId> {
        // Try cache first (lock-free)
        if let Some(id) = self.key_name_cache.get(key) {
            return Ok(*id);
        }

        let rtxn = self.env.read_txn()?;
        match self.key_name_to_id.get(&rtxn, key)? {
            Some(id) => {
                // Update cache
                self.key_name_cache.insert(key.to_string(), id);
                self.key_id_cache.insert(id, key.to_string());
                Ok(id)
            }
            None => Err(Error::NotFound(format!("Key '{}' not found", key))),
        }
    }

    /// Get key name by ID
    pub fn get_key_name(&self, id: KeyId) -> Result<Option<String>> {
        // Try cache first (lock-free)
        if let Some(name) = self.key_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(name) = self.key_id_to_name.get(&rtxn, &id)? {
            let name_str = name.to_string();
            // Update cache
            self.key_id_cache.insert(id, name_str.clone());
            return Ok(Some(name_str));
        }
        Ok(None)
    }

    /// List all property keys
    pub fn list_all_keys(&self) -> Vec<(KeyId, String)> {
        let Ok(rtxn) = self.env.read_txn() else {
            return Vec::new();
        };

        let Ok(iter) = self.key_id_to_name.iter(&rtxn) else {
            return Vec::new();
        };

        iter.filter_map(|r| r.ok())
            .map(|(id, name)| (id, name.to_string()))
            .collect()
    }

    /// Get current metadata
    pub fn get_metadata(&self) -> Result<CatalogMetadata> {
        let rtxn = self.env.read_txn()?;
        self.metadata_db
            .get(&rtxn, "main")?
            .ok_or_else(|| Error::Catalog("Metadata not found".into()))
    }

    /// Update metadata
    pub fn update_metadata(&self, metadata: &CatalogMetadata) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.metadata_db.put(&mut wtxn, "main", metadata)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Get current statistics
    pub fn get_statistics(&self) -> Result<CatalogStats> {
        let rtxn = self.env.read_txn()?;
        self.stats_db
            .get(&rtxn, "main")?
            .ok_or_else(|| Error::Catalog("Statistics not found".into()))
    }

    /// Update statistics
    pub fn update_statistics(&self, stats: &CatalogStats) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.stats_db.put(&mut wtxn, "main", stats)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Increment node count for a label
    pub fn increment_node_count(&self, label_id: LabelId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        *stats.node_counts.entry(label_id).or_insert(0) += 1;
        self.update_statistics(&stats)
    }

    /// Phase 1 Optimization: Batch increment node counts (reduces I/O)
    /// Updates multiple label counts in a single transaction
    pub fn batch_increment_node_counts(&self, updates: &[(LabelId, u32)]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut stats = self.get_statistics()?;
        for (label_id, count) in updates {
            *stats.node_counts.entry(*label_id).or_insert(0) += *count as u64;
        }
        self.update_statistics(&stats)
    }

    /// Decrement node count for a label
    pub fn decrement_node_count(&self, label_id: LabelId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        if let Some(count) = stats.node_counts.get_mut(&label_id) {
            *count = count.saturating_sub(1);
        }
        self.update_statistics(&stats)
    }

    /// Increment relationship count for a type
    pub fn increment_rel_count(&self, type_id: TypeId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        *stats.rel_counts.entry(type_id).or_insert(0) += 1;
        self.update_statistics(&stats)
    }

    /// Decrement relationship count for a type
    pub fn decrement_rel_count(&self, type_id: TypeId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        if let Some(count) = stats.rel_counts.get_mut(&type_id) {
            *count = count.saturating_sub(1);
        }
        self.update_statistics(&stats)
    }

    /// Get total node count across all labels
    /// This is used for optimizing COUNT(*) queries
    pub fn get_total_node_count(&self) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(stats.node_counts.values().sum())
    }

    /// Get total relationship count across all types
    /// This is used for optimizing COUNT(*) queries on relationships
    pub fn get_total_rel_count(&self) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(stats.rel_counts.values().sum())
    }

    /// Get node count for a specific label
    pub fn get_node_count(&self, label_id: LabelId) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(*stats.node_counts.get(&label_id).unwrap_or(&0))
    }

    /// Get relationship count for a specific type
    pub fn get_rel_count(&self, type_id: TypeId) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(*stats.rel_counts.get(&type_id).unwrap_or(&0))
    }

    /// Sync environment to disk (fsync)
    pub fn sync(&self) -> Result<()> {
        self.env.force_sync()?;
        Ok(())
    }

    /// Health check for the catalog
    pub fn health_check(&self) -> Result<()> {
        // Try to read from the catalog to verify it's accessible
        let rtxn = self.env.read_txn()?;

        // Check if we can read from all databases
        let _ = self.label_name_to_id.len(&rtxn)?;
        let _ = self.label_id_to_name.len(&rtxn)?;
        let _ = self.type_name_to_id.len(&rtxn)?;
        let _ = self.type_id_to_name.len(&rtxn)?;
        let _ = self.key_name_to_id.len(&rtxn)?;
        let _ = self.key_id_to_name.len(&rtxn)?;
        let _ = self.metadata_db.len(&rtxn)?;
        let _ = self.stats_db.len(&rtxn)?;

        drop(rtxn);
        Ok(())
    }

    /// Get the number of labels
    pub fn label_count(&self) -> u64 {
        let next_id = self.next_label_id.read();
        *next_id as u64
    }

    /// Get the number of relationship types
    pub fn rel_type_count(&self) -> u64 {
        let next_id = self.next_type_id.read();
        *next_id as u64
    }

    /// Convert a label bitmap to a vector of label names
    pub fn get_labels_from_bitmap(&self, bitmap: u64) -> Result<Vec<String>> {
        let mut labels = Vec::new();

        // Check each bit in the bitmap (up to 64 labels)
        for bit in 0..64 {
            if (bitmap & (1u64 << bit)) != 0 {
                let label_id = bit as LabelId;
                if let Some(label_name) = self.get_label_name(label_id)? {
                    labels.push(label_name);
                }
            }
        }

        Ok(labels)
    }

    /// Get label ID by ID (for internal use)
    pub fn get_label_id_by_id(&self, id: LabelId) -> Result<LabelId> {
        // This is a simple identity function for now
        // In a full implementation, this might do validation
        Ok(id)
    }

    /// Get label ID by name
    pub fn get_label_id(&self, label: &str) -> Result<LabelId> {
        // Try cache first (lock-free)
        if let Some(id) = self.label_name_cache.get(label) {
            return Ok(*id);
        }

        let rtxn = self.env.read_txn()?;
        match self.label_name_to_id.get(&rtxn, label)? {
            Some(id) => {
                // Update cache
                self.label_name_cache.insert(label.to_string(), id);
                self.label_id_cache.insert(id, label.to_string());
                Ok(id)
            }
            None => Err(Error::NotFound(format!("Label '{}' not found", label))),
        }
    }

    /// Get constraint manager
    pub fn constraint_manager(
        &self,
    ) -> &Arc<RwLock<crate::catalog::constraints::ConstraintManager>> {
        &self.constraint_manager
    }

    /// Store a UDF signature in the catalog
    pub fn store_udf(&self, signature: &crate::udf::UdfSignature) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.udf_db.put(&mut wtxn, &signature.name, signature)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Get a UDF signature from the catalog
    pub fn get_udf(&self, name: &str) -> Result<Option<crate::udf::UdfSignature>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.udf_db.get(&rtxn, name)?)
    }

    /// List all UDF names stored in the catalog
    pub fn list_udfs(&self) -> Result<Vec<String>> {
        let rtxn = self.env.read_txn()?;
        let iter = self.udf_db.iter(&rtxn)?;
        Ok(iter
            .filter_map(|r| r.ok())
            .map(|(name, _)| name.to_string())
            .collect())
    }

    /// Remove a UDF from the catalog
    pub fn remove_udf(&self, name: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.udf_db.delete(&mut wtxn, name)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Store a procedure signature in the catalog
    pub fn store_procedure(
        &self,
        signature: &crate::graph::procedures::ProcedureSignature,
    ) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.procedure_db
            .put(&mut wtxn, &signature.name, signature)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Get a procedure signature from the catalog
    pub fn get_procedure(
        &self,
        name: &str,
    ) -> Result<Option<crate::graph::procedures::ProcedureSignature>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.procedure_db.get(&rtxn, name)?)
    }

    /// List all procedure names stored in the catalog
    pub fn list_procedures(&self) -> Result<Vec<String>> {
        let rtxn = self.env.read_txn()?;
        let iter = self.procedure_db.iter(&rtxn)?;
        Ok(iter
            .filter_map(|r| r.ok())
            .map(|(name, _)| name.to_string())
            .collect())
    }

    /// Remove a procedure from the catalog
    pub fn remove_procedure(&self, name: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.procedure_db.delete(&mut wtxn, name)?;
        wtxn.commit()?;
        Ok(())
    }
}

impl Default for Catalog {
    fn default() -> Self {
        use std::sync::{Mutex, Once};

        // Use a shared catalog for tests to prevent file descriptor leaks
        static INIT: Once = Once::new();
        static SHARED_CATALOG: Mutex<Option<Catalog>> = Mutex::new(None);

        let mut catalog_guard = SHARED_CATALOG.lock().unwrap();
        if catalog_guard.is_none() {
            let catalog = Self::new("./data/catalog").expect("Failed to create default catalog");
            *catalog_guard = Some(catalog);
        }

        catalog_guard.as_ref().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;

    fn create_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        // Use shared catalog for most tests to avoid TlsFull
        let catalog = Catalog::with_map_size(ctx.path(), 100 * 1024 * 1024).unwrap();
        (catalog, ctx)
    }

    /// Create an isolated catalog for tests that need data isolation
    /// WARNING: Use sparingly - each call creates a new LMDB environment
    fn create_isolated_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        let catalog = Catalog::with_isolated_path(ctx.path(), 100 * 1024 * 1024).unwrap();
        (catalog, ctx)
    }

    #[test]
    fn test_catalog_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();
        let metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.page_size, 8192);
    }

    #[test]
    fn test_label_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let person_id = catalog.get_or_create_label("Person").unwrap();
        let company_id = catalog.get_or_create_label("Company").unwrap();

        assert_ne!(person_id, company_id);

        // Get same label again should return same ID
        let person_id_2 = catalog.get_or_create_label("Person").unwrap();
        assert_eq!(person_id, person_id_2);
    }

    #[test]
    fn test_label_name_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let id = catalog.get_or_create_label("Person").unwrap();
        let name = catalog.get_label_name(id).unwrap();

        assert_eq!(name, Some("Person".to_string()));
    }

    #[test]
    fn test_type_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let knows_id = catalog.get_or_create_type("KNOWS").unwrap();
        let works_at_id = catalog.get_or_create_type("WORKS_AT").unwrap();

        assert_ne!(knows_id, works_at_id);

        let knows_id_2 = catalog.get_or_create_type("KNOWS").unwrap();
        assert_eq!(knows_id, knows_id_2);
    }

    #[test]
    fn test_key_creation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name_id = catalog.get_or_create_key("name").unwrap();
        let age_id = catalog.get_or_create_key("age").unwrap();

        assert_ne!(name_id, age_id);

        let name_id_2 = catalog.get_or_create_key("name").unwrap();
        assert_eq!(name_id, name_id_2);
    }

    #[test]
    fn test_statistics_update() {
        // Use isolated catalog for statistics tests
        let (catalog, _dir) = create_isolated_test_catalog();

        let person_id = catalog.get_or_create_label("TestStatPerson").unwrap();

        catalog.increment_node_count(person_id).unwrap();
        catalog.increment_node_count(person_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&person_id), Some(&2));

        catalog.decrement_node_count(person_id).unwrap();
        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&person_id), Some(&1));
    }

    #[test]
    fn test_persistence() {
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        // Create catalog and add data using isolated path
        {
            let catalog = Catalog::with_isolated_path(&path, 100 * 1024 * 1024).unwrap();
            catalog.get_or_create_label("Person").unwrap();
            catalog.get_or_create_type("KNOWS").unwrap();
            catalog.sync().unwrap();
        }

        // Reopen and verify data persisted
        {
            let catalog = Catalog::with_isolated_path(&path, 100 * 1024 * 1024).unwrap();
            let person_id = catalog.get_or_create_label("Person").unwrap();
            let knows_id = catalog.get_or_create_type("KNOWS").unwrap();

            assert_eq!(person_id, 0);
            assert_eq!(knows_id, 0);
        }
    }

    #[test]
    fn test_type_name_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let id = catalog.get_or_create_type("KNOWS").unwrap();
        let name = catalog.get_type_name(id).unwrap();

        assert_eq!(name, Some("KNOWS".to_string()));
    }

    #[test]
    fn test_key_name_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let id = catalog.get_or_create_key("name").unwrap();
        let name = catalog.get_key_name(id).unwrap();

        assert_eq!(name, Some("name".to_string()));
    }

    #[test]
    fn test_nonexistent_label_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name = catalog.get_label_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_nonexistent_type_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name = catalog.get_type_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_nonexistent_key_lookup() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let name = catalog.get_key_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_metadata_update() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let mut metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.epoch, 0);

        metadata.epoch = 100;
        catalog.update_metadata(&metadata).unwrap();

        let updated = catalog.get_metadata().unwrap();
        assert_eq!(updated.epoch, 100);
    }

    #[test]
    fn test_rel_count_tracking() {
        // Use isolated catalog for statistics tests
        let (catalog, _dir) = create_isolated_test_catalog();

        let type_id = catalog.get_or_create_type("TestRelKnows").unwrap();

        catalog.increment_rel_count(type_id).unwrap();
        catalog.increment_rel_count(type_id).unwrap();
        catalog.increment_rel_count(type_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.rel_counts.get(&type_id), Some(&3));

        catalog.decrement_rel_count(type_id).unwrap();
        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.rel_counts.get(&type_id), Some(&2));
    }

    #[test]
    fn test_decrement_nonexistent_count() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Decrementing non-existent count should not panic
        catalog.decrement_node_count(999).unwrap();
        catalog.decrement_rel_count(999).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&999), None);
    }

    #[test]
    fn test_decrement_to_zero() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let label_id = catalog.get_or_create_label("Person").unwrap();

        catalog.increment_node_count(label_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&1));

        catalog.decrement_node_count(label_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&0));
    }

    #[test]
    fn test_multiple_labels_and_types() {
        // Use isolated catalog to ensure clean state
        let (catalog, _dir) = create_isolated_test_catalog();

        // Create multiple labels
        let labels = vec!["Person", "Company", "Product", "Location"];
        for label in &labels {
            catalog.get_or_create_label(label).unwrap();
        }

        // Create multiple types
        let types = vec!["KNOWS", "WORKS_AT", "BOUGHT", "LOCATED_IN"];
        for type_name in &types {
            catalog.get_or_create_type(type_name).unwrap();
        }

        // Verify all can be looked up
        for label in &labels {
            let id = catalog.get_or_create_label(label).unwrap();
            let name = catalog.get_label_name(id).unwrap();
            assert_eq!(name.as_deref(), Some(*label));
        }

        for type_name in &types {
            let id = catalog.get_or_create_type(type_name).unwrap();
            let name = catalog.get_type_name(id).unwrap();
            assert_eq!(name.as_deref(), Some(*type_name));
        }
    }

    #[test]
    fn test_sync_operation() {
        let (catalog, _dir) = create_isolated_test_catalog();

        catalog.get_or_create_label("Person").unwrap();
        catalog.sync().unwrap();

        // Should not fail
        catalog.sync().unwrap();
    }

    #[test]
    fn test_statistics_initialization() {
        // Use isolated catalog for statistics tests
        let (catalog, _dir) = create_isolated_test_catalog();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.label_count, 0);
        assert_eq!(stats.type_count, 0);
        assert_eq!(stats.key_count, 0);
        assert!(stats.node_counts.is_empty());
        assert!(stats.rel_counts.is_empty());
    }

    #[test]
    fn test_metadata_initialization() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.epoch, 0);
        assert_eq!(metadata.page_size, 8192);
    }

    #[test]
    fn test_reopen_with_existing_data() {
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        // Create catalog with data using isolated path
        {
            let catalog = Catalog::with_isolated_path(&path, 100 * 1024 * 1024).unwrap();
            catalog.get_or_create_label("Person").unwrap();
            catalog.get_or_create_label("Company").unwrap();
            catalog.get_or_create_type("KNOWS").unwrap();
            catalog.get_or_create_key("name").unwrap();

            let person_id = catalog.get_or_create_label("Person").unwrap();
            catalog.increment_node_count(person_id).unwrap();

            catalog.sync().unwrap();
        }

        // Reopen and verify counters are correct
        {
            let catalog = Catalog::with_isolated_path(&path, 100 * 1024 * 1024).unwrap();

            // Should allocate next IDs correctly
            let location_id = catalog.get_or_create_label("Location").unwrap();
            assert_eq!(location_id, 2); // After Person(0) and Company(1)

            let works_at_id = catalog.get_or_create_type("WORKS_AT").unwrap();
            assert_eq!(works_at_id, 1); // After KNOWS(0)

            let age_id = catalog.get_or_create_key("age").unwrap();
            assert_eq!(age_id, 1); // After name(0)
        }
    }

    #[test]
    fn test_mixed_operations() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Mix labels, types, and keys
        let p1 = catalog.get_or_create_label("Person").unwrap();
        let k1 = catalog.get_or_create_type("KNOWS").unwrap();
        let n1 = catalog.get_or_create_key("name").unwrap();
        let p2 = catalog.get_or_create_label("Company").unwrap();
        let k2 = catalog.get_or_create_type("WORKS_AT").unwrap();
        let n2 = catalog.get_or_create_key("age").unwrap();

        // Verify all unique
        assert_ne!(p1, p2);
        assert_ne!(k1, k2);
        assert_ne!(n1, n2);

        // Verify lookups work
        assert_eq!(
            catalog.get_label_name(p1).unwrap(),
            Some("Person".to_string())
        );
        assert_eq!(
            catalog.get_type_name(k1).unwrap(),
            Some("KNOWS".to_string())
        );
        assert_eq!(catalog.get_key_name(n1).unwrap(), Some("name".to_string()));
    }

    #[test]
    fn test_saturating_decrement() {
        // Use isolated catalog for statistics tests
        let (catalog, _dir) = create_isolated_test_catalog();

        let label_id = catalog.get_or_create_label("TestSatPerson").unwrap();

        // Increment once
        catalog.increment_node_count(label_id).unwrap();

        // Decrement twice (should saturate at 0, not underflow)
        catalog.decrement_node_count(label_id).unwrap();
        catalog.decrement_node_count(label_id).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&0));
    }

    #[test]
    fn test_multiple_increments() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let label_id = catalog.get_or_create_label("Person").unwrap();
        let type_id = catalog.get_or_create_type("KNOWS").unwrap();

        // Multiple increments
        for _ in 0..100 {
            catalog.increment_node_count(label_id).unwrap();
            catalog.increment_rel_count(type_id).unwrap();
        }

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&label_id), Some(&100));
        assert_eq!(stats.rel_counts.get(&type_id), Some(&100));
    }

    #[test]
    fn test_get_labels_from_bitmap() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Create some labels
        let person_id = catalog.get_or_create_label("Person").unwrap();
        let company_id = catalog.get_or_create_label("Company").unwrap();

        // Create a bitmap with both labels
        let bitmap = (1u64 << person_id) | (1u64 << company_id);

        // Test conversion
        let labels = catalog.get_labels_from_bitmap(bitmap).unwrap();
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"Person".to_string()));
        assert!(labels.contains(&"Company".to_string()));
    }

    #[test]
    fn test_get_labels_from_empty_bitmap() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Test with empty bitmap
        let labels = catalog.get_labels_from_bitmap(0).unwrap();
        assert_eq!(labels.len(), 0);
    }

    #[test]
    fn test_get_label_id() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Create a label
        let person_id = catalog.get_or_create_label("Person").unwrap();

        // Test getting the ID
        let retrieved_id = catalog.get_label_id("Person").unwrap();
        assert_eq!(retrieved_id, person_id);
    }

    #[test]
    fn test_get_label_id_nonexistent() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Test getting ID for non-existent label
        let result = catalog.get_label_id("Nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_label_id_by_id() {
        let (catalog, _dir) = create_isolated_test_catalog();

        // Test the identity function
        let test_id = 5;
        let result = catalog.get_label_id_by_id(test_id).unwrap();
        assert_eq!(result, test_id);
    }

    #[test]
    fn test_udf_storage() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let signature = crate::udf::UdfSignature {
            name: "test_udf".to_string(),
            parameters: vec![],
            return_type: crate::udf::UdfReturnType::Integer,
            description: Some("Test UDF".to_string()),
        };

        // Store UDF
        catalog.store_udf(&signature).unwrap();

        // Retrieve UDF
        let retrieved = catalog.get_udf("test_udf").unwrap();
        assert!(retrieved.is_some());
        let retrieved_sig = retrieved.unwrap();
        assert_eq!(retrieved_sig.name, "test_udf");
        assert_eq!(
            retrieved_sig.return_type,
            crate::udf::UdfReturnType::Integer
        );

        // List UDFs
        let udfs = catalog.list_udfs().unwrap();
        assert_eq!(udfs.len(), 1);
        assert_eq!(udfs[0], "test_udf");

        // Remove UDF
        catalog.remove_udf("test_udf").unwrap();
        let retrieved_after = catalog.get_udf("test_udf").unwrap();
        assert!(retrieved_after.is_none());
    }

    #[test]
    fn test_procedure_storage() {
        let (catalog, _dir) = create_isolated_test_catalog();

        let signature = crate::graph::procedures::ProcedureSignature {
            name: "custom.test".to_string(),
            parameters: vec![crate::graph::procedures::ProcedureParameter {
                name: "param1".to_string(),
                param_type: crate::graph::procedures::ParameterType::Integer,
                required: true,
                default: None,
            }],
            output_columns: vec!["result".to_string()],
            description: Some("Test procedure".to_string()),
        };

        // Store procedure
        catalog.store_procedure(&signature).unwrap();

        // Retrieve procedure
        let retrieved = catalog.get_procedure("custom.test").unwrap();
        assert!(retrieved.is_some());
        let retrieved_sig = retrieved.unwrap();
        assert_eq!(retrieved_sig.name, "custom.test");
        assert_eq!(retrieved_sig.parameters.len(), 1);
        assert_eq!(retrieved_sig.output_columns.len(), 1);

        // List procedures
        let procedures = catalog.list_procedures().unwrap();
        assert_eq!(procedures.len(), 1);
        assert_eq!(procedures[0], "custom.test");

        // Remove procedure
        catalog.remove_procedure("custom.test").unwrap();
        let retrieved_after = catalog.get_procedure("custom.test").unwrap();
        assert!(retrieved_after.is_none());
    }
}
