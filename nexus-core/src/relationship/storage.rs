//! Specialized Relationship Storage
//!
//! Implements relationship-centric storage structures for optimized
//! access patterns and memory efficiency.

use parking_lot::RwLock as ParkingRwLock;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::executor::Direction;
use crate::relationship::*;
use serde_json::Value;

/// Relationship record with optimized layout
#[derive(Debug, Clone)]
pub struct RelationshipRecord {
    pub id: u64,
    pub source_id: u64,
    pub target_id: u64,
    pub type_id: u32,
    pub properties: Vec<u8>, // Compressed properties
}

/// Adjacency entry for fast relationship lookup
#[derive(Debug, Clone, Copy)]
pub struct AdjacencyEntry {
    pub relationship_id: u64,
    pub neighbor_id: u64,
    pub type_id: u32,
    pub weight: Option<f64>,
}

/// Compressed adjacency list
#[derive(Debug, Clone)]
pub struct CompressedAdjacencyList {
    pub entries: Vec<AdjacencyEntry>,
    pub compression_ratio: f64,
}

/// Relationship type information for optimization
#[derive(Debug, Clone)]
pub struct RelationshipTypeInfo {
    pub type_id: u32,
    pub name: String,
    pub relationship_count: usize,
    pub avg_properties_size: usize,
    pub compression_savings: f64,
}

/// Statistics for relationship processing optimization
#[derive(Debug, Clone, Default)]
pub struct RelationshipStats {
    pub total_relationships: usize,
    pub total_adjacency_entries: usize,
    pub memory_usage_bytes: usize,
    pub compression_ratio: f64,
    pub avg_lookup_time_ns: u64,
    pub cache_hit_rate: f64,
}

/// Relationship Storage Manager - Core component for Phase 8.1
pub struct RelationshipStorageManager {
    // Type-specific relationship stores
    type_stores: HashMap<u32, Arc<ParkingRwLock<TypeRelationshipStore>>>,
    // Global relationship metadata
    metadata: Arc<ParkingRwLock<RelationshipMetadata>>,
    // Compression manager
    compression: RelationshipCompressionManager,
    // Statistics
    stats: Arc<ParkingRwLock<RelationshipStats>>,
}

impl RelationshipStorageManager {
    pub fn new() -> Self {
        Self {
            type_stores: HashMap::new(),
            metadata: Arc::new(ParkingRwLock::new(RelationshipMetadata::new())),
            compression: RelationshipCompressionManager::new(),
            stats: Arc::new(ParkingRwLock::new(RelationshipStats::default())),
        }
    }

    /// Create a new relationship with optimized storage
    pub fn create_relationship(
        &mut self,
        source_id: u64,
        target_id: u64,
        type_id: u32,
        properties: HashMap<String, Value>,
    ) -> Result<u64, RelationshipStorageError> {
        // Generate relationship ID
        let rel_id = self.generate_relationship_id()?;

        // Compress properties
        let compressed_props = self.compression.compress_properties(&properties)?;

        // Create relationship record
        let relationship = RelationshipRecord {
            id: rel_id,
            source_id,
            target_id,
            type_id,
            properties: compressed_props,
        };

        // Get or create type-specific store
        let type_store = self.get_or_create_type_store(type_id);

        // Store relationship
        {
            let mut store = type_store.write();
            store.store_relationship(relationship)?;
        }

        // Update metadata
        {
            let mut metadata = self.metadata.write();
            metadata.add_relationship(type_id, rel_id, source_id, target_id);
        }

        // Update statistics
        self.update_stats_on_create();

        Ok(rel_id)
    }

    /// Get relationships for a node with optimized lookup
    pub fn get_relationships(
        &self,
        node_id: u64,
        direction: Direction,
        type_filter: Option<u32>,
    ) -> Result<Vec<RelationshipRecord>, RelationshipStorageError> {
        let start_time = std::time::Instant::now();

        let mut results = Vec::new();

        // Determine which type stores to query
        let type_ids = if let Some(type_id) = type_filter {
            vec![type_id]
        } else {
            self.metadata.read().get_all_type_ids()
        };

        for &type_id in &type_ids {
            if let Some(type_store) = self.type_stores.get(&type_id) {
                let store = type_store.read();

                // Get relationships based on direction
                let rel_ids = match direction {
                    Direction::Outgoing => store.get_outgoing_relationships(node_id),
                    Direction::Incoming => store.get_incoming_relationships(node_id),
                    Direction::Both => {
                        let mut all = store.get_outgoing_relationships(node_id);
                        all.extend(store.get_incoming_relationships(node_id));
                        all
                    }
                };

                // Load full relationship records
                for &rel_id in &rel_ids {
                    if let Some(rel) = store.get_relationship(rel_id)? {
                        results.push(rel);
                    }
                }
            }
        }

        // Update statistics
        let lookup_time = start_time.elapsed().as_nanos() as u64;
        self.update_stats_on_lookup(lookup_time);

        Ok(results)
    }

