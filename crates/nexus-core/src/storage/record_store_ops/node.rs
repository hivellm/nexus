use crate::error::{Error, Result};
use crate::storage::property_store;
use crate::storage::record_store::RecordStore;
use crate::storage::records::{FLAG_ALLOCATED, NODE_RECORD_SIZE, NodeRecord};

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

        // Overflow-safe offset (see read_node); a write whose offset
        // arithmetic overflows is rejected rather than wrapping past the map.
        let offset = node_id
            .checked_mul(NODE_RECORD_SIZE as u64)
            .ok_or_else(|| Error::Storage(format!("node id {} offset overflow", node_id)))?;
        let record_end = offset
            .checked_add(NODE_RECORD_SIZE as u64)
            .ok_or_else(|| Error::Storage(format!("node id {} offset overflow", node_id)))?;

        // Grow the file if the target record extends past it. #4: the grow is
        // sized to at least `record_end`, so a sparse write far past EOF is
        // covered by a single grow instead of slicing past the mapping.
        if record_end > self.nodes_file_size as u64 {
            self.grow_nodes_file(record_end)?;
        }

        // Phase 3 Optimization: Direct write without intermediate allocation
        let start = offset as usize;
        let end = start + NODE_RECORD_SIZE;
        // phase0_fix-anonymous-node-lost-on-restart: stamp the allocated bit
        // on every write so a live node — even one with no labels,
        // properties or relationships — is never byte-for-byte all-zero on
        // disk. This also self-migrates any legacy (flags == 0) record that
        // gets rewritten after the fix, without mutating the caller's copy.
        let mut record_to_write = *record;
        record_to_write.flags |= FLAG_ALLOCATED;
        let record_bytes = bytemuck::bytes_of(&record_to_write);
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

        // Overflow-safe offset: compute `node_id * SIZE` and `offset + SIZE`
        // with checked u64 arithmetic (never `id as usize`, which also
        // truncates on 32-bit targets). A crafted/corrupt id can otherwise
        // make the multiply overflow, or make `offset + SIZE` wrap to 0 and
        // slip past the bounds check, then panic on the out-of-range slice.
        let offset = node_id
            .checked_mul(NODE_RECORD_SIZE as u64)
            .ok_or_else(|| Error::NotFound(format!("Node {} not found", node_id)))?;
        let record_end = offset
            .checked_add(NODE_RECORD_SIZE as u64)
            .ok_or_else(|| Error::NotFound(format!("Node {} not found", node_id)))?;

        let start = offset as usize;
        let end = start + NODE_RECORD_SIZE;
        // Bound-check against the LIVE shared mapping length under the same
        // read lock used to copy the record — never the per-clone cached
        // `nodes_file_size`, which diverges from the shared mmap after a grow
        // by another clone (stale-small -> spurious NotFound) or a clear_all
        // (stale-large -> out-of-bounds slice). Mirrors read_all_node_headers.
        // See phase0_fix-store-size-per-clone-divergence.
        let mut record: NodeRecord = {
            let guard = self.nodes_mmap.read().unwrap();
            if record_end > guard.len() as u64 {
                return Err(Error::NotFound(format!("Node {} not found", node_id)));
            }
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
}
