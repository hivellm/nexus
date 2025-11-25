//! Memory-mapped Catalog implementation
//!
//! This module provides a Catalog implementation that uses memory-mapped files
//! directly instead of LMDB, avoiding the TlsFull error when many databases are created.
//!
//! The catalog data is stored in a single memory-mapped file with the following structure:
//! - Header (fixed size)
//! - Label mappings (name -> id, id -> name)
//! - Type mappings (name -> id, id -> name)
//! - Key mappings (name -> id, id -> name)
//! - Metadata and statistics

use crate::{Error, Result};
use dashmap::DashMap;
use memmap2::{MmapMut, MmapOptions};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Arc;

use super::{CatalogMetadata, CatalogStats, KeyId, LabelId, TypeId};

/// Catalog data structure stored in memory-mapped file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogData {
    /// Label name -> ID mapping
    label_name_to_id: HashMap<String, LabelId>,
    /// Label ID -> name mapping
    label_id_to_name: HashMap<LabelId, String>,
    /// Type name -> ID mapping
    type_name_to_id: HashMap<String, TypeId>,
    /// Type ID -> name mapping
    type_id_to_name: HashMap<TypeId, String>,
    /// Key name -> ID mapping
    key_name_to_id: HashMap<String, KeyId>,
    /// Key ID -> name mapping
    key_id_to_name: HashMap<KeyId, String>,
    /// Metadata
    metadata: CatalogMetadata,
    /// Statistics
    stats: CatalogStats,
    /// Next label ID
    next_label_id: LabelId,
    /// Next type ID
    next_type_id: TypeId,
    /// Next key ID
    next_key_id: KeyId,
}

impl Default for CatalogData {
    fn default() -> Self {
        Self {
            label_name_to_id: HashMap::new(),
            label_id_to_name: HashMap::new(),
            type_name_to_id: HashMap::new(),
            type_id_to_name: HashMap::new(),
            key_name_to_id: HashMap::new(),
            key_id_to_name: HashMap::new(),
            metadata: CatalogMetadata::default(),
            stats: CatalogStats::default(),
            next_label_id: 0,
            next_type_id: 0,
            next_key_id: 0,
        }
    }
}

/// Memory-mapped Catalog implementation
pub struct MmapCatalog {
    /// Path to catalog file
    file_path: std::path::PathBuf,
    /// Memory-mapped file
    mmap: Arc<RwLock<MmapMut>>,
    /// File handle
    file: Arc<File>,
    /// In-memory cache for fast lookups (lock-free)
    label_name_cache: Arc<DashMap<String, LabelId>>,
    label_id_cache: Arc<DashMap<LabelId, String>>,
    type_name_cache: Arc<DashMap<String, TypeId>>,
    type_id_cache: Arc<DashMap<TypeId, String>>,
    key_name_cache: Arc<DashMap<String, KeyId>>,
    key_id_cache: Arc<DashMap<KeyId, String>>,
    /// Next IDs (cached for performance)
    next_label_id: Arc<RwLock<LabelId>>,
    next_type_id: Arc<RwLock<TypeId>>,
    next_key_id: Arc<RwLock<KeyId>>,
}

impl MmapCatalog {
    /// Initial file size (1MB should be enough for most catalogs)
    const INITIAL_FILE_SIZE: usize = 1024 * 1024;

    /// Create a new memory-mapped catalog
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file_path = if path.is_dir() {
            path.join("catalog.dat")
        } else {
            path.to_path_buf()
        };

