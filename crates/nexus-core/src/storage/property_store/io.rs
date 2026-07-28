use super::*;
use std::fs::OpenOptions;

impl PropertyStore {
    /// Write a u64 value at the given offset
    pub(super) fn write_u64(&mut self, offset: u64, value: u64) {
        let bytes = value.to_le_bytes();
        self.mmap[offset as usize..offset as usize + 8].copy_from_slice(&bytes);
    }

    /// Write a u32 value at the given offset
    pub(super) fn write_u32(&mut self, offset: u64, value: u32) {
        let bytes = value.to_le_bytes();
        self.mmap[offset as usize..offset as usize + 4].copy_from_slice(&bytes);
    }

    /// Write a u8 value at the given offset
    pub(super) fn write_u8(&mut self, offset: u64, value: u8) {
        self.mmap[offset as usize] = value;
    }

    /// Write bytes at the given offset
    pub(super) fn write_bytes(&mut self, offset: u64, data: &[u8]) {
        self.mmap[offset as usize..offset as usize + data.len()].copy_from_slice(data);
    }

    /// Overwrite the `entity_type` header byte of the property entry
    /// starting at `entry_offset` with [`ENTITY_TYPE_TOMBSTONE`], so the
    /// shared rebuild scanner ([`PropertyStore::scan_entry_at`]) strides
    /// over it without re-indexing it. The `data_size` field (and the
    /// payload it describes) is left untouched, so the entry's on-disk
    /// footprint never changes. See
    /// phase0_fix-deleted-properties-resurrected-on-rebuild.
    pub(super) fn write_tombstone(&mut self, entry_offset: u64) {
        self.write_u8(entry_offset + 8, ENTITY_TYPE_TOMBSTONE);
    }

    /// Read a u64 value from the given offset.
    ///
    /// #3: bounds-checked so a caller that omits its own pre-check cannot
    /// panic here. Returns 0 when the 8-byte read would run past EOF — never
    /// reached with a valid header offset, which the call sites already guard.
    pub(super) fn read_u64(&self, offset: u64) -> u64 {
        let start = offset as usize;
        match start.checked_add(8) {
            Some(end) if end <= self.mmap.len() => {}
            _ => return 0,
        }
        u64::from_le_bytes([
            self.mmap[start],
            self.mmap[start + 1],
            self.mmap[start + 2],
            self.mmap[start + 3],
            self.mmap[start + 4],
            self.mmap[start + 5],
            self.mmap[start + 6],
            self.mmap[start + 7],
        ])
    }

    /// Read a u32 value from the given offset (bounds-checked; see read_u64).
    pub(super) fn read_u32(&self, offset: u64) -> u32 {
        let start = offset as usize;
        match start.checked_add(4) {
            Some(end) if end <= self.mmap.len() => {}
            _ => return 0,
        }
        u32::from_le_bytes([
            self.mmap[start],
            self.mmap[start + 1],
            self.mmap[start + 2],
            self.mmap[start + 3],
        ])
    }

    /// Read a u8 value from the given offset (bounds-checked; see read_u64).
    pub(super) fn read_u8(&self, offset: u64) -> u8 {
        let start = offset as usize;
        if start >= self.mmap.len() {
            return 0;
        }
        self.mmap[start]
    }

    /// Return the byte-offset stored in the reverse index for `(entity_id, entity_type)`.
    ///
    /// This is the canonical way to recover a valid `prop_ptr` for a node (or
    /// relationship) when the on-disk record's pointer is known to be corrupt.
    /// The index must be populated (via [`PropertyStore::ensure_index_populated`])
    /// before calling this.
    pub fn offset_for(&self, entity_id: u64, entity_type: EntityType) -> Option<u64> {
        self.reverse_index.get(&(entity_id, entity_type)).copied()
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
