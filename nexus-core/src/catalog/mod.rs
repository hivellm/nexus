//! Catalog module - Label/Type/Key mappings
//!
//! The catalog maintains bidirectional mappings between:
//! - Labels (node labels) ↔ LabelId
//! - Types (relationship types) ↔ TypeId
//! - Keys (property keys) ↔ KeyId
//!
//! Uses LMDB (via heed) for durable storage of these mappings.
//!
//! # Architecture
//!
//! The catalog uses 6 LMDB databases for bidirectional mappings:
//! - `label_name_to_id`: String → u32
//! - `label_id_to_name`: u32 → String
//! - `type_name_to_id`: String → u32
//! - `type_id_to_name`: u32 → String
//! - `key_name_to_id`: String → u32
//! - `key_id_to_name`: u32 → String
//!
//! Plus databases for:
//! - `metadata`: Version, epoch, config
//! - `statistics`: Node counts, relationship counts

use crate::{Error, Result};
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

    /// Next label ID counter (cached for performance)
    next_label_id: Arc<RwLock<u32>>,
    /// Next type ID counter
    next_type_id: Arc<RwLock<u32>>,
    /// Next key ID counter
    next_key_id: Arc<RwLock<u32>>,
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
        // Create directory if it doesn't exist
        std::fs::create_dir_all(&path)?;

        // Open LMDB environment (10GB max size, 8 databases)
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(10 * 1024 * 1024 * 1024) // 10GB
                .max_dbs(8)
                .open(path.as_ref())?
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

        drop(rtxn);

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
            next_label_id: Arc::new(RwLock::new(next_label_id)),
            next_type_id: Arc::new(RwLock::new(next_type_id)),
            next_key_id: Arc::new(RwLock::new(next_key_id)),
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
        // Try to read existing ID first
        let rtxn = self.env.read_txn()?;
        if let Some(id) = self.label_name_to_id.get(&rtxn, label)? {
            return Ok(id);
        }
        drop(rtxn);

        // Need to create new ID - acquire write lock
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it
        if let Some(id) = self.label_name_to_id.get(&wtxn, label)? {
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

        Ok(id)
    }

    /// Get label name by ID
    pub fn get_label_name(&self, id: LabelId) -> Result<Option<String>> {
        let rtxn = self.env.read_txn()?;
        Ok(self
            .label_id_to_name
            .get(&rtxn, &id)?
            .map(|s| s.to_string()))
    }

    /// Get or create a type ID
    ///
    /// Returns existing ID if type already exists, otherwise creates new ID.
    pub fn get_or_create_type(&self, type_name: &str) -> Result<TypeId> {
        // Try to read existing ID first
        let rtxn = self.env.read_txn()?;
        if let Some(id) = self.type_name_to_id.get(&rtxn, type_name)? {
            return Ok(id);
        }
        drop(rtxn);

        // Need to create new ID - acquire write lock
        let mut wtxn = self.env.write_txn()?;

        // Double-check
        if let Some(id) = self.type_name_to_id.get(&wtxn, type_name)? {
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

        Ok(id)
    }

    /// Get type name by ID
    pub fn get_type_name(&self, id: TypeId) -> Result<Option<String>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.type_id_to_name.get(&rtxn, &id)?.map(|s| s.to_string()))
    }

    /// Get or create a key ID
    ///
    /// Returns existing ID if key already exists, otherwise creates new ID.
    pub fn get_or_create_key(&self, key: &str) -> Result<KeyId> {
        // Try to read existing ID first
        let rtxn = self.env.read_txn()?;
        if let Some(id) = self.key_name_to_id.get(&rtxn, key)? {
            return Ok(id);
        }
        drop(rtxn);

        // Need to create new ID - acquire write lock
        let mut wtxn = self.env.write_txn()?;

        // Double-check
        if let Some(id) = self.key_name_to_id.get(&wtxn, key)? {
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

        Ok(id)
    }

    /// Get key name by ID
    pub fn get_key_name(&self, id: KeyId) -> Result<Option<String>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.key_id_to_name.get(&rtxn, &id)?.map(|s| s.to_string()))
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
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new("./data/catalog").expect("Failed to create default catalog")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_catalog() -> (Catalog, TempDir) {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::new(dir.path()).unwrap();
        (catalog, dir)
    }

    #[test]
    fn test_catalog_creation() {
        let (catalog, _dir) = create_test_catalog();
        let metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.page_size, 8192);
    }

    #[test]
    fn test_label_creation() {
        let (catalog, _dir) = create_test_catalog();

        let person_id = catalog.get_or_create_label("Person").unwrap();
        let company_id = catalog.get_or_create_label("Company").unwrap();

        assert_ne!(person_id, company_id);

        // Get same label again should return same ID
        let person_id_2 = catalog.get_or_create_label("Person").unwrap();
        assert_eq!(person_id, person_id_2);
    }

    #[test]
    fn test_label_name_lookup() {
        let (catalog, _dir) = create_test_catalog();

        let id = catalog.get_or_create_label("Person").unwrap();
        let name = catalog.get_label_name(id).unwrap();

        assert_eq!(name, Some("Person".to_string()));
    }

    #[test]
    fn test_type_creation() {
        let (catalog, _dir) = create_test_catalog();

        let knows_id = catalog.get_or_create_type("KNOWS").unwrap();
        let works_at_id = catalog.get_or_create_type("WORKS_AT").unwrap();

        assert_ne!(knows_id, works_at_id);

        let knows_id_2 = catalog.get_or_create_type("KNOWS").unwrap();
        assert_eq!(knows_id, knows_id_2);
    }

    #[test]
    fn test_key_creation() {
        let (catalog, _dir) = create_test_catalog();

        let name_id = catalog.get_or_create_key("name").unwrap();
        let age_id = catalog.get_or_create_key("age").unwrap();

        assert_ne!(name_id, age_id);

        let name_id_2 = catalog.get_or_create_key("name").unwrap();
        assert_eq!(name_id, name_id_2);
    }

    #[test]
    fn test_statistics_update() {
        let (catalog, _dir) = create_test_catalog();

        let person_id = catalog.get_or_create_label("Person").unwrap();

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
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Create catalog and add data
        {
            let catalog = Catalog::new(&path).unwrap();
            catalog.get_or_create_label("Person").unwrap();
            catalog.get_or_create_type("KNOWS").unwrap();
            catalog.sync().unwrap();
        }

        // Reopen and verify data persisted
        {
            let catalog = Catalog::new(&path).unwrap();
            let person_id = catalog.get_or_create_label("Person").unwrap();
            let knows_id = catalog.get_or_create_type("KNOWS").unwrap();

            assert_eq!(person_id, 0);
            assert_eq!(knows_id, 0);
        }
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let dir = TempDir::new().unwrap();
        let catalog = Arc::new(Catalog::new(dir.path()).unwrap());

        let mut handles = vec![];

        // Spawn 10 threads concurrently creating labels
        for i in 0..10 {
            let cat = catalog.clone();
            let handle = thread::spawn(move || {
                let label = format!("Label{}", i);
                cat.get_or_create_label(&label).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all labels created
        for i in 0..10 {
            let label = format!("Label{}", i);
            let id = catalog.get_or_create_label(&label).unwrap();
            assert!(id < 10);
        }
    }

    #[test]
    fn test_type_name_lookup() {
        let (catalog, _dir) = create_test_catalog();

        let id = catalog.get_or_create_type("KNOWS").unwrap();
        let name = catalog.get_type_name(id).unwrap();

        assert_eq!(name, Some("KNOWS".to_string()));
    }

    #[test]
    fn test_key_name_lookup() {
        let (catalog, _dir) = create_test_catalog();

        let id = catalog.get_or_create_key("name").unwrap();
        let name = catalog.get_key_name(id).unwrap();

        assert_eq!(name, Some("name".to_string()));
    }

    #[test]
    fn test_nonexistent_label_lookup() {
        let (catalog, _dir) = create_test_catalog();

        let name = catalog.get_label_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_nonexistent_type_lookup() {
        let (catalog, _dir) = create_test_catalog();

        let name = catalog.get_type_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_nonexistent_key_lookup() {
        let (catalog, _dir) = create_test_catalog();

        let name = catalog.get_key_name(999).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_metadata_update() {
        let (catalog, _dir) = create_test_catalog();

        let mut metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.epoch, 0);

        metadata.epoch = 100;
        catalog.update_metadata(&metadata).unwrap();

        let updated = catalog.get_metadata().unwrap();
        assert_eq!(updated.epoch, 100);
    }

    #[test]
    fn test_rel_count_tracking() {
        let (catalog, _dir) = create_test_catalog();

        let type_id = catalog.get_or_create_type("KNOWS").unwrap();

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
        let (catalog, _dir) = create_test_catalog();

        // Decrementing non-existent count should not panic
        catalog.decrement_node_count(999).unwrap();
        catalog.decrement_rel_count(999).unwrap();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.node_counts.get(&999), None);
    }

    #[test]
    fn test_decrement_to_zero() {
        let (catalog, _dir) = create_test_catalog();

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
        let (catalog, _dir) = create_test_catalog();

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
    fn test_concurrent_types_and_keys() {
        use std::sync::Arc;
        use std::thread;

        let dir = TempDir::new().unwrap();
        let catalog = Arc::new(Catalog::new(dir.path()).unwrap());

        let mut handles = vec![];

        // Concurrently create types
        for i in 0..5 {
            let cat = catalog.clone();
            let handle = thread::spawn(move || {
                let type_name = format!("TYPE_{}", i);
                cat.get_or_create_type(&type_name).unwrap();
            });
            handles.push(handle);
        }

        // Concurrently create keys
        for i in 0..5 {
            let cat = catalog.clone();
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                cat.get_or_create_key(&key).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all created
        for i in 0..5 {
            let type_name = format!("TYPE_{}", i);
            catalog.get_or_create_type(&type_name).unwrap();

            let key = format!("key_{}", i);
            catalog.get_or_create_key(&key).unwrap();
        }
    }

    #[test]
    fn test_sync_operation() {
        let (catalog, _dir) = create_test_catalog();

        catalog.get_or_create_label("Person").unwrap();
        catalog.sync().unwrap();

        // Should not fail
        catalog.sync().unwrap();
    }

    #[test]
    fn test_statistics_initialization() {
        let (catalog, _dir) = create_test_catalog();

        let stats = catalog.get_statistics().unwrap();
        assert_eq!(stats.label_count, 0);
        assert_eq!(stats.type_count, 0);
        assert_eq!(stats.key_count, 0);
        assert!(stats.node_counts.is_empty());
        assert!(stats.rel_counts.is_empty());
    }

    #[test]
    fn test_metadata_initialization() {
        let (catalog, _dir) = create_test_catalog();

        let metadata = catalog.get_metadata().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.epoch, 0);
        assert_eq!(metadata.page_size, 8192);
    }

    #[test]
    fn test_reopen_with_existing_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Create catalog with data
        {
            let catalog = Catalog::new(&path).unwrap();
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
            let catalog = Catalog::new(&path).unwrap();

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
    fn test_concurrent_label_same_name() {
        use std::sync::Arc;
        use std::thread;

        let dir = TempDir::new().unwrap();
        let catalog = Arc::new(Catalog::new(dir.path()).unwrap());

        let mut handles = vec![];

        // Multiple threads trying to create same label
        for _ in 0..5 {
            let cat = catalog.clone();
            let handle = thread::spawn(move || cat.get_or_create_label("Person").unwrap());
            handles.push(handle);
        }

        let ids: Vec<u32> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All should get same ID
        assert!(ids.iter().all(|&id| id == ids[0]));
    }

    #[test]
    fn test_mixed_operations() {
        let (catalog, _dir) = create_test_catalog();

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
        let (catalog, _dir) = create_test_catalog();

        let label_id = catalog.get_or_create_label("Person").unwrap();

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
        let (catalog, _dir) = create_test_catalog();

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
}
