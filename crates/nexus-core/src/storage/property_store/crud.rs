use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;

impl PropertyStore {
    /// Create a new property store
    pub fn new(path: PathBuf) -> Result<Self> {
        let property_file = path.join("properties.store");

        // Whether the backing file already exists with (potential) data. For an
        // existing file we must let rebuild_index() perform a full scan from
        // disk, so next_offset is seeded to 0 below (the >0 "preserve" branch in
        // rebuild_index would otherwise skip the on-disk scan and lose the index
        // on every reopen — the root cause of issue #4 property loss on reboot).
        let file_existed = property_file.exists();

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
            // For a brand-new file, start at offset 1 (offset 0 is reserved
            // because prop_ptr=0 means "no properties"). For an existing file,
            // seed 0 so rebuild_index() takes the full on-disk scan branch and
            // reconstructs the index correctly (issue #4).
            next_offset: if file_existed { 0 } else { 1 },
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
        // #3: reject any offset whose full 13-byte entry header would run past
        // EOF (see get_entity_info_at_offset).
        match offset.checked_add(PROPERTY_ENTRY_HEADER_SIZE) {
            Some(end) if end <= self.mmap.len() as u64 => {}
            _ => return Ok(None),
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
        // #3: reject any offset whose full 13-byte header would run past
        // EOF, not just `offset >= len`. A tail shorter than the header cannot
        // hold a valid entry, and the read_u64/read_u8 below would otherwise
        // walk off the mapping and panic on a corrupt/crafted prop_ptr.
        match offset.checked_add(PROPERTY_ENTRY_HEADER_SIZE) {
            Some(end) if end <= self.mmap.len() as u64 => {}
            _ => return None,
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

        // phase0_fix-property-store-shrink-corruption (§2.1, option b —
        // grow-only): rewrite in place ONLY when the footprint is
        // IDENTICAL. A strictly smaller payload used to reuse this slot by
        // overwriting `data_size` and the leading bytes while leaving the
        // freed tail of the old, longer payload untouched on disk. On
        // reopen, `rebuild_index`/`ensure_index_populated` stride by the
        // (now smaller) stored `data_size`, land inside that stale tail
        // instead of at the next entity's true header, and either drop
        // every later entity or fabricate a wrong mapping. Allocating fresh
        // space for anything that isn't a same-size rewrite guarantees
        // `data_size` on disk always equals the entry's physical footprint.
        if new_data_size == existing_data_size {
            tracing::debug!("[update_properties] Updating in place: offset={}", offset);
            self.write_u32(offset + 9, new_data_size);
            self.write_bytes(offset + 13, &serialized);
            Ok(offset) // Return same offset
        } else {
            // Need to allocate new space (grow OR shrink — see above: only
            // an identical-size rewrite may reuse the existing slot).
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
    ///
    /// Removes the entity from the in-memory indexes AND tombstones its
    /// on-disk entry (phase0_fix-deleted-properties-resurrected-on-rebuild
    /// §3.1). Without the tombstone, the entry's bytes remain a
    /// well-formed, fully-parseable entry and the next rebuild scan
    /// (`rebuild_index` / `ensure_index_populated`) would re-index it,
    /// resurrecting the "deleted" property after a restart.
    pub fn delete_properties(&mut self, entity_id: u64, entity_type: EntityType) -> Result<()> {
        if let Some(property_ptr) = self.reverse_index.remove(&(entity_id, entity_type)) {
            self.index.remove(&property_ptr);
            self.write_tombstone(property_ptr);
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
}