        // Create directory if needed
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open or create file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&file_path)?;

        // Initialize file size if empty
        let metadata = file.metadata()?;
        if metadata.len() == 0 {
            file.set_len(Self::INITIAL_FILE_SIZE as u64)?;
        }

        // Create memory mapping
        let mut mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        // Initialize file if empty
        if metadata.len() == 0 {
            let default_data = CatalogData::default();
            let serialized = bincode::serialize(&default_data)
                .map_err(|e| Error::Storage(format!("Failed to serialize catalog: {}", e)))?;

            if serialized.len() <= mmap.len() {
                mmap[..serialized.len()].copy_from_slice(&serialized);
                mmap.flush()?;
            }
        }

        let file = Arc::new(file);
        let mmap = Arc::new(RwLock::new(mmap));

        // Load data from file
        let data = Self::load_data(&mmap)?;

        // Initialize caches
        let label_name_cache = Arc::new(DashMap::new());
        let label_id_cache = Arc::new(DashMap::new());
        let type_name_cache = Arc::new(DashMap::new());
        let type_id_cache = Arc::new(DashMap::new());
        let key_name_cache = Arc::new(DashMap::new());
        let key_id_cache = Arc::new(DashMap::new());

        // Populate caches
        for (name, id) in &data.label_name_to_id {
            label_name_cache.insert(name.clone(), *id);
            label_id_cache.insert(*id, name.clone());
        }
        for (name, id) in &data.type_name_to_id {
            type_name_cache.insert(name.clone(), *id);
            type_id_cache.insert(*id, name.clone());
        }
        for (name, id) in &data.key_name_to_id {
            key_name_cache.insert(name.clone(), *id);
            key_id_cache.insert(*id, name.clone());
        }

        Ok(Self {
            file_path,
            mmap,
            file,
            label_name_cache,
            label_id_cache,
            type_name_cache,
            type_id_cache,
            key_name_cache,
            key_id_cache,
            next_label_id: Arc::new(RwLock::new(data.next_label_id)),
            next_type_id: Arc::new(RwLock::new(data.next_type_id)),
            next_key_id: Arc::new(RwLock::new(data.next_key_id)),
        })
    }

    /// Load catalog data from memory-mapped file
    fn load_data(mmap: &Arc<RwLock<MmapMut>>) -> Result<CatalogData> {
        let mmap_guard = mmap.read();

        // Find the end of valid data (look for null terminator or use entire file)
        let data = &mmap_guard[..];

        // Try to deserialize
        match bincode::deserialize(data) {
            Ok(data) => Ok(data),
            Err(_) => {
                // If deserialization fails, return default (empty catalog)
                Ok(CatalogData::default())
            }
        }
    }

    /// Save catalog data to memory-mapped file
    fn save_data(&self) -> Result<()> {
        // Reconstruct data from caches
        let mut data = CatalogData {
            label_name_to_id: self
                .label_name_cache
                .iter()
                .map(|e| (e.key().clone(), *e.value()))
                .collect(),
            label_id_to_name: self
                .label_id_cache
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect(),
            type_name_to_id: self
                .type_name_cache
                .iter()
                .map(|e| (e.key().clone(), *e.value()))
                .collect(),
            type_id_to_name: self
                .type_id_cache
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect(),
            key_name_to_id: self
                .key_name_cache
                .iter()
                .map(|e| (e.key().clone(), *e.value()))
                .collect(),
            key_id_to_name: self
                .key_id_cache
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect(),
            metadata: CatalogMetadata::default(),
            stats: CatalogStats::default(),
            next_label_id: *self.next_label_id.read(),
            next_type_id: *self.next_type_id.read(),
            next_key_id: *self.next_key_id.read(),
        };

        // Serialize
        let serialized = bincode::serialize(&data)
            .map_err(|e| Error::Storage(format!("Failed to serialize catalog: {}", e)))?;

        // Ensure file is large enough
        let required_size = serialized.len();
        let current_size = self.file.metadata()?.len() as usize;

        if required_size > current_size {
            // Grow file
            let new_size = (required_size * 2).max(Self::INITIAL_FILE_SIZE);
            self.file.set_len(new_size as u64)?;

            // Remap
            let new_mmap = unsafe { MmapOptions::new().map_mut(self.file.as_ref())? };
            *self.mmap.write() = new_mmap;
        }

        // Write to memory-mapped file
        let mut mmap_guard = self.mmap.write();
        if serialized.len() > mmap_guard.len() {
            return Err(Error::Storage("Catalog data too large".to_string()));
        }

        mmap_guard[..serialized.len()].copy_from_slice(&serialized);

        // Flush to disk
        mmap_guard
            .flush()
            .map_err(|e| Error::Storage(format!("Failed to flush catalog: {}", e)))?;

        Ok(())
    }

    /// Get or create a label ID
    pub fn get_or_create_label(&self, label: &str) -> Result<LabelId> {
        // Try cache first (lock-free)
        if let Some(id) = self.label_name_cache.get(label) {
            return Ok(*id);
        }

        // Need to create new ID
        let id = {
            let mut next_id = self.next_label_id.write();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Update caches
        self.label_name_cache.insert(label.to_string(), id);
        self.label_id_cache.insert(id, label.to_string());

        // Save to disk
        self.save_data()?;

        Ok(id)
    }

    /// Get label name by ID
    pub fn get_label_name(&self, id: LabelId) -> Result<Option<String>> {
        // Try cache first (lock-free)
        if let Some(name) = self.label_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }
        Ok(None)
    }

    /// Get label ID by name
    pub fn get_label_id(&self, label: &str) -> Result<Option<LabelId>> {
        // Try cache first (lock-free)
        if let Some(id) = self.label_name_cache.get(label) {
            return Ok(Some(*id));
        }
        Ok(None)
    }

    /// Get or create a type ID
    pub fn get_or_create_type(&self, type_name: &str) -> Result<TypeId> {
        // Try cache first (lock-free)
        if let Some(id) = self.type_name_cache.get(type_name) {
            return Ok(*id);
        }

        // Need to create new ID
        let id = {
            let mut next_id = self.next_type_id.write();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Update caches
        self.type_name_cache.insert(type_name.to_string(), id);
        self.type_id_cache.insert(id, type_name.to_string());

        // Save to disk
        self.save_data()?;

        Ok(id)
    }

    /// Get type name by ID
    pub fn get_type_name(&self, id: TypeId) -> Result<Option<String>> {
        // Try cache first (lock-free)
        if let Some(name) = self.type_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }
        Ok(None)
    }

    /// Get type ID by name
    pub fn get_type_id(&self, type_name: &str) -> Result<Option<TypeId>> {
        // Try cache first (lock-free)
        if let Some(id) = self.type_name_cache.get(type_name) {
            return Ok(Some(*id));
        }
        Ok(None)
    }

    /// Get or create a key ID
    pub fn get_or_create_key(&self, key: &str) -> Result<KeyId> {
        // Try cache first (lock-free)
        if let Some(id) = self.key_name_cache.get(key) {
            return Ok(*id);
        }

        // Need to create new ID
        let id = {
            let mut next_id = self.next_key_id.write();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Update caches
        self.key_name_cache.insert(key.to_string(), id);
        self.key_id_cache.insert(id, key.to_string());

        // Save to disk
        self.save_data()?;

        Ok(id)
    }

    /// Get key name by ID
    pub fn get_key_name(&self, id: KeyId) -> Result<Option<String>> {
        // Try cache first (lock-free)
        if let Some(name) = self.key_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }
        Ok(None)
    }

    /// Get key ID by name
    pub fn get_key_id(&self, key: &str) -> Result<Option<KeyId>> {
        // Try cache first (lock-free)
        if let Some(id) = self.key_name_cache.get(key) {
            return Ok(Some(*id));
        }
        Ok(None)
    }
}
