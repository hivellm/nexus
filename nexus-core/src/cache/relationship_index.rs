//! Advanced Relationship Indexing System
//!
//! This module provides high-performance indexes for relationship queries to replace
//! the slow linked-list traversal approach. It implements:
//!
//! - **Type-based indexes**: `type_id → RoaringBitmap<rel_id>` for fast type filtering
//! - **Node-based indexes**: `node_id → Vec<rel_id>` for fast node relationship lookup
//! - **Direction-aware**: Separate indexes for outgoing and incoming relationships
//! - **Memory-efficient**: Uses RoaringBitmap for sparse relationship ID sets
//!
//! ## Performance Improvements
//!
//! - **Relationship queries**: O(1) type filtering vs O(n) linked list traversal
//! - **Node expansion**: O(k) where k is relationship count vs O(n) traversal
//! - **Memory usage**: Efficient bitmap compression for sparse data
//! - **Cache-friendly**: Indexes can be cached in memory for hot data

use crate::Result;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::sync::RwLock;

/// Type-based relationship index: maps relationship type to set of relationship IDs
#[derive(Debug, Clone)]
pub struct TypeRelationshipIndex {
    /// Maps type_id to bitmap of relationship IDs of that type
    pub type_to_rels: HashMap<u32, RoaringBitmap>,
}

/// Direction-specific relationship index for a node
#[derive(Debug, Clone)]
pub struct NodeRelationshipIndex {
    /// Outgoing relationships: maps type_id to relationship IDs
    pub outgoing: HashMap<u32, Vec<u64>>,
    /// Incoming relationships: maps type_id to relationship IDs
    pub incoming: HashMap<u32, Vec<u64>>,
}

/// Comprehensive relationship index system
#[derive(Debug)]
pub struct RelationshipIndex {
    /// Type-based index for fast relationship type filtering
    type_index: RwLock<TypeRelationshipIndex>,
    /// Node-based index for fast node relationship lookup
    node_index: RwLock<HashMap<u64, NodeRelationshipIndex>>,
    /// Statistics for monitoring
    stats: RwLock<RelationshipIndexStats>,
}

#[derive(Debug, Clone, Default)]
pub struct RelationshipIndexStats {
    /// Total relationships indexed
    pub total_relationships: u64,
    /// Total nodes with indexed relationships
    pub total_nodes: u64,
    /// Memory usage in bytes
    pub memory_usage: usize,
    /// Cache hit rate for index lookups
    pub hit_rate: f64,
    /// Number of index lookups
    pub lookups: u64,
    /// Number of cache hits
    pub hits: u64,
}

impl Default for RelationshipIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationshipIndex {
    /// Create a new relationship index
    pub fn new() -> Self {
        Self {
            type_index: RwLock::new(TypeRelationshipIndex {
                type_to_rels: HashMap::new(),
            }),
            node_index: RwLock::new(HashMap::new()),
            stats: RwLock::new(RelationshipIndexStats::default()),
        }
    }

    /// Index a new relationship
    pub fn add_relationship(
        &self,
        rel_id: u64,
        src_id: u64,
        dst_id: u64,
        type_id: u32,
    ) -> Result<()> {
        // Update type index
        {
            let mut type_index = self.type_index.write().unwrap();
            type_index
                .type_to_rels
                .entry(type_id)
                .or_default()
                .insert(rel_id as u32);
        }

        // Update node index for source node (outgoing)
        {
            let mut node_index = self.node_index.write().unwrap();
            let src_entry = node_index
                .entry(src_id)
                .or_insert_with(|| NodeRelationshipIndex {
                    outgoing: HashMap::new(),
                    incoming: HashMap::new(),
                });
            src_entry.outgoing.entry(type_id).or_default().push(rel_id);
        }

        // Update node index for destination node (incoming)
        {
            let mut node_index = self.node_index.write().unwrap();
            let dst_entry = node_index
                .entry(dst_id)
                .or_insert_with(|| NodeRelationshipIndex {
                    outgoing: HashMap::new(),
                    incoming: HashMap::new(),
                });
            dst_entry.incoming.entry(type_id).or_default().push(rel_id);
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.total_relationships += 1;

            // Track unique nodes (approximate)
            let node_index = self.node_index.read().unwrap();
            stats.total_nodes = node_index.len() as u64;

            // Rough memory estimation
            stats.memory_usage += 16; // Approximate per relationship
        }

        Ok(())
    }

