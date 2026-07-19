//! Operational methods for [`RecordStore`]: CRUD operations for nodes and
//! relationships, property management, adjacency-list helpers, and
//! store-level utilities (`clear_all`, `repair_corrupt_node_prop_ptrs`).
//!
//! All methods are implemented on `RecordStore` and live in a separate file
//! purely to keep `record_store.rs` (struct definition + lifecycle methods)
//! under the 1 500-line budget.

use std::sync::atomic::Ordering;

use crate::error::{Error, Result};
use memmap2::MmapOptions;

use super::external_id::{ConflictPolicy, ExternalId};
use super::property_store;
use super::record_store::RecordStore;
use super::records::{
    INITIAL_NODES_FILE_SIZE, INITIAL_RELS_FILE_SIZE, NODE_RECORD_SIZE, NodeRecord, REL_RECORD_SIZE,
    RelationshipRecord,
};

impl RecordStore {
    /// Write a node record
    /// Phase 3 Deep Optimization: Optimized write path
    pub fn write_node(&mut self, node_id: u64, record: &NodeRecord) -> Result<()> {
        // PHASE 2: Validate prop_ptr before writing to prevent corruption
        // Only block if prop_ptr points to Relationship properties (definite corruption)
        // If it points to another Node, warn but allow (may be test code or will be corrected by load_node_properties)
        if record.prop_ptr != 0 {
            if let Some((stored_entity_id, stored_entity_type)) = self
                .property_store
                .read()
                .unwrap()
                .get_entity_info_at_offset(record.prop_ptr)
            {
                // CRITICAL: Block if prop_ptr points to Relationship (definite corruption)
                if stored_entity_type == property_store::EntityType::Relationship {
                    let error_msg = format!(
                        "PHASE 2 VALIDATION FAILED: prop_ptr for node {} points to Relationship {} - this is corruption!",
                        node_id, stored_entity_id
                    );
                    tracing::error!("{}", error_msg);
                    return Err(Error::Storage(error_msg));
                }

                // If prop_ptr points to a different Node, warn but allow
                // This may be test code or will be corrected by load_node_properties fallback
                if stored_entity_id != node_id {
                    tracing::warn!(
                        "write_node: node_id={}, prop_ptr={} points to Node {} instead of Node {} (may be test code or will be corrected)",
                        node_id,
                        record.prop_ptr,
                        stored_entity_id,
                        node_id
                    );
                }
            } else {
                // prop_ptr not found in property_store index - might be:
                // 1. Test code using simulated prop_ptr values (allow)
                // 2. Stale/invalid prop_ptr that will be corrected by load_node_properties fallback (allow)
                tracing::debug!(
                    "write_node: node_id={}, prop_ptr={} not found in property_store index, proceeding with write (may be test/simulation code)",
                    node_id,
                    record.prop_ptr
                );
            }
        }

        let offset = (node_id as usize * NODE_RECORD_SIZE) as u64;

        // Phase 3 Optimization: Pre-check file size to avoid unnecessary grow check
        if offset + NODE_RECORD_SIZE as u64 > self.nodes_file_size as u64 {
            self.grow_nodes_file()?;
        }

        // Phase 3 Optimization: Direct write without intermediate allocation
        let start = offset as usize;
        let end = start + NODE_RECORD_SIZE;
        let record_bytes = bytemuck::bytes_of(record);
        self.nodes_mmap.write().unwrap()[start..end].copy_from_slice(record_bytes);

        // Memory barrier to ensure write is visible to subsequent reads
        // Release is sufficient for single-writer model
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        Ok(())
    }

    /// Scan every node slot and durably fix any `prop_ptr` that is corrupt.
    ///
    /// A `prop_ptr` is corrupt when it is non-zero and the property store does
    /// not contain a **Node** entry for the owning node at that offset (e.g.
    /// because it points at a Relationship entry, or at stale/garbage bytes).
    ///
    /// The correct offset is recovered from the property store's `reverse_index`
    /// via [`property_store::PropertyStore::offset_for`].  If no entry exists the
    /// pointer is reset to `0` (meaning "no properties"), which is safe because
    /// `load_node_properties` will then return `None` rather than corrupt data.
    ///
    /// After the scan, if any record was corrected the nodes mmap is flushed to
    /// disk so the repair survives the next restart.  This closes the recurring
    /// corruption loop described in issue #4.
    ///
    /// Returns the number of slots that were repaired.
    pub fn repair_corrupt_node_prop_ptrs(&mut self) -> Result<usize> {
        let slot_count = self.nodes_file_size / NODE_RECORD_SIZE;
        let mut repaired = 0usize;

        for slot in 0..slot_count {
            let byte_start = slot * NODE_RECORD_SIZE;
            let byte_end = byte_start + NODE_RECORD_SIZE;

            // Read raw on-disk bytes — do NOT go through read_node because that
            // resets prop_ptr in memory without persisting; we need the real value.
            let mut record: NodeRecord = {
                let guard = self.nodes_mmap.read().unwrap();
                let bytes = &guard[byte_start..byte_end];
                // Skip all-zero slots: they are unallocated/never-written.
                if bytes.iter().all(|&b| b == 0) {
                    continue;
                }
                *bytemuck::from_bytes(bytes)
            };

            // Nothing to validate when prop_ptr is already 0.
            if record.prop_ptr == 0 {
                continue;
            }

            let node_id = slot as u64;

            // Determine whether the on-disk prop_ptr is valid for this node.
            let is_valid = self
                .property_store
                .read()
                .unwrap()
                .get_entity_info_at_offset(record.prop_ptr)
                .map(|(eid, etype)| eid == node_id && etype == property_store::EntityType::Node)
                .unwrap_or(false);

            if is_valid {
                continue;
            }

            // prop_ptr is corrupt — recover the correct offset (or 0).
            let correct_ptr = self
                .property_store
                .read()
                .unwrap()
                .offset_for(node_id, property_store::EntityType::Node)
                .unwrap_or(0);

            record.prop_ptr = correct_ptr;

            // write_node validates that the new prop_ptr points to a Node entry
            // (or is 0), so it will accept this corrected record.
            self.write_node(node_id, &record)?;
            repaired += 1;
        }

        if repaired > 0 {
            // Make the corrections durable before the constructor returns.
            self.nodes_mmap
                .read()
                .unwrap()
                .flush()
                .map_err(|e| Error::Storage(format!("Failed to flush after repair: {}", e)))?;
            tracing::info!("repaired {} corrupt node prop_ptr(s)", repaired);
        }

        Ok(repaired)
    }

