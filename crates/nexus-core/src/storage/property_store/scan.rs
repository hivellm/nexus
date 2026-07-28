use super::*;
use crate::storage::records::{NODE_RECORD_SIZE, NodeRecord, REL_RECORD_SIZE, RelationshipRecord};
use memmap2::{Mmap, MmapOptions};
use std::fs::File;
use std::path::Path;

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
    pub(super) fn rebuild_index(&mut self) -> Result<()> {
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
    pub(super) fn ensure_capacity(&mut self, required_size: u64) -> Result<()> {
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
}
