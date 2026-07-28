use std::sync::atomic::Ordering;

use crate::error::Result;
use crate::storage::record_store::RecordStore;
use crate::storage::records::{INITIAL_NODES_FILE_SIZE, INITIAL_RELS_FILE_SIZE};
use memmap2::MmapOptions;

impl RecordStore {
    /// Clear all data from the storage
    pub fn clear_all(&mut self) -> Result<()> {
        tracing::debug!("[RecordStore::clear_all] Clearing all storage data");

        // Reset counters
        self.next_node_id.store(0, Ordering::SeqCst);
        self.next_rel_id.store(0, Ordering::SeqCst);

        // The record files are re-mapped wholesale below, bypassing
        // `write_rel` — the only other place the adjacency index is
        // maintained — so drop every entry explicitly.
        self.adjacency_index.clear();

        // CRITICAL FIX: Clear property store FIRST to prevent next_offset corruption
        // When clear_all() is called, the properties.store file still contains old data
        // If PropertyStore is recreated later, rebuild_index() will read old data and set
        // next_offset incorrectly, causing new properties to overwrite old ones
        self.property_store.write().unwrap().clear_all()?;

        // CRITICAL FIX: Drop memory mappings before truncating files
        // On Windows, you cannot truncate a file that has a memory-mapped section open
        // Create temporary empty files to replace the mappings
        let temp_dir = tempfile::tempdir()?;
        let temp_nodes_path = temp_dir.path().join("nodes.tmp");
        let temp_rels_path = temp_dir.path().join("rels.tmp");

        // Create temporary empty files and keep them open
        let mut temp_nodes_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&temp_nodes_path)?;
        temp_nodes_file.set_len(INITIAL_NODES_FILE_SIZE as u64)?;
        let temp_nodes_mmap = unsafe { MmapOptions::new().map_mut(&temp_nodes_file)? };

        let mut temp_rels_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&temp_rels_path)?;
        temp_rels_file.set_len(INITIAL_RELS_FILE_SIZE as u64)?;
        let temp_rels_mmap = unsafe { MmapOptions::new().map_mut(&temp_rels_file)? };

        // Replace old mappings with temporary ones (drops old mappings) inside
        // the shared Arc<RwLock> so every clone sees the reset. Assigning into
        // the guard drops the previous mapping, releasing the original files.
        *self.nodes_mmap.write().unwrap() = temp_nodes_mmap;
        *self.rels_mmap.write().unwrap() = temp_rels_mmap;

        // Now we can truncate the original files (mappings are closed)
        self.nodes_file.set_len(INITIAL_NODES_FILE_SIZE as u64)?;
        self.rels_file.set_len(INITIAL_RELS_FILE_SIZE as u64)?;

        // Zero out the files
        use std::io::Write;
        self.nodes_file
            .write_all(&vec![0u8; INITIAL_NODES_FILE_SIZE])?;
        self.rels_file
            .write_all(&vec![0u8; INITIAL_RELS_FILE_SIZE])?;
        self.nodes_file.sync_all()?;
        self.rels_file.sync_all()?;

        // Update file sizes
        self.nodes_file_size = INITIAL_NODES_FILE_SIZE;
        self.rels_file_size = INITIAL_RELS_FILE_SIZE;

        // Recreate memory mappings from original files (in the shared lock).
        *self.nodes_mmap.write().unwrap() =
            unsafe { MmapOptions::new().map_mut(&*self.nodes_file)? };
        *self.rels_mmap.write().unwrap() = unsafe { MmapOptions::new().map_mut(&*self.rels_file)? };

        // Drop temporary files and mappings (temp_dir will be dropped at end of scope)
        drop(temp_nodes_file);
        drop(temp_rels_file);

        tracing::debug!("[RecordStore::clear_all] Storage cleared successfully");
        Ok(())
    }
}
