use crate::error::{Error, Result};
use crate::storage::record_store::RecordStore;
use crate::storage::records::{FLAG_ALLOCATED, REL_RECORD_SIZE, RelationshipRecord};

impl RecordStore {
    /// Write a relationship record
    /// Phase 3 Deep Optimization: Optimized write path
    pub fn write_rel(&mut self, rel_id: u64, record: &RelationshipRecord) -> Result<()> {
        // Overflow-safe offset (see read_node); a write whose offset
        // arithmetic overflows is rejected rather than wrapping past the map.
        let offset = rel_id
            .checked_mul(REL_RECORD_SIZE as u64)
            .ok_or_else(|| Error::Storage(format!("relationship id {} offset overflow", rel_id)))?;
        let record_end = offset
            .checked_add(REL_RECORD_SIZE as u64)
            .ok_or_else(|| Error::Storage(format!("relationship id {} offset overflow", rel_id)))?;

        // Grow the file if the target record extends past it. #4: sized to at
        // least `record_end` so a sparse write far past EOF is covered.
        if record_end > self.rels_file_size as u64 {
            self.grow_rels_file(record_end)?;
        }

        // Phase 3 Optimization: Direct write without intermediate allocation
        let start = offset as usize;
        let end = start + REL_RECORD_SIZE;
        // phase0_fix-anonymous-node-lost-on-restart §2.3: same allocated-bit
        // stamp as write_node, closing the degenerate all-zero self-loop gap
        // (src_id == dst_id == 0, type_id == 0, no pointers).
        let mut record_to_write = *record;
        record_to_write.flags |= FLAG_ALLOCATED;
        let record_bytes = bytemuck::bytes_of(&record_to_write);
        self.rels_mmap.write().unwrap()[start..end].copy_from_slice(record_bytes);

        // Memory barrier to ensure write is visible to subsequent reads
        // Release is sufficient for single-writer model
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        // Maintain the adjacency index HERE, and only here: this is the single
        // funnel every relationship-record mutation passes through (creation,
        // the `next_src_ptr` fix-ups, and all five deletion paths, only two of
        // which go through `delete_rel`). Doing it at the funnel is what makes
        // the index authoritative for write paths that do not know it exists —
        // the property `cache::RelationshipIndex` lacks, and the reason it can
        // only ever be a hint. Endpoints are immutable once written, so the
        // record's own deleted bit decides add-or-remove; no read-before-write.
        // See docs/analysis/store-adjacency-index/01_write_chokepoint.md.
        if record_to_write.is_deleted() {
            self.adjacency_index
                .remove(rel_id, record_to_write.src_id, record_to_write.dst_id);
        } else {
            self.adjacency_index
                .insert(rel_id, record_to_write.src_id, record_to_write.dst_id);
        }

        Ok(())
    }

    /// Live relationship ids pointing AT `node_id` (the reverse adjacency the
    /// record format itself does not carry), ascending. O(in-degree).
    pub fn incoming_relationships(&self, node_id: u64) -> Vec<u64> {
        self.adjacency_index.incoming(node_id)
    }

    /// Live relationship ids `node_id` is the source of, ascending.
    /// O(out-degree). Unlike a `first_rel_ptr` chain walk this cannot be
    /// defeated by a damaged chain — it is rebuilt from the records on open.
    pub fn outgoing_relationships(&self, node_id: u64) -> Vec<u64> {
        self.adjacency_index.outgoing(node_id)
    }

    /// Every live relationship id incident on `node_id` in either direction,
    /// ascending and de-duplicated (a self-loop appears once). O(degree).
    pub fn connected_relationships(&self, node_id: u64) -> Vec<u64> {
        self.adjacency_index.connected(node_id)
    }

    /// Whether any live relationship touches `node_id`. O(1).
    pub fn has_any_relationship(&self, node_id: u64) -> bool {
        self.adjacency_index.has_any(node_id)
    }

    /// Read a relationship record
    pub fn read_rel(&self, rel_id: u64) -> Result<RelationshipRecord> {
        // Overflow-safe offset (see read_node).
        let offset = rel_id
            .checked_mul(REL_RECORD_SIZE as u64)
            .ok_or_else(|| Error::NotFound(format!("Relationship {} not found", rel_id)))?;
        let record_end = offset
            .checked_add(REL_RECORD_SIZE as u64)
            .ok_or_else(|| Error::NotFound(format!("Relationship {} not found", rel_id)))?;

        let start = offset as usize;
        let end = start + REL_RECORD_SIZE;
        // Bound-check against the live shared mapping length under the read
        // lock, not the per-clone cached `rels_file_size` (see read_node).
        let guard = self.rels_mmap.read().unwrap();
        if record_end > guard.len() as u64 {
            return Err(Error::NotFound(format!(
                "Relationship {} not found",
                rel_id
            )));
        }
        Ok(*bytemuck::from_bytes(&guard[start..end]))
    }
}