    /// Remove a relationship from the index
    pub fn remove_relationship(
        &self,
        rel_id: u64,
        src_id: u64,
        dst_id: u64,
        type_id: u32,
    ) -> Result<()> {
        // Update type index
        {
            let mut type_index = self.type_index.write().unwrap();
            if let Some(bitmap) = type_index.type_to_rels.get_mut(&type_id) {
                bitmap.remove(rel_id as u32);
                if bitmap.is_empty() {
                    type_index.type_to_rels.remove(&type_id);
                }
            }
        }

        // Update node index for source node
        {
            let mut node_index = self.node_index.write().unwrap();
            if let Some(src_entry) = node_index.get_mut(&src_id) {
                if let Some(outgoing) = src_entry.outgoing.get_mut(&type_id) {
                    outgoing.retain(|&id| id != rel_id);
                    if outgoing.is_empty() {
                        src_entry.outgoing.remove(&type_id);
                    }
                }
                if src_entry.outgoing.is_empty() && src_entry.incoming.is_empty() {
                    node_index.remove(&src_id);
                }
            }
        }

        // Update node index for destination node
        {
            let mut node_index = self.node_index.write().unwrap();
            if let Some(dst_entry) = node_index.get_mut(&dst_id) {
                if let Some(incoming) = dst_entry.incoming.get_mut(&type_id) {
                    incoming.retain(|&id| id != rel_id);
                    if incoming.is_empty() {
                        dst_entry.incoming.remove(&type_id);
                    }
                }
                if dst_entry.outgoing.is_empty() && dst_entry.incoming.is_empty() {
                    node_index.remove(&dst_id);
                }
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            if stats.total_relationships > 0 {
                stats.total_relationships -= 1;
                stats.memory_usage = stats.memory_usage.saturating_sub(16);
            }
        }

        Ok(())
    }

    /// Get all relationships of specific types using type index
    pub fn get_relationships_by_types(&self, type_ids: &[u32]) -> Result<Vec<u64>> {
        let type_index = self.type_index.read().unwrap();

        if type_ids.is_empty() {
            // Return all relationships across all types
            let mut all_rels = Vec::new();
            for bitmap in type_index.type_to_rels.values() {
                all_rels.extend(bitmap.iter().map(|id| id as u64));
            }
            Ok(all_rels)
        } else {
            let mut result = Vec::new();
            for &type_id in type_ids {
                if let Some(bitmap) = type_index.type_to_rels.get(&type_id) {
                    result.extend(bitmap.iter().map(|id| id as u64));
                }
            }
            Ok(result)
        }
    }

    /// Get relationships for a node with specific types and direction
    pub fn get_node_relationships(
        &self,
        node_id: u64,
        type_ids: &[u32],
        outgoing: bool,
    ) -> Result<Vec<u64>> {
        let node_index = self.node_index.read().unwrap();

        let mut result = Vec::new();

        if let Some(node_entry) = node_index.get(&node_id) {
            let type_map = if outgoing {
                &node_entry.outgoing
            } else {
                &node_entry.incoming
            };

            if type_ids.is_empty() {
                // Return all relationships of this direction
                for rels in type_map.values() {
                    result.extend(rels);
                }
            } else {
                // Return relationships of specific types
                for &type_id in type_ids {
                    if let Some(rels) = type_map.get(&type_id) {
                        result.extend(rels);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get statistics
    pub fn stats(&self) -> RelationshipIndexStats {
        self.stats.read().unwrap().clone()
    }

    /// Clear all indexes
    pub fn clear(&self) -> Result<()> {
        {
            let mut type_index = self.type_index.write().unwrap();
            type_index.type_to_rels.clear();
        }
        {
            let mut node_index = self.node_index.write().unwrap();
            node_index.clear();
        }
        {
            let mut stats = self.stats.write().unwrap();
            *stats = RelationshipIndexStats::default();
        }
        Ok(())
    }

    /// Check if relationship index is healthy
    pub fn health_check(&self) -> Result<()> {
        // Basic consistency checks
        let type_index = self.type_index.read().unwrap();
        let node_index = self.node_index.read().unwrap();

        // Count total relationships from type index
        let mut total_from_types = 0u64;
        for bitmap in type_index.type_to_rels.values() {
            total_from_types += bitmap.len();
        }

        // Count total relationships from node index
        let mut total_from_nodes = 0u64;
        for node_entry in node_index.values() {
            for rels in node_entry.outgoing.values() {
                total_from_nodes += rels.len() as u64;
            }
            for rels in node_entry.incoming.values() {
                total_from_nodes += rels.len() as u64;
            }
        }

        // They should match (each relationship appears in both source and destination nodes)
        if total_from_types != total_from_nodes / 2 {
            return Err(crate::Error::IndexConsistency(format!(
                "Index inconsistency: type_index has {} relationships, node_index has {} total entries",
                total_from_types, total_from_nodes
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relationship_index_basic() {
        let index = RelationshipIndex::new();

        // Add some relationships
        index.add_relationship(1, 10, 20, 1).unwrap(); // src=10, dst=20, type=1
        index.add_relationship(2, 10, 30, 1).unwrap(); // src=10, dst=30, type=1
        index.add_relationship(3, 20, 10, 2).unwrap(); // src=20, dst=10, type=2

        // Test type-based queries
        let type1_rels = index.get_relationships_by_types(&[1]).unwrap();
        assert_eq!(type1_rels.len(), 2);
        assert!(type1_rels.contains(&1));
        assert!(type1_rels.contains(&2));

        // Test node-based queries
        let outgoing_from_10 = index.get_node_relationships(10, &[], true).unwrap();
        assert_eq!(outgoing_from_10.len(), 2);
        assert!(outgoing_from_10.contains(&1));
        assert!(outgoing_from_10.contains(&2));

        let incoming_to_10 = index.get_node_relationships(10, &[], false).unwrap();
        assert_eq!(incoming_to_10.len(), 1);
        assert!(incoming_to_10.contains(&3));

        // Test filtering by type
        let outgoing_type1_from_10 = index.get_node_relationships(10, &[1], true).unwrap();
        assert_eq!(outgoing_type1_from_10.len(), 2);
    }

    #[test]
    fn test_relationship_index_removal() {
        let index = RelationshipIndex::new();

        // Add and then remove a relationship
        index.add_relationship(1, 10, 20, 1).unwrap();
        index.remove_relationship(1, 10, 20, 1).unwrap();

        let type1_rels = index.get_relationships_by_types(&[1]).unwrap();
        assert_eq!(type1_rels.len(), 0);

        let outgoing_from_10 = index.get_node_relationships(10, &[], true).unwrap();
        assert_eq!(outgoing_from_10.len(), 0);

        let incoming_to_20 = index.get_node_relationships(20, &[], false).unwrap();
        assert_eq!(incoming_to_20.len(), 0);
    }

    #[test]
    fn test_relationship_index_stats() {
        let index = RelationshipIndex::new();

        index.add_relationship(1, 10, 20, 1).unwrap();
        index.add_relationship(2, 10, 30, 1).unwrap();

        let stats = index.stats();
        assert_eq!(stats.total_relationships, 2);
        assert!(stats.memory_usage > 0);
    }
}
