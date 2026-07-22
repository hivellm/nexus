//! Property storage system for Nexus graph database
//!
//! This module provides efficient storage and retrieval of node and relationship properties
//! using a key-value store with JSON serialization.

use crate::error::{Error, Result};
use memmap2::{Mmap, MmapMut, MmapOptions};
use serde_json;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing;

use super::records::{NODE_RECORD_SIZE, NodeRecord, REL_RECORD_SIZE, RelationshipRecord};

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

/// Read-only, best-effort view of the authoritative `nodes.store` /
/// `rels.store` record files living beside this property store.
///
/// Used only to reconcile legacy (pre-tombstone) deleted entities during
/// an index-rebuild scan: phase0_fix-deleted-properties-resurrected-on-rebuild
/// §2.2. A property store used standalone — no sibling record files, e.g.
/// this module's own unit tests — has nothing to reconcile against, so
/// [`RecordLiveness::is_live`] trusts the parsed entry in that case,
/// preserving pre-fix behavior.
///
/// Only the `is_deleted` flag bit is consulted, never `is_allocated`:
/// `RecordStore::new` stamps the allocated bit on legacy records in a
/// migration pass that runs *after* `PropertyStore::new` (and therefore
/// after the first rebuild scan), so `is_allocated` cannot be trusted
/// here. `is_deleted` predates that migration and is unaffected by it.
struct RecordLiveness {
    nodes: Option<Mmap>,
    rels: Option<Mmap>,
}

impl RecordLiveness {
    /// Open read-only mappings of `nodes.store` / `rels.store` in `dir`,
    /// if present. Missing files (or files that fail to map) simply
    /// disable reconciliation for that entity type — see
    /// [`RecordLiveness::is_live`].
    fn open(dir: &Path) -> Self {
        let nodes = File::open(dir.join("nodes.store"))
            .ok()
            .and_then(|f| unsafe { MmapOptions::new().map(&f) }.ok());
        let rels = File::open(dir.join("rels.store"))
            .ok()
            .and_then(|f| unsafe { MmapOptions::new().map(&f) }.ok());
        Self { nodes, rels }
    }