    /// Delete relationship with cleanup
    pub fn delete_relationship(&self, rel_id: u64) -> Result<(), RelationshipStorageError> {
        // Find which type store contains this relationship
        for type_store in self.type_stores.values() {
            let mut store = type_store.write();
            if store.delete_relationship(rel_id)? {
                // Update metadata
                let mut metadata = self.metadata.write();
                metadata.remove_relationship(rel_id);

                // Update statistics
                self.update_stats_on_delete();
                return Ok(());
            }
        }

        Err(RelationshipStorageError::RelationshipNotFound(rel_id))
    }

    /// Get optimized adjacency list for traversals
    pub fn get_adjacency_list(
        &self,
        node_id: u64,
        direction: Direction,
        type_filter: Option<u32>,
    ) -> Result<CompressedAdjacencyList, RelationshipStorageError> {
        let mut entries = Vec::new();

        let type_ids = if let Some(type_id) = type_filter {
            vec![type_id]
        } else {
            self.metadata.read().get_all_type_ids()
        };

        for &type_id in &type_ids {
            if let Some(type_store) = self.type_stores.get(&type_id) {
                let store = type_store.read();

                let adj_entries = match direction {
                    Direction::Outgoing => store.get_outgoing_adjacency(node_id),
                    Direction::Incoming => store.get_incoming_adjacency(node_id),
                    Direction::Both => {
                        let mut all = store.get_outgoing_adjacency(node_id);
                        all.extend(store.get_incoming_adjacency(node_id));
                        all
                    }
                };

                entries.extend(adj_entries);
            }
        }

        let compression_ratio = self.compression.calculate_compression_ratio(&entries);

        Ok(CompressedAdjacencyList {
            entries,
            compression_ratio,
        })
    }

    // Internal helper methods
    fn generate_relationship_id(&self) -> Result<u64, RelationshipStorageError> {
        // Simple ID generation - in production would use proper ID management
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RelationshipStorageError::IdGenerationFailed)?
            .as_nanos() as u64;

        // Add some randomness to avoid collisions
        let random_part = (timestamp % 1000000) as u64;
        Ok(timestamp + random_part)
    }

    fn get_or_create_type_store(
        &mut self,
        type_id: u32,
    ) -> Arc<ParkingRwLock<TypeRelationshipStore>> {
        self.type_stores
            .entry(type_id)
            .or_insert_with(|| Arc::new(ParkingRwLock::new(TypeRelationshipStore::new(type_id))))
            .clone()
    }

    fn update_stats_on_create(&self) {
        let mut stats = self.stats.write();
        stats.total_relationships += 1;
        stats.memory_usage_bytes += 256; // Rough estimate per relationship
    }

    fn update_stats_on_lookup(&self, lookup_time_ns: u64) {
        let mut stats = self.stats.write();
        stats.total_adjacency_entries += 1; // Approximation
        // Simple moving average for lookup time
        stats.avg_lookup_time_ns = (stats.avg_lookup_time_ns + lookup_time_ns) / 2;
    }

    fn update_stats_on_delete(&self) {
        let mut stats = self.stats.write();
        if stats.total_relationships > 0 {
            stats.total_relationships -= 1;
        }
    }

    /// Get current statistics
    pub fn get_stats(&self) -> RelationshipStats {
        self.stats.read().clone()
    }
}

/// Type-specific relationship store
pub struct TypeRelationshipStore {
    type_id: u32,
    relationships: HashMap<u64, RelationshipRecord>,
    outgoing_adjacency: HashMap<u64, Vec<u64>>, // node_id -> [rel_id]
    incoming_adjacency: HashMap<u64, Vec<u64>>, // node_id -> [rel_id]
}

impl TypeRelationshipStore {
    pub fn new(type_id: u32) -> Self {
        Self {
            type_id,
            relationships: HashMap::new(),
            outgoing_adjacency: HashMap::new(),
            incoming_adjacency: HashMap::new(),
        }
    }

    pub fn store_relationship(
        &mut self,
        rel: RelationshipRecord,
    ) -> Result<(), RelationshipStorageError> {
        let rel_id = rel.id;
        let source_id = rel.source_id;
        let target_id = rel.target_id;

        // Store the relationship
        self.relationships.insert(rel_id, rel);

        // Update adjacency lists
        self.outgoing_adjacency
            .entry(source_id)
            .or_default()
            .push(rel_id);

        self.incoming_adjacency
            .entry(target_id)
            .or_default()
            .push(rel_id);

        Ok(())
    }

    pub fn get_relationship(
        &self,
        rel_id: u64,
    ) -> Result<Option<RelationshipRecord>, RelationshipStorageError> {
        Ok(self.relationships.get(&rel_id).cloned())
    }

