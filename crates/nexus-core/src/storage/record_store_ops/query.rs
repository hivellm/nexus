use std::sync::atomic::Ordering;

use crate::error::Result;
use crate::storage::record_store::RecordStore;
use crate::storage::records::{NodeRecord, RelationshipRecord};

impl RecordStore {
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
}