    /// `true` unless the owning record is provably deleted or its slot
    /// does not exist in the record store — the "deleted or absent" test
    /// from phase0_fix-deleted-properties-resurrected-on-rebuild §2.2.
    fn is_live(&self, entity_id: u64, entity_type: EntityType) -> bool {
        let (mmap, record_size) = match entity_type {
            EntityType::Node => (self.nodes.as_deref(), NODE_RECORD_SIZE),
            EntityType::Relationship => (self.rels.as_deref(), REL_RECORD_SIZE),
        };
        let Some(mmap) = mmap else {
            // No sibling record store to reconcile against.
            return true;
        };

        let bounds = (entity_id as usize)
            .checked_mul(record_size)
            .and_then(|start| Some(start).zip(start.checked_add(record_size)));
        let Some((start, end)) = bounds else {
            // Overflowed the address space -- not a real record slot.
            return false;
        };
        if end > mmap.len() {
            // No slot for this id at all -- the "absent" half of §2.2.
            return false;
        }

        match entity_type {
            EntityType::Node => {
                let record: NodeRecord = *bytemuck::from_bytes(&mmap[start..end]);
                !record.is_deleted()
            }
            EntityType::Relationship => {
                let record: RelationshipRecord = *bytemuck::from_bytes(&mmap[start..end]);
                !record.is_deleted()
            }
        }
    }
}

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

    /// A successfully parsed property-entry header, produced by
    /// [`PropertyStore::try_parse_entry`] and consumed by the index-rebuild
    /// scanners ([`PropertyStore::rebuild_index`],
    /// [`PropertyStore::ensure_index_populated`]).
    ///
    /// Kept as a private implementation detail shared by both scanners so
    /// their stride and back-compat resync logic (see
    /// [`PropertyStore::resync_to_next_entry`]) cannot diverge from each
    /// other — phase0_fix-property-store-shrink-corruption §3.2/§3.3.
    fn try_parse_entry(&self, offset: u64, limit: u64) -> Option<PropertyEntryHeader> {
        if offset + PROPERTY_ENTRY_HEADER_SIZE > limit {
            return None;
        }

        let entity_id = self.read_u64(offset);
        let entity_type_byte = self.read_u8(offset + 8);
        let data_size = self.read_u32(offset + 9);

        let entity_type = EntityType::from_u8(entity_type_byte).ok()?;

        let entry_size = PROPERTY_ENTRY_HEADER_SIZE + data_size as u64;
        if offset + entry_size > limit {
            return None;
        }

        // §2.2 back-compat: validating that the payload bytes deserialize
        // as JSON is what lets the resync scan tell a genuine header apart
        // from a false-positive match inside a pre-fix in-place shrink's
        // stale, unzeroed tail (arbitrary JSON text bytes only rarely line
        // up with a valid `EntityType` byte AND a `data_size` that stays in
        // bounds AND happen to be followed by more valid JSON).
        let data_start = (offset + PROPERTY_ENTRY_HEADER_SIZE) as usize;
        let data_end = (offset + entry_size) as usize;
        serde_json::from_slice::<serde_json::Value>(&self.mmap[data_start..data_end]).ok()?;

        Some(PropertyEntryHeader {
            offset,
            entity_id,
            entity_type,
            entry_size,
        })
    }

    /// Scan forward, byte by byte, from `start` for the next offset at
    /// which [`PropertyStore::try_parse_entry`] succeeds.
    ///
    /// Used by [`PropertyStore::scan_entry_at`] to resync after landing on
    /// an unparseable header — the symptom of striding into a pre-fix
    /// in-place shrink's stale tail — instead of dropping every entity that
    /// follows. Returns `None` if no valid header is found before `limit`,
    /// which the caller treats as the (accepted) unrecoverable-tail case:
    /// phase0_fix-property-store-shrink-corruption §2.2's caveat that an
    /// entry whose header was already overwritten by a pre-fix mis-scan
    /// write cannot be recovered.
    fn resync_to_next_entry(&self, start: u64, limit: u64) -> Option<PropertyEntryHeader> {
        let mut candidate = start;
        while candidate < limit {
            if let Some(parsed) = self.try_parse_entry(candidate, limit) {
                return Some(parsed);
            }
            candidate += 1;
        }
        None
    }

    /// Classify the property entry at `offset` for an index-rebuild scan
    /// bounded by `limit`, reconciled against `liveness`.
    ///
    /// Shared by [`PropertyStore::rebuild_index`] and
    /// [`PropertyStore::ensure_index_populated`] so the two scanners cannot
    /// diverge (phase0_fix-property-store-shrink-corruption §3.2/§3.3).
    fn scan_entry_at(
        &mut self,
        offset: u64,
        limit: u64,
        liveness: &RecordLiveness,
    ) -> PropertyScanStep {
        if offset + PROPERTY_ENTRY_HEADER_SIZE > limit {
            return PropertyScanStep::End;
        }

        let entity_id = self.read_u64(offset);
        let entity_type_byte = self.read_u8(offset + 8);
        let data_size = self.read_u32(offset + 9);

        // A never-written, zeroed header is the legitimate end of the live
        // entries — not corruption. This check must run BEFORE attempting a
        // resync, otherwise every fresh store would pay for a byte-by-byte
        // scan of its entire pre-allocated (zeroed) capacity.
        if entity_id == 0 && entity_type_byte == 0 && data_size == 0 {
            return PropertyScanStep::End;
        }

        // phase0_fix-deleted-properties-resurrected-on-rebuild §3.2: an
        // entry tombstoned by `delete_properties` carries the reserved
        // marker byte. `data_size` was never rewritten by the delete, so
        // it still describes this entry's true footprint and we can
        // stride over the dead payload without re-indexing it.
        if entity_type_byte == ENTITY_TYPE_TOMBSTONE {
            let entry_size = PROPERTY_ENTRY_HEADER_SIZE + data_size as u64;
            if offset + entry_size <= limit {
                return PropertyScanStep::Dead { entry_size };
            }
            // Bounds look wrong for a genuine tombstone (corruption) —
            // fall through to the normal parse/resync path below.
        }

        if let Some(parsed) = self.try_parse_entry(offset, limit) {
            return self.reconcile_parsed_entry(parsed, liveness);
        }

        match self.resync_to_next_entry(offset + 1, limit) {
            Some(parsed) => self.reconcile_parsed_entry(parsed, liveness),
            None => PropertyScanStep::Unrecoverable,
        }
    }

    /// §2.2 back-compat: a pre-fix store may hold a deleted entity that was
    /// never tombstoned (`delete_properties` only cleared the in-memory
    /// index before phase0_fix-deleted-properties-resurrected-on-rebuild).
    /// Reconcile a successfully parsed entry against the authoritative
    /// record store: if the owning node/relationship record is deleted or
    /// its slot doesn't exist, this entry must not be resurrected into the
    /// index. Tombstone it now so later reopens don't pay the
    /// reconciliation cost again.
    fn reconcile_parsed_entry(
        &mut self,
        parsed: PropertyEntryHeader,
        liveness: &RecordLiveness,
    ) -> PropertyScanStep {
        if liveness.is_live(parsed.entity_id, parsed.entity_type) {
            return PropertyScanStep::Entry(parsed);
        }
        let entry_size = parsed.entry_size;
        self.write_tombstone(parsed.offset);
        PropertyScanStep::Dead { entry_size }
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

            // Scan file to rebuild indexes, but don't update next_offset.
            // Entries start at offset 1 (offset 0 is the reserved sentinel);
            // scanning from 0 would misalign every read. Uses the shared
            // `scan_entry_at` classifier so this preserved-range scan
            // cannot diverge from `rebuild_index`'s full scan or from
            // `ensure_index_populated` (phase0_fix-property-store-shrink-corruption §3.2/§3.3).
            let liveness = RecordLiveness::open(&self.path);
            let mut offset = 1;
            loop {
                match self.scan_entry_at(offset, preserved_next_offset, &liveness) {
                    PropertyScanStep::Entry(parsed) => {
                        self.index
                            .insert(parsed.offset, (parsed.entity_id, parsed.entity_type));
                        self.reverse_index
                            .insert((parsed.entity_id, parsed.entity_type), parsed.offset);
                        offset = parsed.offset + parsed.entry_size;
                    }
                    PropertyScanStep::Dead { entry_size } => {
                        offset += entry_size;
                    }
                    PropertyScanStep::End | PropertyScanStep::Unrecoverable => break,
                }
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
        // This helps detect if we're reading old data that shouldn't be used.
        // Entries start at offset 1 (offset 0 is the reserved sentinel because
        // prop_ptr=0 means "no properties"); scanning from 0 misaligns every
        // read and fabricates a phantom (0, Node) entry (issue #4).
        //
        // phase0_fix-property-store-shrink-corruption §3.2: `max_valid_offset`
        // — the end of the LAST successfully parsed entry — is what
        // `next_offset` is derived from below, never the raw scan cursor.
        // Before this fix, an invalid `EntityType` byte (the pre-fix
        // in-place-shrink stale-tail symptom) `break`-ed straight to
        // `self.next_offset = offset`, landing `next_offset` mid-garbage —
        // the exact corruption this task closes. `scan_entry_at` now also
        // resyncs forward past a stale tail to recover later entities
        // instead of dropping them (§2.2 back-compat).
        let mmap_len = self.mmap.len() as u64;
        let liveness = RecordLiveness::open(&self.path);
        let mut offset = 1;
        let mut max_valid_offset = 0;
        let mut found_valid_entries = false;

        loop {
            match self.scan_entry_at(offset, mmap_len, &liveness) {
                PropertyScanStep::Entry(parsed) => {
                    self.index
                        .insert(parsed.offset, (parsed.entity_id, parsed.entity_type));
                    self.reverse_index
                        .insert((parsed.entity_id, parsed.entity_type), parsed.offset);

                    found_valid_entries = true;
                    max_valid_offset = parsed.offset + parsed.entry_size;
                    offset = max_valid_offset;
                }
                PropertyScanStep::Dead { entry_size } => {
                    // Dead space (tombstoned, or just-reconciled orphan)
                    // still occupies its footprint on disk, so it must
                    // still advance next_offset — only indexing is
                    // skipped.
                    found_valid_entries = true;
                    max_valid_offset = offset + entry_size;
                    offset = max_valid_offset;
                }
                PropertyScanStep::End => {
                    tracing::debug!(
                        "[rebuild_index] Found empty entry at offset={}, found_valid_entries={}, max_valid_offset={}",
                        offset,
                        found_valid_entries,
                        max_valid_offset
                    );
                    break;
                }
                PropertyScanStep::Unrecoverable => {
                    tracing::debug!(
                        "[rebuild_index] Unrecoverable gap at offset={} (no parseable header before end of file); \
                         stopping scan, found_valid_entries={}, max_valid_offset={}",
                        offset,
                        found_valid_entries,
                        max_valid_offset
                    );
                    break;
                }
            }
        }

        // CRITICAL FIX: Only update next_offset if we found valid entries
        // If the file contains only old data (from previous runs), don't use it
        // This prevents rebuild_index from resetting next_offset to old values
        if found_valid_entries {
            self.next_offset = max_valid_offset;
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

    /// Overwrite the `entity_type` header byte of the property entry
    /// starting at `entry_offset` with [`ENTITY_TYPE_TOMBSTONE`], so the
    /// shared rebuild scanner ([`PropertyStore::scan_entry_at`]) strides
    /// over it without re-indexing it. The `data_size` field (and the
    /// payload it describes) is left untouched, so the entry's on-disk
    /// footprint never changes. See
    /// phase0_fix-deleted-properties-resurrected-on-rebuild.
    fn write_tombstone(&mut self, entry_offset: u64) {
        self.write_u8(entry_offset + 8, ENTITY_TYPE_TOMBSTONE);
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

    /// Return the byte-offset stored in the reverse index for `(entity_id, entity_type)`.
    ///
    /// This is the canonical way to recover a valid `prop_ptr` for a node (or
    /// relationship) when the on-disk record's pointer is known to be corrupt.
    /// The index must be populated (via [`PropertyStore::ensure_index_populated`])
    /// before calling this.
    pub fn offset_for(&self, entity_id: u64, entity_type: EntityType) -> Option<u64> {
        self.reverse_index.get(&(entity_id, entity_type)).copied()
    }

    /// Ensure the in-memory index is populated by scanning the property file.
    ///
    /// Called by [`RecordStore::repair_corrupt_node_prop_ptrs`] at startup,
    /// before it needs to look up `offset_for` entries.  The normal
    /// `rebuild_index` path skips the full scan when `next_offset` is already
    /// set (i.e. on every fresh open of an existing store), which means the
    /// reverse_index is initially empty.  This method forces a full scan so
    /// the repair has a complete map from `(entity_id, entity_type)` → offset.
    ///
    /// If the indexes are already populated, this is a no-op.
    pub fn ensure_index_populated(&mut self) -> Result<()> {
        if !self.index.is_empty() {
            // Already populated — nothing to do.
            return Ok(());
        }

        // Full scan: start at offset 1 (offset 0 is always zero because
        // prop_ptr=0 means "no properties"). Uses the shared `scan_entry_at`
        // classifier so this scanner is IDENTICAL to `rebuild_index`'s full
        // scan and cannot diverge from it
        // (phase0_fix-property-store-shrink-corruption §3.2/§3.3): both
        // stride by the parsed entry's true footprint and resync forward
        // past an unparseable (stale-tail) header instead of dropping every
        // later entity.
        let mut offset: u64 = 1;
        let mmap_len = self.mmap.len() as u64;
        let liveness = RecordLiveness::open(&self.path);
        let mut found_next_offset: u64 = 1;

        loop {
            match self.scan_entry_at(offset, mmap_len, &liveness) {
                PropertyScanStep::Entry(parsed) => {
                    self.index
                        .insert(parsed.offset, (parsed.entity_id, parsed.entity_type));
                    self.reverse_index
                        .insert((parsed.entity_id, parsed.entity_type), parsed.offset);

                    found_next_offset = parsed.offset + parsed.entry_size;
                    offset = found_next_offset;
                }
                PropertyScanStep::Dead { entry_size } => {
                    found_next_offset = offset + entry_size;
                    offset = found_next_offset;
                }
                PropertyScanStep::End | PropertyScanStep::Unrecoverable => break,
            }
        }

        // Advance next_offset to the end of the last valid entry so that new
        // properties are appended correctly.
        if found_next_offset > self.next_offset {
            self.next_offset = found_next_offset;
        }

        Ok(())
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
    use crate::testing::TestContext;
    use serde_json::json;
    use std::io::{Seek, SeekFrom};
    use std::sync::{Arc, RwLock};

    #[test]
    fn test_property_store_creation() {
        let ctx = TestContext::new();
        let store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();
        assert_eq!(store.property_count(), 0);
    }

    #[test]
    fn test_store_and_load_properties() {
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

        let properties = json!({
            "name": "Alice",
            "age": 30,
            "active": true
        });

        let ptr = store
            .store_properties(1, EntityType::Node, properties.clone())
            .unwrap();
        // First property should be at offset 1 (not 0, because prop_ptr=0 means "no properties")
        assert!(
            ptr == 1,
            "First property should be at offset 1, got {}",
            ptr
        );

        let loaded = store.load_properties(1, EntityType::Node).unwrap().unwrap();
        assert_eq!(loaded, properties);
    }

    #[test]
    fn test_update_properties() {
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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

    /// phase0_fix-deleted-properties-resurrected-on-rebuild §1.1: a deleted
    /// property must stay deleted after the store is dropped and reopened
    /// (reopen drives `PropertyStore::new` -> `rebuild_index`, the same
    /// path a server restart takes). Before the fix, `delete_properties`
    /// only cleared the in-memory index, so the rebuild scan re-parsed the
    /// still-intact on-disk bytes and resurrected the property.
    #[test]
    fn test_deleted_properties_do_not_resurrect_on_reopen() {
        let ctx = TestContext::new();
        let dir = ctx.path().to_path_buf();

        {
            let mut store = PropertyStore::new(dir.clone()).unwrap();
            store
                .store_properties(1, EntityType::Node, json!({"secret": "x"}))
                .unwrap();
            store.delete_properties(1, EntityType::Node).unwrap();
            store.flush().unwrap();
        }

        let reopened = PropertyStore::new(dir).unwrap();
        assert!(
            reopened
                .load_properties(1, EntityType::Node)
                .unwrap()
                .is_none(),
            "deleted property resurrected after reopen"
        );
    }

    /// phase0_fix-deleted-properties-resurrected-on-rebuild §4.2: a
    /// deleted entity must stay deleted even when live neighbours are
    /// interleaved with it on disk, and the scanner must still stride
    /// correctly past the dead entry to find them.
    #[test]
    fn test_deleted_properties_do_not_resurrect_among_live_neighbours() {
        let ctx = TestContext::new();
        let dir = ctx.path().to_path_buf();

        {
            let mut store = PropertyStore::new(dir.clone()).unwrap();
            store
                .store_properties(1, EntityType::Node, json!({"secret": "x"}))
                .unwrap();
            store
                .store_properties(2, EntityType::Node, json!({"name": "Bob"}))
                .unwrap();
            store.delete_properties(1, EntityType::Node).unwrap();
            store.flush().unwrap();
        }

        let reopened = PropertyStore::new(dir).unwrap();
        assert!(
            reopened
                .load_properties(1, EntityType::Node)
                .unwrap()
                .is_none(),
            "deleted property resurrected after reopen"
        );
        assert_eq!(
            reopened
                .load_properties(2, EntityType::Node)
                .unwrap()
                .unwrap(),
            json!({"name": "Bob"}),
            "live neighbour was lost or corrupted by the tombstone scan"
        );
    }

    #[test]
    fn test_relationship_properties() {
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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
        let ctx = TestContext::new();
        let store = Arc::new(RwLock::new(
            PropertyStore::new(ctx.path().to_path_buf()).unwrap(),
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
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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
        let ctx = TestContext::new();
        let store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

        // Health check should pass for valid store
        store.health_check().unwrap();

        // Test property count
        assert_eq!(store.property_count(), 0);
    }

    #[test]
    fn test_property_store_error_handling() {
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

        // Test loading non-existent property
        let result = store.load_properties(999, EntityType::Node).unwrap();
        assert!(result.is_none());

        // Test deleting non-existent property (should not error)
        store.delete_properties(999, EntityType::Node).unwrap();
    }

    #[test]
    fn test_property_store_serialization_types() {
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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
        let ctx = TestContext::new();
        let mut store = PropertyStore::new(ctx.path().to_path_buf()).unwrap();

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

    /// Edge case: re-adding properties for an entity after they were
    /// deleted must not resurrect the OLD (tombstoned) value, and must
    /// survive a reopen with the NEW value.
    #[test]
    fn test_store_after_delete_reuses_entity_with_new_value() {
        let ctx = TestContext::new();
        let dir = ctx.path().to_path_buf();

        {
            let mut store = PropertyStore::new(dir.clone()).unwrap();
            store
                .store_properties(1, EntityType::Node, json!({"name": "Alice"}))
                .unwrap();
            store.delete_properties(1, EntityType::Node).unwrap();
            store
                .store_properties(1, EntityType::Node, json!({"name": "Bob"}))
                .unwrap();
            store.flush().unwrap();
        }

        let reopened = PropertyStore::new(dir).unwrap();
        assert_eq!(
            reopened
                .load_properties(1, EntityType::Node)
                .unwrap()
                .unwrap(),
            json!({"name": "Bob"}),
            "re-added property was lost, or the stale deleted value resurrected"
        );
    }

    /// Edge case: deleting properties for an entity that was never stored
    /// must not error and must not tombstone unrelated bytes.
    #[test]
    fn test_delete_properties_for_nonexistent_entity_is_a_noop() {
        let ctx = TestContext::new();
        let dir = ctx.path().to_path_buf();

        let mut store = PropertyStore::new(dir).unwrap();
        store
            .store_properties(1, EntityType::Node, json!({"name": "Alice"}))
            .unwrap();

        // Deleting an entity that was never stored must succeed silently.
        store.delete_properties(999, EntityType::Node).unwrap();

        // The unrelated, still-live entity must be unaffected.
        assert_eq!(
            store.load_properties(1, EntityType::Node).unwrap().unwrap(),
            json!({"name": "Alice"})
        );
    }

    /// phase0_fix-deleted-properties-resurrected-on-rebuild §2.2 back-compat:
    /// a pre-fix store may hold a deleted entity that was never tombstoned
    /// (only its owning node record was marked deleted). Reopening must
    /// reconcile against the record store and not resurrect it, while a
    /// live neighbour with no record store entry (no reconciliation data)
    /// is trusted as before.
    #[test]
    fn test_back_compat_reconciles_untombstoned_deleted_entity_against_record_store() {
        let ctx = TestContext::new();
        let dir = ctx.path().to_path_buf();

        {
            let mut store = PropertyStore::new(dir.clone()).unwrap();
            store
                .store_properties(1, EntityType::Node, json!({"secret": "x"}))
                .unwrap();
            store
                .store_properties(2, EntityType::Node, json!({"name": "Bob"}))
                .unwrap();
            // Simulate the pre-fix delete path: entity 1's property blob is
            // left fully parseable on disk (no tombstone).
            store.flush().unwrap();
        }

        // Simulate node 1 having been deleted at the record-store level
        // (the authoritative signal a pre-fix property store had no way to
        // record itself). Node 2's slot is present but not deleted, to
        // confirm reconciliation only drops the deleted entity.
        let nodes_path = dir.join("nodes.store");
        let mut deleted_record = NodeRecord::new();
        deleted_record.mark_deleted();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&nodes_path)
            .unwrap();
        file.set_len(3 * NODE_RECORD_SIZE as u64).unwrap();
        file.seek(SeekFrom::Start(NODE_RECORD_SIZE as u64)).unwrap();
        file.write_all(bytemuck::bytes_of(&deleted_record)).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let reopened = PropertyStore::new(dir).unwrap();
        assert!(
            reopened
                .load_properties(1, EntityType::Node)
                .unwrap()
                .is_none(),
            "pre-fix, un-tombstoned deleted entity resurrected after reopen"
        );
        assert_eq!(
            reopened
                .load_properties(2, EntityType::Node)
                .unwrap()
                .unwrap(),
            json!({"name": "Bob"}),
            "live neighbour was wrongly reconciled away as deleted"
        );
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
