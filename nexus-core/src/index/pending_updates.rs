//! Pending Index Updates for Deferred Index Maintenance
//!
//! This module provides structures to accumulate index updates during transactions
//! and apply them in batches during commit for improved write performance.
//!
//! Features:
//! - **Deferred updates**: Accumulate index updates during transaction
//! - **Batch application**: Apply all updates in batch during commit
//! - **Consistency**: Maintain index consistency across transactions

use serde_json::Value;
use std::collections::HashSet;

/// Pending index update operation
#[derive(Debug, Clone)]
pub enum IndexUpdate {
    /// Add node to label index
    AddNodeToLabel { node_id: u64, label_ids: Vec<u32> },
    /// Remove node from label index
    RemoveNodeFromLabel { node_id: u64, label_ids: Vec<u32> },
    /// Index node properties
    IndexNodeProperties { node_id: u64, properties: Value },
    /// Remove node from property index
    RemoveNodeFromPropertyIndex { node_id: u64 },
    /// Add relationship to index
    AddRelationship {
        rel_id: u64,
        source_id: u64,
        target_id: u64,
        type_id: u32,
    },
    /// Remove relationship from index
    RemoveRelationship {
        rel_id: u64,
        source_id: u64,
        target_id: u64,
        type_id: u32,
    },
}

/// Accumulator for pending index updates during a transaction
#[derive(Debug, Clone, Default)]
pub struct PendingIndexUpdates {
    /// Pending index updates
    updates: Vec<IndexUpdate>,
    /// Nodes affected by updates (for quick lookup)
    affected_nodes: HashSet<u64>,
    /// Relationships affected by updates (for quick lookup)
    affected_relationships: HashSet<u64>,
}

impl PendingIndexUpdates {
    /// Create a new pending index updates accumulator
    pub fn new() -> Self {
        Self {
            updates: Vec::new(),
            affected_nodes: HashSet::new(),
            affected_relationships: HashSet::new(),
        }
    }

    /// Add a pending update
    pub fn add_update(&mut self, update: IndexUpdate) {
        // Track affected entities
        match &update {
            IndexUpdate::AddNodeToLabel { node_id, .. }
            | IndexUpdate::RemoveNodeFromLabel { node_id, .. }
            | IndexUpdate::IndexNodeProperties { node_id, .. }
            | IndexUpdate::RemoveNodeFromPropertyIndex { node_id } => {
                self.affected_nodes.insert(*node_id);
            }
            IndexUpdate::AddRelationship { rel_id, .. }
            | IndexUpdate::RemoveRelationship { rel_id, .. } => {
                self.affected_relationships.insert(*rel_id);
            }
        }

        self.updates.push(update);
    }

    /// Get all pending updates (consumes the accumulator)
    pub fn take_updates(&mut self) -> Vec<IndexUpdate> {
        let updates = std::mem::take(&mut self.updates);
        self.affected_nodes.clear();
        self.affected_relationships.clear();
        updates
    }

    /// Get pending updates without consuming (for inspection)
    #[allow(dead_code)]
    pub fn get_updates(&self) -> &[IndexUpdate] {
        &self.updates
    }

    /// Check if there are any pending updates
    pub fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }

    /// Get count of pending updates
    pub fn len(&self) -> usize {
        self.updates.len()
    }

    /// Clear all pending updates
    pub fn clear(&mut self) {
        self.updates.clear();
        self.affected_nodes.clear();
        self.affected_relationships.clear();
    }

    /// Check if a node is affected by pending updates
    pub fn is_node_affected(&self, node_id: u64) -> bool {
        self.affected_nodes.contains(&node_id)
    }

    /// Check if a relationship is affected by pending updates
    pub fn is_relationship_affected(&self, rel_id: u64) -> bool {
        self.affected_relationships.contains(&rel_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_index_updates_empty() {
        let updates = PendingIndexUpdates::new();
        assert!(updates.is_empty());
        assert_eq!(updates.len(), 0);
    }

    #[test]
    fn test_pending_index_updates_add() {
        let mut updates = PendingIndexUpdates::new();

        let update = IndexUpdate::AddNodeToLabel {
            node_id: 1,
            label_ids: vec![0, 1],
        };
        updates.add_update(update);

        assert!(!updates.is_empty());
        assert_eq!(updates.len(), 1);
        assert!(updates.is_node_affected(1));
    }

    #[test]
    fn test_pending_index_updates_take() {
        let mut updates = PendingIndexUpdates::new();

        updates.add_update(IndexUpdate::AddNodeToLabel {
            node_id: 1,
            label_ids: vec![0],
        });
        updates.add_update(IndexUpdate::AddRelationship {
            rel_id: 1,
            source_id: 1,
            target_id: 2,
            type_id: 0,
        });

        assert_eq!(updates.len(), 2);

        let taken = updates.take_updates();
        assert_eq!(taken.len(), 2);
        assert!(updates.is_empty());
        assert!(!updates.is_node_affected(1));
        assert!(!updates.is_relationship_affected(1));
    }

    #[test]
    fn test_pending_index_updates_clear() {
        let mut updates = PendingIndexUpdates::new();

        updates.add_update(IndexUpdate::AddNodeToLabel {
            node_id: 1,
            label_ids: vec![0],
        });
        assert!(!updates.is_empty());

        updates.clear();
        assert!(updates.is_empty());
    }
}