    pub fn delete_relationship(&mut self, rel_id: u64) -> Result<bool, RelationshipStorageError> {
        if let Some(rel) = self.relationships.remove(&rel_id) {
            // Remove from adjacency lists
            self.outgoing_adjacency
                .entry(rel.source_id)
                .or_default()
                .retain(|&id| id != rel_id);

            self.incoming_adjacency
                .entry(rel.target_id)
                .or_default()
                .retain(|&id| id != rel_id);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get_outgoing_relationships(&self, node_id: u64) -> Vec<u64> {
        self.outgoing_adjacency
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_incoming_relationships(&self, node_id: u64) -> Vec<u64> {
        self.incoming_adjacency
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_outgoing_adjacency(&self, node_id: u64) -> Vec<AdjacencyEntry> {
        self.outgoing_adjacency
            .get(&node_id)
            .map(|rel_ids| {
                rel_ids
                    .iter()
                    .filter_map(|&rel_id| {
                        self.relationships.get(&rel_id).map(|rel| AdjacencyEntry {
                            relationship_id: rel_id,
                            neighbor_id: rel.target_id,
                            type_id: self.type_id,
                            weight: None, // For now
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_incoming_adjacency(&self, node_id: u64) -> Vec<AdjacencyEntry> {
        self.incoming_adjacency
            .get(&node_id)
            .map(|rel_ids| {
                rel_ids
                    .iter()
                    .filter_map(|&rel_id| {
                        self.relationships.get(&rel_id).map(|rel| AdjacencyEntry {
                            relationship_id: rel_id,
                            neighbor_id: rel.source_id,
                            type_id: self.type_id,
                            weight: None, // For now
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Relationship metadata for optimization
pub struct RelationshipMetadata {
    type_info: HashMap<u32, RelationshipTypeInfo>,
    relationship_locations: HashMap<u64, (u32, u64, u64)>, // rel_id -> (type_id, source_id, target_id)
}

impl RelationshipMetadata {
    pub fn new() -> Self {
        Self {
            type_info: HashMap::new(),
            relationship_locations: HashMap::new(),
        }
    }

    pub fn add_relationship(&mut self, type_id: u32, rel_id: u64, source_id: u64, target_id: u64) {
        // Update type info
        let type_info = self
            .type_info
            .entry(type_id)
            .or_insert_with(|| RelationshipTypeInfo {
                type_id,
                name: format!("type_{}", type_id), // Would be resolved from schema
                relationship_count: 0,
                avg_properties_size: 0,
                compression_savings: 0.0,
            });
        type_info.relationship_count += 1;

        // Store location
        self.relationship_locations
            .insert(rel_id, (type_id, source_id, target_id));
    }

    pub fn remove_relationship(&mut self, rel_id: u64) {
        if let Some((type_id, _, _)) = self.relationship_locations.remove(&rel_id) {
            if let Some(type_info) = self.type_info.get_mut(&type_id) {
                if type_info.relationship_count > 0 {
                    type_info.relationship_count -= 1;
                }
            }
        }
    }

    pub fn get_all_type_ids(&self) -> Vec<u32> {
        self.type_info.keys().cloned().collect()
    }

    pub fn get_type_info(&self, type_id: u32) -> Option<&RelationshipTypeInfo> {
        self.type_info.get(&type_id)
    }
}

/// Relationship compression manager
pub struct RelationshipCompressionManager {
    // Compression algorithms
}

impl RelationshipCompressionManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn compress_properties(
        &self,
        properties: &HashMap<String, Value>,
    ) -> Result<Vec<u8>, RelationshipStorageError> {
        // Simple JSON serialization for now - would implement proper compression
        let json = serde_json::to_string(properties)
            .map_err(|_| RelationshipStorageError::CompressionFailed)?;
        Ok(json.into_bytes())
    }

    pub fn decompress_properties(
        &self,
        data: &[u8],
    ) -> Result<HashMap<String, Value>, RelationshipStorageError> {
        let json =
            std::str::from_utf8(data).map_err(|_| RelationshipStorageError::DecompressionFailed)?;
        serde_json::from_str(json).map_err(|_| RelationshipStorageError::DecompressionFailed)
    }

    pub fn calculate_compression_ratio(&self, entries: &[AdjacencyEntry]) -> f64 {
        // Simple calculation - would implement proper compression analysis
        let uncompressed_size = entries.len() * std::mem::size_of::<AdjacencyEntry>();
        let compressed_size = uncompressed_size / 2; // Rough estimate
        uncompressed_size as f64 / compressed_size as f64
    }
}

/// Relationship storage errors
#[derive(Debug, thiserror::Error)]
pub enum RelationshipStorageError {
    #[error("Relationship not found: {0}")]
    RelationshipNotFound(u64),

    #[error("ID generation failed")]
    IdGenerationFailed,

    #[error("Compression failed")]
    CompressionFailed,

    #[error("Decompression failed")]
    DecompressionFailed,

    #[error("Storage operation failed")]
    StorageOperationFailed,
}