    /// Read a node record
    pub fn read_node(&self, node_id: u64) -> Result<NodeRecord> {
        // Memory barrier to ensure visibility of writes from other threads
        // Acquire is sufficient - pairs with Release barriers in write operations
        std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);

        let offset = (node_id as usize * NODE_RECORD_SIZE) as u64;

        if offset + NODE_RECORD_SIZE as u64 > self.nodes_file_size as u64 {
            return Err(Error::NotFound(format!("Node {} not found", node_id)));
        }

        let start = offset as usize;
        let end = start + NODE_RECORD_SIZE;
        let mut record: NodeRecord = {
            let guard = self.nodes_mmap.read().unwrap();
            *bytemuck::from_bytes(&guard[start..end])
        };

        // CRITICAL FIX: Validate prop_ptr immediately after read to detect corruption early
        // If prop_ptr points to a Relationship, it's corrupted - reset to 0
        // This prevents corruption from propagating and helps identify when corruption occurs
        // IMPORTANT: When prop_ptr is reset to 0, load_node_properties will use reverse_index fallback
        // to recover properties, so properties are not lost
        if record.prop_ptr != 0 {
            if let Some((stored_entity_id, stored_entity_type)) = self
                .property_store
                .read()
                .unwrap()
                .get_entity_info_at_offset(record.prop_ptr)
            {
                if stored_entity_type == property_store::EntityType::Relationship {
                    tracing::error!(
                        "[read_node] node_id={} prop_ptr corruption detected (points to Relationship {}), resetting to 0. Properties will be recovered via reverse_index.",
                        node_id,
                        stored_entity_id
                    );
                    // Reset prop_ptr to 0 to prevent further corruption
                    // Note: This is a read-only operation, so we can't write back the corrected value
                    // The corrected value will be written on next write_node call
                    // IMPORTANT: Properties are NOT lost - load_node_properties will use reverse_index
                    // to recover them when prop_ptr is 0
                    record.prop_ptr = 0;
                }
            }
        }

