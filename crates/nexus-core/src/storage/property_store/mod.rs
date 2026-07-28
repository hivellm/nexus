//! Property storage system for Nexus graph database
//!
//! This module provides efficient storage and retrieval of node and relationship properties
//! using a key-value store with JSON serialization.

use crate::error::{Error, Result};
use memmap2::{MmapMut, MmapOptions};
use serde_json;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::PathBuf;

mod crud;
mod io;
mod scan;

#[cfg(test)]
mod tests;

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

/// Byte size of a property entry's on-disk header:
/// `entity_id: u64` (8) + `entity_type: u8` (1) + `data_size: u32` (4).
const PROPERTY_ENTRY_HEADER_SIZE: u64 = 13;

/// Reserved `entity_type` header byte marking a property entry as
/// tombstoned (logically deleted). Distinct from every valid
/// [`EntityType`] discriminant (`0` = Node, `1` = Relationship), so
/// [`EntityType::from_u8`] always rejects it and the shared rebuild
/// scanner ([`PropertyStore::scan_entry_at`]) can recognise it before
/// attempting to parse the entry as live data.
///
/// Written by [`PropertyStore::write_tombstone`], called from both
/// [`PropertyStore::delete_properties`] (forward fix) and the rebuild
/// scanner's back-compat reconciliation against the authoritative record
/// store (see [`RecordLiveness`]). The `data_size` header field and the
/// payload it describes are left untouched, so a tombstoned entry's
/// on-disk footprint — needed to stride over it correctly — never
/// changes. See phase0_fix-deleted-properties-resurrected-on-rebuild.
const ENTITY_TYPE_TOMBSTONE: u8 = 0xFF;

/// A successfully parsed property-entry header at some on-disk offset.
/// See [`PropertyStore::try_parse_entry`].
struct PropertyEntryHeader {
    /// Byte offset of this entry's header within the property file.
    offset: u64,
    entity_id: u64,
    entity_type: EntityType,
    /// Total footprint of this entry (header + payload), in bytes.
    entry_size: u64,
}

/// Result of classifying the property entry at a given offset during an
/// index-rebuild scan. See [`PropertyStore::scan_entry_at`].
enum PropertyScanStep {
    /// A live, successfully parsed entry (possibly found via resync).
    Entry(PropertyEntryHeader),
    /// Dead space that must be strided over but never indexed: either an
    /// entry tombstoned by [`PropertyStore::delete_properties`], or a
    /// pre-fix, un-tombstoned entry whose owning record was reconciled
    /// as deleted/absent against the authoritative record store (and has
    /// just been tombstoned in place so future scans skip the
    /// reconciliation check).
    Dead {
        /// Total on-disk footprint of the dead entry, in bytes.
        entry_size: u64,
    },
    /// A never-written, zeroed header — the legitimate end of live entries.
    End,
    /// The header at the scanned offset did not parse, and no later valid
    /// header could be resynced to before the scan's `limit`.
    Unrecoverable,
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
