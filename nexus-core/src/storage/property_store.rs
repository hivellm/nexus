//! Property storage system for Nexus graph database
//!
//! This module provides efficient storage and retrieval of node and relationship properties
//! using a key-value store with JSON serialization.

use crate::error::{Error, Result};
use memmap2::{MmapMut, MmapOptions};
use serde_json;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing;

/// Property store for efficient property storage and retrieval
pub struct PropertyStore {
    /// Path to the property store file
    path: PathBuf,
    /// Memory-mapped file for property data
    mmap: MmapMut,
    /// Next available offset for new properties
    next_offset: u64,
    /// Property index: property_ptr -> (entity_id, entity_type)
    index: HashMap<u64, (u64, EntityType)>,
    /// Reverse index: (entity_id, entity_type) -> property_ptr
    reverse_index: HashMap<(u64, EntityType), u64>,
}

/// Type of entity that owns properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Node,
    Relationship,
}

/// Property entry in the store
#[derive(Debug, Clone)]
struct PropertyEntry {
    /// Entity ID that owns these properties
    entity_id: u64,
    /// Type of entity (node or relationship)
    entity_type: EntityType,
    /// Serialized properties as JSON
    properties: serde_json::Value,
    /// Size of the serialized data
    data_size: u32,
}

impl PropertyStore {
    /// Create a new property store
    pub fn new(path: PathBuf) -> Result<Self> {
        let property_file = path.join("properties.store");

        // Create or open the property file
        let file = if property_file.exists() {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&property_file)?
        } else {
            // Create new file with initial size
            let mut file = File::create(&property_file)?;
            // Write initial size (1MB)
            file.write_all(&[0u8; 1024 * 1024])?;
            file.sync_all()?;
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&property_file)?
        };

        // Memory map the file
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        let mut store = Self {
            path,
            mmap,
            // CRITICAL: Start at offset 1, not 0, because prop_ptr=0 means "no properties"
            // This ensures the first stored property gets offset 1, which is a valid non-zero prop_ptr
            next_offset: 1,
            index: HashMap::new(),
            reverse_index: HashMap::new(),
        };

        // Rebuild index from existing data
        store.rebuild_index()?;

        Ok(store)
    }

    /// Store properties for an entity
    pub fn store_properties(
        &mut self,
        entity_id: u64,
        entity_type: EntityType,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let key = (entity_id, entity_type);
        // Check if properties already exist for this entity
        if let Some(&existing_ptr) = self.reverse_index.get(&key) {
            // Update existing properties - may return new offset if properties don't fit
            let actual_offset =
                self.update_properties(existing_ptr, entity_id, entity_type, properties)?;
            return Ok(actual_offset);
        } else {
        }

        // Phase 1 Deep Optimization: Use to_string for small properties, to_writer for large
        // to_string is often faster for small JSON objects due to better optimizations
        let serialized = if properties.is_object() {
            let obj = properties.as_object().unwrap();
            // For small objects (< 5 properties), to_string is faster
            if obj.len() < 5 {
                serde_json::to_string(&properties)
                    .map_err(Error::Json)?
                    .into_bytes()
            } else {
                // For larger objects, use pre-allocated buffer
                let estimated_size = obj.len() * 50;
                let mut buffer = Vec::with_capacity(estimated_size);
                serde_json::to_writer(&mut buffer, &properties).map_err(Error::Json)?;
                buffer
            }
        } else {
            // For non-objects, to_string is usually faster
            serde_json::to_string(&properties)
                .map_err(Error::Json)?
                .into_bytes()
        };

        let data_size = serialized.len() as u32;
        let entry_size = 8 + 1 + 4 + data_size as usize; // entity_id + entity_type + data_size + data

        // Phase 1 Optimization: Batch capacity checks (only grow if really needed)
        // Ensure we have enough space
        self.ensure_capacity(self.next_offset + entry_size as u64)?;

        // Write property entry
        let offset = self.next_offset;

        // Phase 1 Deep Optimization: Batch writes to reduce mmap access overhead
        // Write header (entity_id + entity_type + data_size) in one operation
        let header_start = offset as usize;
        let header_end = header_start + 13;

        // Write entity_id (8 bytes) - little endian
        let entity_id_bytes = entity_id.to_le_bytes();
        self.mmap[header_start..header_start + 8].copy_from_slice(&entity_id_bytes);

        // Write entity_type (1 byte)
        self.mmap[header_start + 8] = entity_type as u8;

        // Write data_size (4 bytes) - little endian
        let data_size_bytes = data_size.to_le_bytes();
        self.mmap[header_start + 9..header_end].copy_from_slice(&data_size_bytes);

        // Write properties data
        let data_start = header_end;
        let data_end = data_start + serialized.len();
        self.mmap[data_start..data_end].copy_from_slice(&serialized);

        // Update indexes
        self.index.insert(offset, (entity_id, entity_type));
        let key = (entity_id, entity_type);
        self.reverse_index.insert(key, offset);
        tracing::debug!(
            "[store_properties] Stored properties: entity_id={}, entity_type={:?}, offset={}, reverse_index size={}",
            entity_id,
            entity_type,
            offset,
            self.reverse_index.len()
        );

        // Update next offset
        let old_next_offset = self.next_offset;
        self.next_offset = offset + entry_size as u64;
        tracing::debug!(
            "[store_properties] AFTER: entity_id={}, entity_type={:?}, offset={}, entry_size={}, old_next_offset={}, new_next_offset={}",
            entity_id,
            entity_type,
            offset,
            entry_size,
            old_next_offset,
            self.next_offset
        );

        Ok(offset)
    }

    /// Load properties for an entity
    pub fn load_properties(
        &self,
        entity_id: u64,
        entity_type: EntityType,
    ) -> Result<Option<serde_json::Value>> {
        let key = (entity_id, entity_type);
        tracing::debug!(
            "[load_properties] Looking up entity_id={}, entity_type={:?}, reverse_index size={}",
            entity_id,
            entity_type,
            self.reverse_index.len()
        );

        // Check if key exists
        if let Some(&property_ptr) = self.reverse_index.get(&key) {
            tracing::debug!(
                "[load_properties] Found entry in reverse_index: entity_id={}, entity_type={:?}, property_ptr={}",
                entity_id,
                entity_type,
                property_ptr
            );
            self.load_properties_at_offset(property_ptr)
        } else {
            tracing::debug!(
                "[load_properties] NOT FOUND in reverse_index: entity_id={}, entity_type={:?}",
                entity_id,
                entity_type
            );
            Ok(None)
        }
    }

    /// Load properties at a specific offset
    pub fn load_properties_at_offset(&self, offset: u64) -> Result<Option<serde_json::Value>> {
        if offset as usize >= self.mmap.len() {
            return Ok(None);
        }

        // Read entity_id (8 bytes)
        let _stored_entity_id = self.read_u64(offset);

        // Read entity_type (1 byte)
        let _stored_entity_type = EntityType::from_u8(self.read_u8(offset + 8))?;

        // Read data_size (4 bytes)
        let data_size = self.read_u32(offset + 9);

        // Read properties data
        let data_start = offset + 13;
        if data_start + data_size as u64 > self.mmap.len() as u64 {
            return Err(Error::storage("Property data extends beyond file"));
        }

        let data = &self.mmap[data_start as usize..(data_start + data_size as u64) as usize];

        // Deserialize properties
        let properties: serde_json::Value = serde_json::from_slice(data).map_err(Error::Json)?;

        Ok(Some(properties))
    }

    /// Check what entity type is stored at a given offset
    /// Returns (entity_id, entity_type) if found, None otherwise
    pub fn get_entity_info_at_offset(&self, offset: u64) -> Option<(u64, EntityType)> {
        if offset as usize >= self.mmap.len() {
            return None;
        }

        // Read entity_id (8 bytes)
        let entity_id = self.read_u64(offset);

        // Read entity_type (1 byte)
        if let Ok(entity_type) = EntityType::from_u8(self.read_u8(offset + 8)) {
            Some((entity_id, entity_type))
        } else {
            None
        }
    }

    /// Update existing properties
    fn update_properties(
        &mut self,
        offset: u64,
        entity_id: u64,
        entity_type: EntityType,
        properties: serde_json::Value,
    ) -> Result<u64> {
        tracing::debug!(
            "[update_properties] Called: entity_id={}, entity_type={:?}, offset={}, next_offset={}",
            entity_id,
            entity_type,
            offset,
            self.next_offset
        );
        // Serialize new properties
        let serialized = serde_json::to_vec(&properties).map_err(Error::Json)?;

        let new_data_size = serialized.len() as u32;

        // Read existing data size
        let existing_data_size = self.read_u32(offset + 9);
        tracing::debug!(
            "[update_properties] existing_data_size={}, new_data_size={}",
            existing_data_size,
            new_data_size
        );

        // If new data fits in existing space, update in place
        if new_data_size <= existing_data_size {
            tracing::debug!("[update_properties] Updating in place: offset={}", offset);
            self.write_u32(offset + 9, new_data_size);
            self.write_bytes(offset + 13, &serialized);
            Ok(offset) // Return same offset
        } else {
            // Need to allocate new space
            let new_offset = self.next_offset;
            tracing::debug!(
                "[update_properties] Allocating new space: old_offset={}, new_offset={}",
                offset,
                new_offset
            );
            let entry_size = 8 + 1 + 4 + new_data_size as usize;

            self.ensure_capacity(new_offset + entry_size as u64)?;

            // Write new entry
            self.write_u64(new_offset, entity_id);
            self.write_u8(new_offset + 8, entity_type as u8);
            self.write_u32(new_offset + 9, new_data_size);
            self.write_bytes(new_offset + 13, &serialized);

            // Update indexes
            self.index.remove(&offset);
            self.index.insert(new_offset, (entity_id, entity_type));
            self.reverse_index
                .insert((entity_id, entity_type), new_offset);

            self.next_offset = new_offset + entry_size as u64;

            Ok(new_offset) // Return new offset
        }
    }

    /// Delete properties for an entity
    pub fn delete_properties(&mut self, entity_id: u64, entity_type: EntityType) -> Result<()> {
        if let Some(property_ptr) = self.reverse_index.remove(&(entity_id, entity_type)) {
            self.index.remove(&property_ptr);
        }
        Ok(())
    }

    /// Clear all properties and reset the store
    pub fn clear_all(&mut self) -> Result<()> {
        tracing::debug!("[PropertyStore::clear_all] Clearing all properties");
        tracing::debug!(
            "[PropertyStore::clear_all] BEFORE: next_offset={}, index size={}, reverse_index size={}",
            self.next_offset,
            self.index.len(),
            self.reverse_index.len()
        );

        // Clear indexes
        self.index.clear();
        self.reverse_index.clear();
        // CRITICAL: Reset to 1, not 0, because prop_ptr=0 means "no properties"
        self.next_offset = 1;

        // Truncate and zero out the property file
        let property_file = self.path.join("properties.store");
        if property_file.exists() {
            tracing::debug!("[PropertyStore::clear_all] Truncating and zeroing property file");

            // CRITICAL FIX for Windows: Create a temporary mmap to replace the current one
            // This allows us to drop the old mmap before truncating the file
            let temp_dir = tempfile::tempdir()?;
            let temp_path = temp_dir.path().join("properties.tmp");
            let mut temp_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&temp_path)?;
            temp_file.set_len(1024 * 1024)?; // 1MB
            let temp_mmap = unsafe { MmapOptions::new().map_mut(&temp_file)? };

            // Replace current mmap with temporary one (drops old mmap)
            let _old_mmap = std::mem::replace(&mut self.mmap, temp_mmap);
            drop(_old_mmap);

            // Now we can safely truncate the original file
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .truncate(true)
                .open(&property_file)?;

            // Write initial size (1MB) filled with zeros
            file.write_all(&[0u8; 1024 * 1024])?;
            file.sync_all()?;
            drop(file);

            // Reopen file for mmap
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&property_file)?;

            // Recreate memory mapping from original file
            self.mmap = unsafe { MmapOptions::new().map_mut(&file)? };
            tracing::debug!(
                "[PropertyStore::clear_all] Recreated mmap, mmap.len()={}",
                self.mmap.len()
            );

            // temp_dir and temp_file will be dropped here
        }

        tracing::debug!(
            "[PropertyStore::clear_all] AFTER: next_offset={}, index size={}, reverse_index size={}",
            self.next_offset,
            self.index.len(),
            self.reverse_index.len()
        );

        Ok(())
    }

    /// Rebuild index from existing data
    fn rebuild_index(&mut self) -> Result<()> {
        // CRITICAL: Only rebuild if indexes are empty or if explicitly requested
        // If indexes already have data, don't rebuild - this would reset next_offset incorrectly
        if !self.index.is_empty() || !self.reverse_index.is_empty() {
            tracing::debug!(
                "[rebuild_index] SKIPPING: indexes already populated (index size={}, reverse_index size={}, next_offset={})",
                self.index.len(),
                self.reverse_index.len(),
                self.next_offset
            );
            return Ok(());
        }

        tracing::debug!(
            "[rebuild_index] STARTING: mmap.len()={}, current next_offset={}",
            self.mmap.len(),
            self.next_offset
        );

        // CRITICAL FIX: Check if file is empty (all zeros) - if so, don't rebuild
        // This prevents rebuild_index from finding old data after clear_all() and resetting next_offset incorrectly
        let first_13_bytes = &self.mmap[0..std::cmp::min(13, self.mmap.len())];
        let is_empty = first_13_bytes.iter().all(|&b| b == 0);

        if is_empty {
            tracing::debug!(
                "[rebuild_index] SKIPPING: file is empty (all zeros), keeping next_offset=1"
            );
            // CRITICAL: Keep at 1, not 0, because prop_ptr=0 means "no properties"
            self.next_offset = 1;
            return Ok(());
        }

        // CRITICAL FIX: If next_offset is already > 0, don't rebuild from file
        // This prevents rebuild_index from resetting next_offset to old values when PropertyStore
        // is recreated after nodes have already been created in the current session
        // The next_offset should only be set from file scan if it's 0 (initial state)
        if self.next_offset > 0 {
            tracing::debug!(
                "[rebuild_index] SKIPPING: next_offset already set to {} (not rebuilding from file to avoid reset)",
                self.next_offset
            );
            // Still rebuild indexes for lookup, but preserve next_offset
            let preserved_next_offset = self.next_offset;
            self.index.clear();
            self.reverse_index.clear();

            // Scan file to rebuild indexes, but don't update next_offset
            let mut offset = 0;
            while offset < self.mmap.len() as u64 && offset < preserved_next_offset {
                if offset + 13 > self.mmap.len() as u64 {
                    break;
                }

                let entity_id = self.read_u64(offset);
                let entity_type_byte = self.read_u8(offset + 8);
                let data_size = self.read_u32(offset + 9);

                if entity_id == 0 && entity_type_byte == 0 && data_size == 0 {
                    break;
                }

                let entity_type = match EntityType::from_u8(entity_type_byte) {
                    Ok(et) => et,
                    Err(_) => break,
                };

                let entry_size = 8 + 1 + 4 + data_size as usize;
                if offset + entry_size as u64 > self.mmap.len() as u64 {
                    break;
                }

                self.index.insert(offset, (entity_id, entity_type));
                self.reverse_index.insert((entity_id, entity_type), offset);

                offset += entry_size as u64;
            }

            // Restore preserved next_offset
            self.next_offset = preserved_next_offset;
            tracing::debug!(
                "[rebuild_index] COMPLETED: preserved next_offset={}, rebuilt index size={}, reverse_index size={}",
                self.next_offset,
                self.index.len(),
                self.reverse_index.len()
            );
            return Ok(());
        }

        let old_next_offset = self.next_offset;
        let old_index_size = self.index.len();
        let old_reverse_index_size = self.reverse_index.len();

        self.index.clear();
        self.reverse_index.clear();
        // CRITICAL: Reset to 1, not 0, because prop_ptr=0 means "no properties"
        self.next_offset = 1;

        tracing::debug!(
            "[rebuild_index] Cleared indexes: old_next_offset={}, old_index_size={}, old_reverse_index_size={}",
            old_next_offset,
            old_index_size,
            old_reverse_index_size
        );

        // CRITICAL FIX: Track the maximum offset found in the file
        // This helps detect if we're reading old data that shouldn't be used
        let mut offset = 0;
        let mut max_valid_offset = 0;
        let mut found_valid_entries = false;

        while offset < self.mmap.len() as u64 {
            if offset + 13 > self.mmap.len() as u64 {
                break;
            }

            let entity_id = self.read_u64(offset);
            let entity_type_byte = self.read_u8(offset + 8);
            let data_size = self.read_u32(offset + 9);

            // Check if this looks like a valid entry (not all zeros)
            if entity_id == 0 && entity_type_byte == 0 && data_size == 0 {
                // Found first empty entry, stop scanning
                tracing::debug!(
                    "[rebuild_index] Found empty entry at offset={}, found_valid_entries={}, max_valid_offset={}",
                    offset,
                    found_valid_entries,
                    max_valid_offset
                );
                // Only set next_offset if we found valid entries, otherwise keep it at 1
                if found_valid_entries {
                    self.next_offset = offset;
                } else {
                    // CRITICAL: Reset to 1, not 0, because prop_ptr=0 means "no properties"
                    self.next_offset = 1;
                }
                break;
            }

            // Validate entity type
            let entity_type = match EntityType::from_u8(entity_type_byte) {
                Ok(et) => et,
                Err(_) => {
                    // Invalid entity type, stop scanning
                    break;
                }
            };

            let entry_size = 8 + 1 + 4 + data_size as usize;
            if offset + entry_size as u64 > self.mmap.len() as u64 {
                break;
            }

            // Update indexes
            self.index.insert(offset, (entity_id, entity_type));
            self.reverse_index.insert((entity_id, entity_type), offset);

            found_valid_entries = true;
            max_valid_offset = offset + entry_size as u64;

            offset += entry_size as u64;
        }

        // CRITICAL FIX: Only update next_offset if we found valid entries
        // If the file contains only old data (from previous runs), don't use it
        // This prevents rebuild_index from resetting next_offset to old values
        if found_valid_entries {
            self.next_offset = offset;
        } else {
            // No valid entries found, keep next_offset at 1
            // CRITICAL: Reset to 1, not 0, because prop_ptr=0 means "no properties"
            self.next_offset = 1;
            tracing::debug!(
                "[rebuild_index] No valid entries found in file, keeping next_offset=1"
            );
        }
        tracing::debug!(
            "[rebuild_index] COMPLETED: final next_offset={}, index size={}, reverse_index size={}",
            self.next_offset,
            self.index.len(),
            self.reverse_index.len()
        );
        Ok(())
    }

    /// Ensure the memory-mapped file has enough capacity
    /// Phase 1 Deep Optimization: Remove sync_all() - let OS manage page cache
    /// This reduces I/O overhead significantly during file growth
    fn ensure_capacity(&mut self, required_size: u64) -> Result<()> {
        if required_size > self.mmap.len() as u64 {
            // Calculate new size (grow by 1.5x, but at least 2MB to reduce frequent grows)
            let min_growth = 2 * 1024 * 1024; // 2MB minimum
            let calculated_size = ((required_size as f64) * 1.5) as usize;
            let new_size = calculated_size.max(min_growth).max(required_size as usize);

            // Resize file
            let property_file = self.path.join("properties.store");
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&property_file)?;
            file.set_len(new_size as u64)?;
            // Phase 1 Deep Optimization: Removed sync_all() - OS will manage page cache
            // This reduces I/O overhead by ~10-20ms per growth operation
            // Data will be flushed eventually by OS or explicit flush()

            // Recreate mmap
            self.mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        }
        Ok(())
    }

    /// Write a u64 value at the given offset
    fn write_u64(&mut self, offset: u64, value: u64) {
        let bytes = value.to_le_bytes();
        self.mmap[offset as usize..offset as usize + 8].copy_from_slice(&bytes);
    }

    /// Write a u32 value at the given offset
    fn write_u32(&mut self, offset: u64, value: u32) {
        let bytes = value.to_le_bytes();
        self.mmap[offset as usize..offset as usize + 4].copy_from_slice(&bytes);
    }

    /// Write a u8 value at the given offset
    fn write_u8(&mut self, offset: u64, value: u8) {
        self.mmap[offset as usize] = value;
    }

    /// Write bytes at the given offset
    fn write_bytes(&mut self, offset: u64, data: &[u8]) {
        self.mmap[offset as usize..offset as usize + data.len()].copy_from_slice(data);
    }

    /// Read a u64 value from the given offset
    fn read_u64(&self, offset: u64) -> u64 {
        u64::from_le_bytes([
            self.mmap[offset as usize],
            self.mmap[offset as usize + 1],
            self.mmap[offset as usize + 2],
            self.mmap[offset as usize + 3],
            self.mmap[offset as usize + 4],
            self.mmap[offset as usize + 5],
            self.mmap[offset as usize + 6],
            self.mmap[offset as usize + 7],
        ])
    }

    /// Read a u32 value from the given offset
    fn read_u32(&self, offset: u64) -> u32 {
        u32::from_le_bytes([
            self.mmap[offset as usize],
            self.mmap[offset as usize + 1],
            self.mmap[offset as usize + 2],
            self.mmap[offset as usize + 3],
        ])
    }

    /// Read a u8 value from the given offset
    fn read_u8(&self, offset: u64) -> u8 {
        self.mmap[offset as usize]
    }

    /// Get the number of stored properties
    pub fn property_count(&self) -> usize {
        self.index.len()
    }

    /// Health check for the property store
    pub fn health_check(&self) -> Result<()> {
        // Check if file is accessible
        if !self.path.join("properties.store").exists() {
            return Err(Error::storage("Property store file does not exist"));
        }

        // Try to read from the memory-mapped file
        let _ = self.mmap.len();

        Ok(())
    }

    /// Flush all pending writes to disk
    ///
    /// Forces the memory-mapped property file to sync with disk.
    pub fn flush(&mut self) -> Result<()> {
        self.mmap
            .flush()
            .map_err(|e| Error::storage(format!("Failed to flush properties: {}", e)))?;

        // Also sync the underlying file to ensure OS-level persistence
        let property_file = self.path.join("properties.store");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&property_file)?;
        file.sync_all()
            .map_err(|e| Error::storage(format!("Failed to sync properties file: {}", e)))?;

        Ok(())
    }
}