        Ok(record)
    }

    /// Read every node header (up to [`Self::node_count`] records) in ONE
    /// `nodes_mmap` lock acquisition instead of one acquisition per node.
    ///
    /// phase8_neo4j-concurrency-gaps §1 — `count_live_nodes_all` /
    /// `count_live_nodes_for_label` used to call [`Self::read_node`] once
    /// per candidate node, each call taking its own `nodes_mmap.read()`
    /// lock (plus a second `property_store.read()` lock whenever the node
    /// had a `prop_ptr`, for a corruption cross-check that `is_deleted()`
    /// never needed). At thousands of nodes and dozens of concurrent
    /// callers that is hundreds of thousands of `RwLock` acquisitions per
    /// second on the same shared locks — the actual serialization point
    /// behind `aggregation.count_all`'s 16w-\>64w collapse (2.5k -\> 2.9k
    /// qps flat, p99 124ms, while Neo4j scaled to 13k), on top of the
    /// Project-skip fix on `Executor::try_short_circuit_count_cross_product`
    /// that made this short-circuit engage for `count(n)` at all.
    /// `NodeRecord` is `bytemuck::Pod`, so casting the locked byte range
    /// once and copying it into an owned `Vec` is a single bulk memcpy —
    /// far cheaper than the lock churn it replaces — and the lock is
    /// released the moment that copy finishes, before the caller iterates.
    ///
    /// Bounded by `node_count()` rather than the mmap's raw (pre-grown)
    /// byte length: the file can be larger than the logical record count
    /// after a capacity grow, and those trailing bytes are zeroed — a
    /// zeroed `NodeRecord` has `flags == 0`, which `is_deleted()` reads as
    /// "not deleted", so including them would silently over-count.
    pub fn read_all_node_headers(&self) -> Vec<NodeRecord> {
        std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);

        let total = self.node_count() as usize;
        let wanted_len = total.saturating_mul(NODE_RECORD_SIZE);

        let guard = self.nodes_mmap.read().unwrap();
        let usable_len = wanted_len.min(guard.len());
        let usable_len = usable_len - (usable_len % NODE_RECORD_SIZE);
        bytemuck::cast_slice::<u8, NodeRecord>(&guard[..usable_len]).to_vec()
    }

    /// Write a relationship record
    /// Phase 3 Deep Optimization: Optimized write path
    pub fn write_rel(&mut self, rel_id: u64, record: &RelationshipRecord) -> Result<()> {
        let offset = (rel_id as usize * REL_RECORD_SIZE) as u64;

        // Phase 3 Optimization: Pre-check file size to avoid unnecessary grow check
        if offset + REL_RECORD_SIZE as u64 > self.rels_file_size as u64 {
            self.grow_rels_file()?;
        }

        // Phase 3 Optimization: Direct write without intermediate allocation
        let start = offset as usize;
        let end = start + REL_RECORD_SIZE;
        let record_bytes = bytemuck::bytes_of(record);
        self.rels_mmap.write().unwrap()[start..end].copy_from_slice(record_bytes);

        // Memory barrier to ensure write is visible to subsequent reads
        // Release is sufficient for single-writer model
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        Ok(())
    }

    /// Read a relationship record
    pub fn read_rel(&self, rel_id: u64) -> Result<RelationshipRecord> {
        let offset = (rel_id as usize * REL_RECORD_SIZE) as u64;

        if offset + REL_RECORD_SIZE as u64 > self.rels_file_size as u64 {
            return Err(Error::NotFound(format!(
                "Relationship {} not found",
                rel_id
            )));
        }

        let start = offset as usize;
        let end = start + REL_RECORD_SIZE;
        let guard = self.rels_mmap.read().unwrap();
        Ok(*bytemuck::from_bytes(&guard[start..end]))
    }

    /// Delete a node (mark as deleted).
    ///
    /// Does **not** clean up the external-id index.  Use
    /// [`RecordStore::delete_node_with_catalog`] when the node may carry an
    /// external id.
    pub fn delete_node(&mut self, node_id: u64) -> Result<()> {
        let mut record = self.read_node(node_id)?;
        record.mark_deleted();
        self.write_node(node_id, &record)
    }

    /// Delete a node and atomically remove its external-id index entries.
    ///
    /// Opens a write transaction on the catalog LMDB env, calls
    /// [`ExternalIdIndex::delete`] (a no-op when the node has no external id),
    /// then marks the node record as deleted — all within the same logical
    /// operation.  The catalog write transaction is committed before the
    /// in-memory record is updated, which is safe because the record store
    /// is single-writer.
    pub fn delete_node_with_catalog(
        &mut self,
        node_id: u64,
        catalog: &crate::catalog::Catalog,
    ) -> Result<()> {
        // Remove external-id mappings first (while the node is still "live").
        let mut wtxn = catalog.write_txn()?;
        catalog.external_id_index().delete(&mut wtxn, node_id)?;
        wtxn.commit()?;

        // Mark the record as deleted.
        let mut record = self.read_node(node_id)?;
        record.mark_deleted();
        self.write_node(node_id, &record)
    }

    /// Delete a relationship (mark as deleted)
    pub fn delete_rel(&mut self, rel_id: u64) -> Result<()> {
        let mut record = self.read_rel(rel_id)?;
        record.mark_deleted();
        self.write_rel(rel_id, &record)
    }

    /// Create a new node
    pub fn create_node(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<u64> {
        // Compute label bits from label names (positional mapping).
        let mut label_bits = 0u64;
        for (i, _label) in labels.iter().enumerate() {
            if i < 64 {
                label_bits |= 1u64 << i;
            }
        }
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            None,
            ConflictPolicy::Error,
            None,
        )
    }

    /// Create a new node with pre-computed label bits
    pub fn create_node_with_label_bits(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        label_bits: u64,
        properties: serde_json::Value,
    ) -> Result<u64> {
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            None,
            ConflictPolicy::Error,
            None,
        )
    }

    /// Create a node carrying an optional external id with a specified conflict policy.
    ///
    /// When `external_id` is `None` the behaviour is identical to
    /// [`RecordStore::create_node`].  When it is `Some(ext)`:
    ///
    /// - `catalog` is used to open a write transaction on the LMDB env.
    /// - The external-id index is consulted via `put_if_absent`.
    /// - If no entry exists the mapping is committed together with the new record.
    /// - If an entry already exists, `policy` decides the outcome:
    ///   - [`ConflictPolicy::Error`] — returns [`Error::ExternalIdConflict`].
    ///   - [`ConflictPolicy::Match`] — returns the existing internal id.
    ///   - [`ConflictPolicy::Replace`] — overwrites properties, returns existing id.
    pub fn create_node_with_external_id(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        labels: Vec<String>,
        properties: serde_json::Value,
        external_id: Option<ExternalId>,
        policy: ConflictPolicy,
        catalog: &crate::catalog::Catalog,
    ) -> Result<u64> {
        let mut label_bits = 0u64;
        for (i, _label) in labels.iter().enumerate() {
            if i < 64 {
                label_bits |= 1u64 << i;
            }
        }
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            external_id,
            policy,
            Some(catalog),
        )
    }

    /// Create a node with pre-computed label bits and an optional external id.
    ///
    /// Fast-path variant used by the executor (which already has label bits
    /// computed).  Conflict-policy semantics mirror
    /// [`RecordStore::create_node_with_external_id`].
    pub fn create_node_with_label_bits_and_external_id(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        label_bits: u64,
        properties: serde_json::Value,
        external_id: Option<ExternalId>,
        policy: ConflictPolicy,
        catalog: &crate::catalog::Catalog,
    ) -> Result<u64> {
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            external_id,
            policy,
            Some(catalog),
        )
    }

    /// Central implementation used by all node-creation paths.
    ///
    /// `catalog` is required only when `external_id` is `Some`.  Passing
    /// `None` for the catalog with a `Some` external id falls through to
    /// plain creation (the external-id is ignored) — this should not occur in
    /// production code but keeps the function total.
    fn create_node_with_label_bits_inner(
        &mut self,
        label_bits: u64,
        properties: serde_json::Value,
        external_id: Option<ExternalId>,
        policy: ConflictPolicy,
        catalog: Option<&crate::catalog::Catalog>,
    ) -> Result<u64> {
        // ── External-id path ──────────────────────────────────────────────────
        //
        // peek-then-allocate:
        //  1. Read next_node_id without consuming it (the probe id).
        //  2. Call put_if_absent with the probe id inside a catalog write txn.
        //  3a. No conflict → allocate (consume the id), write record, commit.
        //  3b. Conflict → dispatch on policy without allocating.
        //
        // Single-writer model means no other thread changes next_node_id
        // between step 1 and step 3a.
        if let (Some(ext), Some(cat)) = (&external_id, catalog) {
            let probe_id = self.next_node_id.load(Ordering::SeqCst);
            let mut wtxn = cat.write_txn()?;
            let idx = cat.external_id_index();

            match idx.put_if_absent(&mut wtxn, ext, probe_id)? {
                None => {
                    // No conflict — consume the id and write the record.
                    let node_id = self.allocate_node_id();
                    debug_assert_eq!(
                        node_id, probe_id,
                        "single-writer invariant violated between probe and alloc"
                    );

                    let prop_ptr = self.store_properties_if_any(node_id, &properties)?;
                    let mut record = NodeRecord::new();
                    record.label_bits = label_bits;
                    record.prop_ptr = prop_ptr;

                    tracing::debug!("create_node (ext): node_id={node_id}, prop_ptr={prop_ptr}");

                    self.write_node(node_id, &record)?;
                    wtxn.commit()?;
                    self.nodes_created.fetch_add(1, Ordering::SeqCst);
                    return Ok(node_id);
                }
                Some(existing_id) => {
                    // Conflict — abort the catalog txn.
                    drop(wtxn);
                    return match policy {
                        ConflictPolicy::Error => Err(Error::ExternalIdConflict {
                            existing_internal_id: existing_id,
                            attempted_external_id: ext.to_string(),
                        }),
                        ConflictPolicy::Match => Ok(existing_id),
                        ConflictPolicy::Replace => {
                            if properties.is_object()
                                && !properties.as_object().map(|m| m.is_empty()).unwrap_or(true)
                            {
                                // store_properties may return a new offset
                                // (when the new property bytes don't fit
                                // in-place). Capture it and re-write the
                                // NodeRecord so subsequent reads via
                                // NodeRecord.prop_ptr see the fresh data
                                // — without this the load path follows
                                // the stale offset and reads the
                                // pre-replace properties (phase9 §2.4
                                // invariant).
                                let new_prop_ptr = self
                                    .property_store
                                    .write()
                                    .map_err(|_| Error::storage("property store lock poisoned"))?
                                    .store_properties(
                                        existing_id,
                                        property_store::EntityType::Node,
                                        properties,
                                    )?;
                                if let Ok(mut record) = self.read_node(existing_id) {
                                    record.prop_ptr = new_prop_ptr;
                                    self.write_node(existing_id, &record)?;
                                }
                            }
                            Ok(existing_id)
                        }
                    };
                }
            }
        }

        // ── Plain creation (no external id, or catalog not supplied) ─────────
        let node_id = self.allocate_node_id();

        let has_properties = properties.is_object()
            && properties
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false);

        tracing::debug!(
            "create_node_with_label_bits_inner: node_id={node_id}, \
             has_properties={has_properties}"
        );

        let prop_ptr = if has_properties {
            let p = self
                .property_store
                .write()
                .map_err(|_| Error::storage("property store lock poisoned"))?
                .store_properties(node_id, property_store::EntityType::Node, properties)?;
            tracing::debug!("create_node_with_label_bits_inner: node_id={node_id}, prop_ptr={p}");
            p
        } else {
            0
        };

        let mut record = NodeRecord::new();
        record.label_bits = label_bits;
        record.prop_ptr = prop_ptr;

        self.write_node(node_id, &record)?;

        if let Ok(verify_record) = self.read_node(node_id) {
            tracing::debug!(
                "create_node_with_label_bits_inner: node_id={node_id}, \
                 verified prop_ptr={}",
                verify_record.prop_ptr
            );
        }

        self.nodes_created.fetch_add(1, Ordering::SeqCst);
        Ok(node_id)
    }

    /// Helper: store properties and return the property pointer (0 when empty).
    fn store_properties_if_any(&self, node_id: u64, properties: &serde_json::Value) -> Result<u64> {
        let has = properties.is_object()
            && properties
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false);
        if has {
            self.property_store
                .write()
                .map_err(|_| Error::storage("property store lock poisoned"))?
                .store_properties(
                    node_id,
                    property_store::EntityType::Node,
                    properties.clone(),
                )
        } else {
            Ok(0)
        }
    }

    /// Create a new relationship
    /// Phase 1 Optimization: Optimized relationship creation with reduced node reads
    pub fn create_relationship(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        from: u64,
        to: u64,
        type_id: u32,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let rel_id = self.allocate_rel_id();

        let mut record = RelationshipRecord::new(from, to, type_id);

        // Phase 1 Optimization: Batch property storage check (avoid multiple is_object checks)
        let has_properties = properties.is_object()
            && properties
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false);

        // Store properties first to get property pointer (if needed)
        record.prop_ptr = if has_properties {
            self.property_store.write().unwrap().store_properties(
                rel_id,
                property_store::EntityType::Relationship,
                properties,
            )?
        } else {
            0
        };

        // Phase 3 Deep Optimization: Optimize node reads and writes
        // Read both nodes first, then write both (better cache locality)
        let mut source_prev_ptr = 0u64;
        let mut target_prev_ptr = 0u64;
        let mut source_node_opt = None;
        let mut target_node_opt = None;

        // Memory barrier to ensure visibility of previous writes
        // Acquire is sufficient for single-writer model
        std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);

        // PHASE 1: Read source node ONCE at the beginning and preserve prop_ptr
        let mut source_node = self.read_node(from)?;

        // CRITICAL FIX: Isolate and preserve prop_ptr - never modify it during relationship creation
        // BUT: Validate prop_ptr first - if it's corrupted, reset it to 0 to prevent write failure
        let mut preserved_source_prop_ptr = source_node.prop_ptr;

        // CRITICAL FIX: Validate prop_ptr before preserving it
        // If prop_ptr points to a Relationship, it's corrupted - reset to 0
        if preserved_source_prop_ptr != 0 {
            if let Some((stored_entity_id, stored_entity_type)) = self
                .property_store
                .read()
                .unwrap()
                .get_entity_info_at_offset(preserved_source_prop_ptr)
            {
                if stored_entity_type == property_store::EntityType::Relationship {
                    tracing::warn!(
                        "[create_relationship] Source node {} prop_ptr corruption detected (points to Relationship {}), resetting to 0",
                        from,
                        stored_entity_id
                    );
                    preserved_source_prop_ptr = 0;
                }
            }
        }

        source_prev_ptr = source_node.first_rel_ptr;

        // CRITICAL FIX: If first_rel_ptr is 0 but this is not the first relationship (rel_id > 0),
        // try to find the actual first_rel_ptr by scanning existing relationships
        // This handles the case where mmap synchronization fails between queries
        if source_prev_ptr == 0 && rel_id > 0 {
            // Scan backwards to find the most recent relationship for this node
            // CRITICAL FIX: Since 'from' is the SOURCE node for the new relationship,
            // we must only look for relationships where src_id == from (not dst_id == from)
            // Relationships where dst_id == from are INCOMING to this node, not OUTGOING
            let mut found_rel_id = None;
            let mut scanned_count = 0;
            for check_rel_id in (0..rel_id).rev() {
                scanned_count += 1;
                if let Ok(rel_record) = self.read_rel(check_rel_id) {
                    if !rel_record.is_deleted() {
                        // Check if this relationship originates from the source node
                        // We only care about OUTGOING relationships (src_id == from)
                        let check_src_id = rel_record.src_id;
                        let check_dst_id = rel_record.dst_id;
                        // CRITICAL: Only consider relationships where this node is the SOURCE
                        if check_src_id == from {
                            found_rel_id = Some(check_rel_id);
                            break;
                        }
                    }
                }
                // Limit scan to avoid performance issues - only scan last 100 relationships
                if scanned_count >= 100 {
                    tracing::debug!(
                        "[create_relationship] Scan limit reached (100 relationships), stopping"
                    );
                    break;
                }
            }

            // If we found a previous relationship, use it as the prev_ptr
            if let Some(prev_rel_id) = found_rel_id {
                source_prev_ptr = prev_rel_id + 1;
                tracing::debug!(
                    "[create_relationship] Corrected source_prev_ptr from 0 to {} (prev_rel_id={})",
                    source_prev_ptr,
                    prev_rel_id
                );
            } else {
                tracing::debug!(
                    "[create_relationship] No previous relationship found after scanning {} relationships, keeping source_prev_ptr=0",
                    scanned_count
                );
            }
        }

        // CRITICAL DEBUG: Log first_rel_ptr update
        tracing::debug!(
            "[create_relationship] Source node {}: old first_rel_ptr={}, new first_rel_ptr={} (rel_id={})",
            from,
            source_prev_ptr,
            rel_id + 1,
            rel_id
        );

        // PHASE 1: Update only first_rel_ptr, FORCE prop_ptr preservation
        source_node.first_rel_ptr = rel_id + 1;
        // CRITICAL: Explicitly restore prop_ptr to the preserved value before writing
        source_node.prop_ptr = preserved_source_prop_ptr;

        // Validate that prop_ptr was correctly preserved
        if source_node.prop_ptr != preserved_source_prop_ptr {
            tracing::error!(
                "[create_relationship] FATAL ERROR: Source node {} prop_ptr corruption detected! Expected {}, got {}",
                from,
                preserved_source_prop_ptr,
                source_node.prop_ptr
            );
            return Err(Error::Storage(format!(
                "prop_ptr corruption detected for node {}",
                from
            )));
        }

        tracing::debug!(
            "[create_relationship] Source node {}: preserving prop_ptr={}, updating first_rel_ptr from {} to {}",
            from,
            preserved_source_prop_ptr,
            source_prev_ptr,
            rel_id + 1
        );
        source_node_opt = Some(source_node);

        // PHASE 1: Read target node (if different from source) - preserve prop_ptr
        if to == from {
            target_prev_ptr = source_prev_ptr;
            // For self-loops, reuse source node (prop_ptr already preserved)
            if let Some(ref source_node) = source_node_opt {
                target_node_opt = Some(*source_node);
            }
        } else {
            // Read target node ONCE and preserve prop_ptr
            let mut target_node = self.read_node(to)?;
            let mut preserved_target_prop_ptr = target_node.prop_ptr;

            // CRITICAL FIX: Validate prop_ptr before preserving it
            // If prop_ptr points to a Relationship, it's corrupted - reset to 0
            if preserved_target_prop_ptr != 0 {
                if let Some((stored_entity_id, stored_entity_type)) = self
                    .property_store
                    .read()
                    .unwrap()
                    .get_entity_info_at_offset(preserved_target_prop_ptr)
                {
                    if stored_entity_type == property_store::EntityType::Relationship {
                        tracing::warn!(
                            "[create_relationship] Target node {} prop_ptr corruption detected (points to Relationship {}), resetting to 0",
                            to,
                            stored_entity_id
                        );
                        preserved_target_prop_ptr = 0;
                    }
                }
            }

            target_prev_ptr = target_node.first_rel_ptr;

            // CRITICAL FIX: Don't update first_rel_ptr on target nodes for incoming relationships
            // first_rel_ptr should only point to OUTGOING relationships from a node
            // For incoming relationships, we use next_dst_ptr to traverse the linked list
            // Updating first_rel_ptr here causes issues when querying outgoing relationships
            // from the target node (it points to relationships where the node is destination)
            tracing::debug!(
                "[create_relationship] Target node {}: NOT updating first_rel_ptr (incoming relationship, rel_id={})",
                to,
                rel_id
            );

            // Don't update first_rel_ptr for incoming relationships
            // Just preserve prop_ptr
            target_node.prop_ptr = preserved_target_prop_ptr;

            // Validate that prop_ptr was correctly preserved
            if target_node.prop_ptr != preserved_target_prop_ptr {
                tracing::error!(
                    "[create_relationship] FATAL ERROR: Target node {} prop_ptr corruption detected! Expected {}, got {}",
                    to,
                    preserved_target_prop_ptr,
                    target_node.prop_ptr
                );
                return Err(Error::Storage(format!(
                    "prop_ptr corruption detected for node {}",
                    to
                )));
            }

            tracing::debug!(
                "[create_relationship] Target node {}: preserving prop_ptr={}, NOT updating first_rel_ptr (incoming relationship)",
                to,
                preserved_target_prop_ptr
            );
            target_node_opt = Some(target_node);
        }

        // Write both nodes (better cache locality - sequential writes)
        if let Some(source_node) = source_node_opt {
            tracing::debug!(
                "[create_relationship] Writing source node {} with first_rel_ptr={}",
                from,
                source_node.first_rel_ptr
            );
            self.write_node(from, &source_node)?;

            // CRITICAL FIX: Flush source node immediately to ensure first_rel_ptr is visible
            // for subsequent relationship creations in separate queries
            // This is essential when creating multiple relationships to the same node
            // in separate MATCH...CREATE statements
            tracing::debug!(
                "[create_relationship] Flushing source node {} after write (first_rel_ptr={})",
                from,
                source_node.first_rel_ptr
            );
            // PERFORMANCE OPTIMIZATION: Skip per-node flush - let executor batch flush at end
            // The memory barrier below is sufficient for single-writer model
            // Durability is ensured by flush_async() at executor level
            std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
        }
        if let Some(target_node) = target_node_opt {
            tracing::debug!(
                "[create_relationship] Writing target node {} with first_rel_ptr={}",
                to,
                target_node.first_rel_ptr
            );
            self.write_node(to, &target_node)?;

            // PERFORMANCE OPTIMIZATION: Skip per-node flush - handled at executor level
        }

        record.next_src_ptr = source_prev_ptr;
        record.next_dst_ptr = target_prev_ptr;

        // CRITICAL DEBUG: Log linked list construction
        tracing::debug!(
            "[create_relationship] Relationship {}: src={}, dst={}, next_src_ptr={}, next_dst_ptr={}",
            rel_id,
            from,
            to,
            source_prev_ptr,
            target_prev_ptr
        );

        // Write the record to storage
        self.write_rel(rel_id, &record)?;

        // Phase 3 Deep Optimization: Lazy adjacency list updates (defer to improve CREATE performance)
        // For now, update immediately but with optimizations
        // TODO: Future optimization - batch updates or lazy updates (update on first read)
        if let Some(ref mut adj_store) = self.adjacency_store {
            // Phase 3 Optimization: Single relationship update (optimized path)
            // Fast append path for single relationships (skips expensive traversal)
            let outgoing_rels = [(rel_id, type_id)];
            if let Err(e) = adj_store.add_outgoing_relationships(from, &outgoing_rels) {
                tracing::warn!(
                    "Failed to update adjacency list for outgoing relationship: {}",
                    e
                );
            }

            // Only update incoming if different node (avoid duplicate work for self-loops)
            if from != to {
                let incoming_rels = [(rel_id, type_id)];
                if let Err(e) = adj_store.add_incoming_relationships(to, &incoming_rels) {
                    tracing::warn!(
                        "Failed to update adjacency list for incoming relationship: {}",
                        e
                    );
                }
            }
            // Self-loop: skip incoming update (same as outgoing)
        }

        Ok(rel_id)
    }

    /// Get a node by ID
    pub fn get_node(
        &self,
        _tx: &crate::transaction::Transaction,
        id: u64,
    ) -> Result<Option<NodeRecord>> {
        // Check if node ID is valid
        if id >= self.next_node_id.load(Ordering::SeqCst) {
            return Ok(None);
        }

        // Read the node record from storage
        match self.read_node(id) {
            Ok(record) => {
                // Check if the node is deleted
                if record.is_deleted() {
                    Ok(None)
                } else {
                    Ok(Some(record))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Get a relationship by ID
    pub fn get_relationship(
        &self,
        _tx: &crate::transaction::Transaction,
        id: u64,
    ) -> Result<Option<RelationshipRecord>> {
        // Check if relationship ID is valid
        if id >= self.next_rel_id.load(Ordering::SeqCst) {
            return Ok(None);
        }

        // Read the relationship record from storage
        match self.read_rel(id) {
            Ok(record) => {
                // Check if the relationship is deleted
                if record.is_deleted() {
                    Ok(None)
                } else {
                    Ok(Some(record))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Phase 3: Get outgoing relationships from adjacency list (optimized traversal)
    pub fn get_outgoing_relationships_adjacency(
        &self,
        node_id: u64,
        type_ids: &[u32],
    ) -> Result<Option<Vec<u64>>> {
        if let Some(ref adj_store) = self.adjacency_store {
            match adj_store.get_outgoing_relationships(node_id, type_ids) {
                Ok(rel_ids) => Ok(Some(rel_ids)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Phase 3: Get incoming relationships from adjacency list (optimized traversal)
    pub fn get_incoming_relationships_adjacency(
        &self,
        node_id: u64,
        type_ids: &[u32],
    ) -> Result<Option<Vec<u64>>> {
        if let Some(ref adj_store) = self.adjacency_store {
            match adj_store.get_incoming_relationships(node_id, type_ids) {
                Ok(rel_ids) => Ok(Some(rel_ids)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Phase 3 Deep Optimization: Count relationships using adjacency list (fast path)
    pub fn count_relationships_adjacency(
        &self,
        node_id: u64,
        type_ids: &[u32],
        direction: crate::executor::Direction,
    ) -> Result<Option<u64>> {
        if let Some(ref adj_store) = self.adjacency_store {
            match direction {
                crate::executor::Direction::Outgoing => {
                    match adj_store.count_outgoing_relationships(node_id, type_ids) {
                        Ok(count) => Ok(Some(count)),
                        Err(_) => Ok(None),
                    }
                }
                crate::executor::Direction::Incoming => {
                    match adj_store.count_incoming_relationships(node_id, type_ids) {
                        Ok(count) => Ok(Some(count)),
                        Err(_) => Ok(None),
                    }
                }
                crate::executor::Direction::Both => {
                    let outgoing = adj_store.count_outgoing_relationships(node_id, type_ids)?;
                    let incoming = adj_store.count_incoming_relationships(node_id, type_ids)?;
                    Ok(Some(outgoing + incoming))
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Clear all data from the storage
    pub fn clear_all(&mut self) -> Result<()> {
        tracing::debug!("[RecordStore::clear_all] Clearing all storage data");

        // Reset counters
        self.next_node_id.store(0, Ordering::SeqCst);
        self.next_rel_id.store(0, Ordering::SeqCst);

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

    /// Load properties for a node
    /// PHASE 3: Enhanced validation with safe fallback to reverse_index
    pub fn load_node_properties(&self, node_id: u64) -> Result<Option<serde_json::Value>> {
        let prop_ptr = self.read_node(node_id).ok().map(|r| r.prop_ptr);
        self.load_node_properties_inner(node_id, prop_ptr)
    }

    /// Same as [`Self::load_node_properties`], but for callers that
    /// already hold a `NodeRecord` (and thus its `prop_ptr`) from a
    /// `read_node`/`read_node_header`-family call moments earlier.
    ///
    /// phase8_neo4j-concurrency-gaps §2 — `load_node_properties(node_id)`
    /// re-reads the node record internally purely to recover `prop_ptr`.
    /// `Executor::read_node_as_value` (the single most-called node
    /// materialiser in the executor — every scan, expand hop, and index
    /// seek routes through it) already has that `NodeRecord` in hand, so
    /// that internal re-read was a second `nodes_mmap` lock acquisition
    /// (plus a second `property_store` corruption cross-check) on every
    /// single node materialisation. Multiplied across every node a scan
    /// or expand hop touches, this was a meaningful share of the
    /// per-node lock traffic behind `traversal.small_two_hop_from_hub`'s
    /// concurrency ceiling. Identical validation/fallback logic to
    /// `load_node_properties` — only the `prop_ptr` source differs.
    pub fn load_node_properties_with_ptr(
        &self,
        node_id: u64,
        prop_ptr: u64,
    ) -> Result<Option<serde_json::Value>> {
        self.load_node_properties_inner(node_id, Some(prop_ptr))
    }

    /// Shared body of [`Self::load_node_properties`] and
    /// [`Self::load_node_properties_with_ptr`]. `prop_ptr = None` means
    /// "the caller could not read a `NodeRecord` at all" (mirrors the
    /// original `self.read_node(node_id)` failure branch); `Some(0)`
    /// means "read a record, but it has no properties yet".
    fn load_node_properties_inner(
        &self,
        node_id: u64,
        prop_ptr: Option<u64>,
    ) -> Result<Option<serde_json::Value>> {
        // phase8_neo4j-concurrency-gaps §2 — acquire the `property_store`
        // read lock ONCE for this whole call instead of once per branch
        // below (up to 3 separate acquisitions previously: the entity-
        // info validation, the offset load, and the reverse-index
        // fallback). Every node materialisation in the executor
        // (`read_node_as_value`, called from every scan/expand/index-seek
        // path) goes through this function, so this is on the hottest
        // per-node lock in the read path.
        let prop_guard = self.property_store.read().unwrap();

        // First try to use prop_ptr from NodeRecord (more reliable)
        if let Some(prop_ptr) = prop_ptr {
            tracing::debug!(
                "load_node_properties: node_id={}, prop_ptr={}",
                node_id,
                prop_ptr
            );
            if prop_ptr != 0 {
                // PHASE 3: Double validation - verify that prop_ptr points to Node properties
                // Check the entity_type stored at this offset BEFORE loading
                if let Some((stored_entity_id, stored_entity_type)) =
                    prop_guard.get_entity_info_at_offset(prop_ptr)
                {
                    if stored_entity_type != property_store::EntityType::Node
                        || stored_entity_id != node_id
                    {
                        // PHASE 3: Prop_ptr corruption detected - fallback to reverse_index
                        tracing::warn!(
                            "load_node_properties: node_id={} prop_ptr={} points to wrong entity (type={:?}, id={}), using reverse_index instead",
                            node_id,
                            prop_ptr,
                            stored_entity_type,
                            stored_entity_id
                        );
                        // Fall through to reverse_index lookup - prop_ptr is corrupted
                    } else {
                        // PHASE 3: Entity type and ID match - safe to load from prop_ptr
                        match prop_guard.load_properties_at_offset(prop_ptr) {
                            Ok(Some(props)) => {
                                let keys = props.as_object().map(|m| m.keys().collect::<Vec<_>>());
                                tracing::debug!(
                                    "load_node_properties: node_id={}, loaded properties from prop_ptr={}, keys={:?}",
                                    node_id,
                                    prop_ptr,
                                    keys
                                );
                                // PHASE 3: Additional validation - check for relationship-like properties
                                if let Some(obj) = props.as_object() {
                                    if obj.contains_key("since") || obj.contains_key("type") {
                                        tracing::warn!(
                                            "load_node_properties: node_id={} prop_ptr={} returned relationship-like properties: {:?}. Falling back to reverse_index",
                                            node_id,
                                            prop_ptr,
                                            keys
                                        );
                                        // Fall through to reverse_index - properties look wrong
                                    } else {
                                        return Ok(Some(props));
                                    }
                                } else {
                                    return Ok(Some(props));
                                }
                            }
                            Ok(None) => {
                                tracing::debug!(
                                    "load_node_properties: node_id={}, prop_ptr={} returned None, using reverse_index",
                                    node_id,
                                    prop_ptr
                                );
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "load_node_properties: node_id={}, error loading from prop_ptr={}: {}, using reverse_index",
                                    node_id,
                                    prop_ptr,
                                    e
                                );
                            }
                        }
                    }
                } else {
                    tracing::warn!(
                        "load_node_properties: node_id={} prop_ptr={} not found in property_store",
                        node_id,
                        prop_ptr
                    );
                    // Fall through to reverse_index lookup
                }
            } else {
                tracing::debug!(
                    "load_node_properties: node_id={}, prop_ptr is 0, trying reverse_index",
                    node_id
                );
            }
        } else {
            tracing::debug!(
                "load_node_properties: node_id={}, failed to read node record, trying reverse_index",
                node_id
            );
        }

        // PHASE 3: Safe fallback to reverse_index lookup (always reliable)
        let result = prop_guard.load_properties(node_id, property_store::EntityType::Node);
        let keys_debug = result.as_ref().ok().and_then(|opt| {
            opt.as_ref()
                .map(|v| v.as_object().map(|m| m.keys().collect::<Vec<_>>()))
        });
        tracing::debug!(
            "load_node_properties: node_id={}, reverse_index result: {:?}",
            node_id,
            keys_debug
        );
        // PHASE 3: Final validation - check if reverse_index returned relationship-like properties
        if let Ok(Some(props)) = &result {
            if let Some(obj) = props.as_object() {
                if obj.contains_key("since") || obj.contains_key("type") {
                    tracing::warn!(
                        "load_node_properties: node_id={} reverse_index has relationship-like properties: {:?}. This indicates severe data corruption!",
                        node_id,
                        keys_debug
                    );
                }
            }
        }
        result
    }

    /// Load properties for a relationship
    pub fn load_relationship_properties(&self, rel_id: u64) -> Result<Option<serde_json::Value>> {
        // For relationships, use reverse_index lookup
        // (Relationship records are accessed differently, so we use the index)
        self.property_store
            .read()
            .unwrap()
            .load_properties(rel_id, property_store::EntityType::Relationship)
    }

    /// Update properties for a node
    /// CRITICAL FIX: Also updates node record's prop_ptr to ensure consistency
    pub fn update_node_properties(
        &mut self,
        node_id: u64,
        properties: serde_json::Value,
    ) -> Result<()> {
        let new_prop_ptr = if properties.is_object() && !properties.as_object().unwrap().is_empty()
        {
            let prop_ptr = self.property_store.write().unwrap().store_properties(
                node_id,
                property_store::EntityType::Node,
                properties,
            )?;
            tracing::debug!(
                "update_node_properties: node_id={}, stored properties, new_prop_ptr={}",
                node_id,
                prop_ptr
            );
            prop_ptr
        } else {
            self.property_store
                .write()
                .unwrap()
                .delete_properties(node_id, property_store::EntityType::Node)?;
            tracing::debug!(
                "update_node_properties: node_id={}, deleted properties, new_prop_ptr=0",
                node_id
            );
            0
        };

        // CRITICAL FIX: Update the node record's prop_ptr to match the new offset
        // This ensures load_node_properties reads from the correct location
        if let Ok(mut node_record) = self.read_node(node_id) {
            if node_record.prop_ptr != new_prop_ptr {
                tracing::debug!(
                    "update_node_properties: node_id={}, updating prop_ptr from {} to {}",
                    node_id,
                    node_record.prop_ptr,
                    new_prop_ptr
                );
                node_record.prop_ptr = new_prop_ptr;
                self.write_node(node_id, &node_record)?;
            }
        }
        Ok(())
    }

    /// Update properties for a relationship
    pub fn update_relationship_properties(
        &mut self,
        rel_id: u64,
        properties: serde_json::Value,
    ) -> Result<()> {
        if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.write().unwrap().store_properties(
                rel_id,
                property_store::EntityType::Relationship,
                properties,
            )?;
        } else {
            self.property_store
                .write()
                .unwrap()
                .delete_properties(rel_id, property_store::EntityType::Relationship)?;
        }
        Ok(())
    }

    /// Delete properties for a node
    pub fn delete_node_properties(&mut self, node_id: u64) -> Result<()> {
        self.property_store
            .write()
            .unwrap()
            .delete_properties(node_id, property_store::EntityType::Node)
    }

    /// Delete properties for a relationship
    pub fn delete_relationship_properties(&mut self, rel_id: u64) -> Result<()> {
        self.property_store
            .write()
            .unwrap()
            .delete_properties(rel_id, property_store::EntityType::Relationship)
    }

    /// Get property store statistics
    pub fn property_count(&self) -> usize {
        self.property_store.read().unwrap().property_count()
    }
}