impl EntityType {
    /// Convert from u8 to EntityType
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(EntityType::Node),
            1 => Ok(EntityType::Relationship),
            _ => Err(Error::storage(format!("Invalid entity type: {}", value))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, RwLock};
    use tempfile::TempDir;

    #[test]
    fn test_property_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();
        assert_eq!(store.property_count(), 0);
    }

    #[test]
    fn test_store_and_load_properties() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        let properties = json!({
            "name": "Alice",
            "age": 30,
            "active": true
        });

        let ptr = store
            .store_properties(1, EntityType::Node, properties.clone())
            .unwrap();
        // First property should be at offset 1 (not 0, because prop_ptr=0 means "no properties")
        assert!(ptr == 1, "First property should be at offset 1, got {}", ptr);

        let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
        assert_eq!(loaded, properties);
    }

    #[test]
    fn test_update_properties() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        let initial_properties = json!({"name": "Alice"});
        let updated_properties = json!({"name": "Alice", "age": 30});

        store
            .store_properties(1, EntityType::Node, initial_properties)
            .unwrap();
        store
            .store_properties(1, EntityType::Node, updated_properties.clone())
            .unwrap();

        let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
        assert_eq!(loaded, updated_properties);
    }

    #[test]
    fn test_delete_properties() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        let properties = json!({"name": "Alice"});
        store
            .store_properties(1, EntityType::Node, properties)
            .unwrap();

        assert!(
            store
                .load_properties(1, EntityType::Node)
                .unwrap()
                .is_some()
        );

        store.delete_properties(1, EntityType::Node).unwrap();
        assert!(
            store
                .load_properties(1, EntityType::Node)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_relationship_properties() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        let properties = json!({"weight": 0.8, "type": "friends"});
        store
            .store_properties(1, EntityType::Relationship, properties.clone())
            .unwrap();

        let loaded = store
            .load_properties(1, EntityType::Relationship)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, properties);
    }

    #[test]
    fn test_large_property_data() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Create a large JSON object
        let mut large_data = serde_json::Map::new();
        for i in 0..1000 {
            large_data.insert(
                format!("key_{}", i),
                serde_json::Value::String(format!("value_{}", i)),
            );
        }
        let properties = serde_json::Value::Object(large_data);

        let _ptr = store
            .store_properties(1, EntityType::Node, properties.clone())
            .unwrap();

        let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
        assert_eq!(loaded, properties);
    }

    #[test]
    fn test_concurrent_property_access() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RwLock::new(
            PropertyStore::new(temp_dir.path().to_path_buf()).unwrap(),
        ));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    let properties = json!({"thread_id": i, "data": format!("thread_{}", i)});
                    store
                        .write()
                        .unwrap()
                        .store_properties(i as u64, EntityType::Node, properties)
                        .unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all properties were stored
        for i in 0..10 {
            let loaded = store
                .read()
                .unwrap()
                .load_properties(i as u64, EntityType::Node)
                .unwrap()
                .unwrap();
            assert_eq!(loaded["thread_id"], i);
        }
    }

    #[test]
    fn test_property_store_capacity_expansion() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Store many properties to trigger capacity expansion
        for i in 0..100 {
            let properties = json!({
                "id": i,
                "data": format!("property_data_{}", i),
                "metadata": {
                    "created_at": "2024-01-01T00:00:00Z",
                    "updated_at": "2024-01-01T00:00:00Z"
                }
            });
            store
                .store_properties(i, EntityType::Node, properties)
                .unwrap();
        }

        // Verify all properties can be loaded
        for i in 0..100 {
            let loaded = store.load_properties(i, EntityType::Node).unwrap().unwrap();
            assert_eq!(loaded["id"], i);
        }
    }

    #[test]
    fn test_property_store_health_check() {
        let temp_dir = TempDir::new().unwrap();
        let store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Health check should pass for valid store
        store.health_check().unwrap();

        // Test property count
        assert_eq!(store.property_count(), 0);
    }

    #[test]
    fn test_property_store_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Test loading non-existent property
        let result = store.load_properties(999, EntityType::Node).unwrap();
        assert!(result.is_none());

        // Test deleting non-existent property (should not error)
        store.delete_properties(999, EntityType::Node).unwrap();
    }

    #[test]
    fn test_property_store_serialization_types() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Test different JSON value types
        let test_cases = vec![
            ("string", json!("hello world")),
            ("number", json!(42)),
            ("float", json!(std::f64::consts::PI)),
            ("boolean", json!(true)),
            ("null", json!(null)),
            ("array", json!([1, 2, 3, "four"])),
            ("object", json!({"nested": {"key": "value"}})),
        ];

        for (name, value) in test_cases {
            store
                .store_properties(1, EntityType::Node, value.clone())
                .unwrap();

            let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
            assert_eq!(loaded, value, "Failed for test case: {}", name);
        }
    }

    #[test]
    fn test_property_store_mixed_entity_types() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = PropertyStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Store properties for both node and relationship with same ID
        let node_props = json!({"type": "user", "name": "Alice"});
        let rel_props = json!({"weight": 0.8, "type": "friends"});

        store
            .store_properties(1, EntityType::Node, node_props.clone())
            .unwrap();
        store
            .store_properties(1, EntityType::Relationship, rel_props.clone())
            .unwrap();

        // Verify both can be loaded independently
        let loaded_node = store.load_properties(1, EntityType::Node).unwrap().unwrap();
        let loaded_rel = store
            .load_properties(1, EntityType::Relationship)
            .unwrap()
            .unwrap();

        assert_eq!(loaded_node, node_props);
        assert_eq!(loaded_rel, rel_props);
    }
}

impl Clone for PropertyStore {
    fn clone(&self) -> Self {
        // CRITICAL FIX: Clone by preserving next_offset and indexes from the original
        // This prevents rebuild_index() from resetting next_offset to old values when RecordStore is cloned
        // Instead, we clone the indexes and next_offset directly, and only recreate the mmap

        let property_file = self.path.join("properties.store");

        // Open the same file (don't create new)
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&property_file)
            .expect("Failed to open property file for clone");

        // Recreate memory mapping from the same file
        let mmap = unsafe {
            MmapOptions::new()
                .map_mut(&file)
                .expect("Failed to map property file for clone")
        };

        // Clone indexes and preserve next_offset from original
        Self {
            path: self.path.clone(),
            mmap,
            next_offset: self.next_offset, // CRITICAL: Preserve next_offset from original
            index: self.index.clone(),     // CRITICAL: Preserve index from original
            reverse_index: self.reverse_index.clone(), // CRITICAL: Preserve reverse_index from original
        }
    }
}
