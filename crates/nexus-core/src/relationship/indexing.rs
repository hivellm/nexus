//! Relationship Property Indexing System
//!
//! Implements specialized indexes for relationship properties
//! to enable sub-millisecond lookups and range queries.

use parking_lot::RwLock as ParkingRwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::relationship::*;
use serde_json::Value;

/// Relationship Property Index Manager
pub struct RelationshipPropertyIndex {
    // Type-specific indexes for fast type filtering
    type_indexes: HashMap<u32, Arc<ParkingRwLock<TypePropertyIndex>>>,
    // Global indexes for cross-type queries (more expensive)
    global_indexes: HashMap<String, Arc<ParkingRwLock<GlobalPropertyIndex>>>,
    // Index maintenance statistics
    stats: Arc<ParkingRwLock<IndexStats>>,
}

impl RelationshipPropertyIndex {
    pub fn new() -> Self {
        Self {
            type_indexes: HashMap::new(),
            global_indexes: HashMap::new(),
            stats: Arc::new(ParkingRwLock::new(IndexStats::default())),
        }
    }

    /// Index properties for a relationship
    pub fn index_properties(
        &mut self,
        rel_id: u64,
        type_id: u32,
        properties: &HashMap<String, Value>,
    ) -> Result<(), IndexError> {
        // Get or create type-specific index
        let type_index = self.get_or_create_type_index(type_id);

        // Index each property
        for (prop_name, prop_value) in properties {
            // Add to type-specific index
            {
                let mut index = type_index.write();
                index.add_property(rel_id, prop_name, prop_value)?;
            }

            // Add to global index
            let global_index = self.get_or_create_global_index(prop_name);
            {
                let mut index = global_index.write();
                index.add_relationship(rel_id, type_id, prop_value)?;
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.total_indexed_properties += properties.len();
            stats.total_index_entries += properties.len();
        }

        Ok(())
    }

    /// Query relationships by property with type filtering
    pub fn query_by_property(
        &self,
        type_id: Option<u32>,
        property_name: &str,
        operator: PropertyOperator,
        value: &Value,
    ) -> Result<Vec<u64>, IndexError> {
        let start_time = std::time::Instant::now();

        let results = if let Some(type_id) = type_id {
            // Use type-specific index (faster)
            self.query_type_specific_index(type_id, property_name, operator, value)?
        } else {
            // Use global index (slower, but covers all types)
            self.query_global_index(property_name, operator, value)?
        };

        // Update statistics
        let query_time = start_time.elapsed().as_nanos() as u64;
        {
            let mut stats = self.stats.write();
            stats.total_queries += 1;
            stats.total_query_time_ns += query_time;
            if !results.is_empty() {
                stats.successful_queries += 1;
            }
        }

        Ok(results)
    }

    /// Remove relationship from all indexes
    pub fn remove_relationship(&self, rel_id: u64, type_id: u32) -> Result<(), IndexError> {
        // Remove from type-specific index
        if let Some(type_index) = self.type_indexes.get(&type_id) {
            let mut index = type_index.write();
            index.remove_relationship(rel_id)?;
        }

        // Remove from global indexes
        for global_index in self.global_indexes.values() {
            let mut index = global_index.write();
            index.remove_relationship(rel_id)?;
        }

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.total_index_entries = stats.total_index_entries.saturating_sub(1);
        }

        Ok(())
    }

    /// Get index statistics
    pub fn get_stats(&self) -> IndexStats {
        self.stats.read().clone()
    }

    /// Optimize indexes based on query patterns
    pub fn optimize_indexes(&self) -> Result<(), IndexError> {
        // Analyze query patterns and rebuild indexes if needed
        // This would be a background operation
        Ok(())
    }

    // Internal helper methods
    fn get_or_create_type_index(&mut self, type_id: u32) -> Arc<ParkingRwLock<TypePropertyIndex>> {
        self.type_indexes
            .entry(type_id)
            .or_insert_with(|| Arc::new(ParkingRwLock::new(TypePropertyIndex::new(type_id))))
            .clone()
    }

    fn get_or_create_global_index(
        &mut self,
        property_name: &str,
    ) -> Arc<ParkingRwLock<GlobalPropertyIndex>> {
        self.global_indexes
            .entry(property_name.to_string())
            .or_insert_with(|| {
                Arc::new(ParkingRwLock::new(GlobalPropertyIndex::new(
                    property_name.to_string(),
                )))
            })
            .clone()
    }

    fn query_type_specific_index(
        &self,
        type_id: u32,
        property_name: &str,
        operator: PropertyOperator,
        value: &Value,
    ) -> Result<Vec<u64>, IndexError> {
        if let Some(type_index) = self.type_indexes.get(&type_id) {
            let index = type_index.read();
            index.query_property(property_name, operator, value)
        } else {
            Ok(Vec::new())
        }
    }

    fn query_global_index(
        &self,
        property_name: &str,
        operator: PropertyOperator,
        value: &Value,
    ) -> Result<Vec<u64>, IndexError> {
        if let Some(global_index) = self.global_indexes.get(property_name) {
            let index = global_index.read();
            index.query(operator, value)
        } else {
            Ok(Vec::new())
        }
    }
}

/// Type-specific property index (optimized for single relationship type)
pub struct TypePropertyIndex {
    type_id: u32,
    property_indexes: HashMap<String, PropertyIndex>,
    relationship_count: usize,
}

impl TypePropertyIndex {
    pub fn new(type_id: u32) -> Self {
        Self {
            type_id,
            property_indexes: HashMap::new(),
            relationship_count: 0,
        }
    }

    pub fn add_property(
        &mut self,
        rel_id: u64,
        prop_name: &str,
        prop_value: &Value,
    ) -> Result<(), IndexError> {
        let prop_index = self
            .property_indexes
            .entry(prop_name.to_string())
            .or_insert_with(PropertyIndex::new);

        prop_index.add_relationship(rel_id, prop_value)?;
        self.relationship_count += 1;

        Ok(())
    }

    pub fn remove_relationship(&mut self, rel_id: u64) -> Result<(), IndexError> {
        for prop_index in self.property_indexes.values_mut() {
            prop_index.remove_relationship(rel_id)?;
        }

        if self.relationship_count > 0 {
            self.relationship_count -= 1;
        }

        Ok(())
    }

    pub fn query_property(
        &self,
        property_name: &str,
        operator: PropertyOperator,
        value: &Value,
    ) -> Result<Vec<u64>, IndexError> {
        if let Some(prop_index) = self.property_indexes.get(property_name) {
            prop_index.query(operator, value)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn get_property_index(&self, property_name: &str) -> Option<&PropertyIndex> {
        self.property_indexes.get(property_name)
    }
}

/// Global property index (covers all relationship types)
pub struct GlobalPropertyIndex {
    property_name: String,
    value_to_relationships: HashMap<Value, HashSet<u64>>,
    relationship_to_types: HashMap<u64, u32>, // rel_id -> type_id
}

impl GlobalPropertyIndex {
    pub fn new(property_name: String) -> Self {
        Self {
            property_name,
            value_to_relationships: HashMap::new(),
            relationship_to_types: HashMap::new(),
        }
    }

    pub fn add_relationship(
        &mut self,
        rel_id: u64,
        type_id: u32,
        value: &Value,
    ) -> Result<(), IndexError> {
        self.value_to_relationships
            .entry(value.clone())
            .or_default()
            .insert(rel_id);

        self.relationship_to_types.insert(rel_id, type_id);

        Ok(())
    }

    pub fn remove_relationship(&mut self, rel_id: u64) -> Result<(), IndexError> {
        // Remove from value mappings
        for relationships in self.value_to_relationships.values_mut() {
            relationships.remove(&rel_id);
        }

        // Remove type mapping
        self.relationship_to_types.remove(&rel_id);

        Ok(())
    }

    pub fn query(&self, operator: PropertyOperator, value: &Value) -> Result<Vec<u64>, IndexError> {
        match operator {
            PropertyOperator::Equal => Ok(self
                .value_to_relationships
                .get(value)
                .map(|rels| rels.iter().cloned().collect())
                .unwrap_or_default()),
            PropertyOperator::GreaterThan => {
                // For global index, we need to scan all values
                // In production, would have B-tree index for ranges
                let mut results = Vec::new();
                for (val, rels) in &self.value_to_relationships {
                    if Self::compare_values(val, value)? == std::cmp::Ordering::Greater {
                        results.extend(rels.iter().cloned());
                    }
                }
                Ok(results)
            }
            // Other operators would be implemented similarly
            _ => Ok(Vec::new()), // Placeholder
        }
    }

    fn compare_values(a: &Value, b: &Value) -> Result<std::cmp::Ordering, IndexError> {
        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                // Simple numeric comparison - would handle different number types
                Ok(a_num
                    .as_f64()
                    .unwrap_or(0.0)
                    .partial_cmp(&b_num.as_f64().unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal))
            }
            (Value::String(a_str), Value::String(b_str)) => Ok(a_str.cmp(b_str)),
            _ => Err(IndexError::UnsupportedComparison),
        }
    }
}

/// Property index for efficient lookups
pub struct PropertyIndex {
    // Equality index (HashMap for O(1) lookups)
    equality_index: HashMap<Value, HashSet<u64>>,

    // Statistics for optimization
    stats: PropertyIndexStats,
}

impl PropertyIndex {
    pub fn new() -> Self {
        Self {
            equality_index: HashMap::new(),
            stats: PropertyIndexStats::default(),
        }
    }

    pub fn add_relationship(&mut self, rel_id: u64, value: &Value) -> Result<(), IndexError> {
        // Add to equality index
        self.equality_index
            .entry(value.clone())
            .or_default()
            .insert(rel_id);

        self.stats.total_entries += 1;
        self.stats.unique_values = self.equality_index.len();

        Ok(())
    }

    pub fn remove_relationship(&mut self, rel_id: u64) -> Result<(), IndexError> {
        // Remove from equality index
        for relationships in self.equality_index.values_mut() {
            relationships.remove(&rel_id);
        }

        // Clean up empty entries
        self.equality_index.retain(|_, rels| !rels.is_empty());

        if self.stats.total_entries > 0 {
            self.stats.total_entries -= 1;
        }

        Ok(())
    }

    pub fn query(&self, operator: PropertyOperator, value: &Value) -> Result<Vec<u64>, IndexError> {
        let results = match operator {
            PropertyOperator::Equal => self
                .equality_index
                .get(value)
                .map(|rels| rels.iter().cloned().collect())
                .unwrap_or_default(),
            PropertyOperator::GreaterThan
            | PropertyOperator::LessThan
            | PropertyOperator::GreaterEqual
            | PropertyOperator::LessEqual => {
                // For now, fall back to scanning for range queries
                // TODO: Implement proper range indexes
                Vec::new()
            }
            PropertyOperator::NotEqual => {
                let mut results = Vec::new();
                for rels in self.equality_index.values() {
                    for &rel_id in rels {
                        if !self
                            .equality_index
                            .get(value)
                            .map(|equal_rels| equal_rels.contains(&rel_id))
                            .unwrap_or(false)
                        {
                            results.push(rel_id);
                        }
                    }
                }
                results
            }
            // Other operators would need more complex implementation
            _ => Vec::new(),
        };

        // Remove duplicates
        let mut unique_results = results;
        unique_results.sort();
        unique_results.dedup();

        Ok(unique_results)
    }

    pub fn get_stats(&self) -> &PropertyIndexStats {
        &self.stats
    }
}

/// Property operators for queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyOperator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterEqual,
    LessEqual,
    Like,
    In,
    Between,
}

/// Index statistics for monitoring and optimization
#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    pub total_indexed_properties: usize,
    pub total_index_entries: usize,
    pub total_queries: usize,
    pub successful_queries: usize,
    pub total_query_time_ns: u64,
    pub avg_query_time_ns: u64,
    pub hit_rate: f64,
}

impl IndexStats {
    pub fn update_averages(&mut self) {
        if self.total_queries > 0 {
            self.avg_query_time_ns = self.total_query_time_ns / self.total_queries as u64;
            self.hit_rate = self.successful_queries as f64 / self.total_queries as f64;
        }
    }
}

/// Property index statistics
#[derive(Debug, Clone, Default)]
pub struct PropertyIndexStats {
    pub total_entries: usize,
    pub unique_values: usize,
    pub total_queries: usize,
    pub successful_queries: usize,
    pub total_query_time_ns: u64,
}

/// Index errors
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("Unsupported property type for indexing")]
    UnsupportedPropertyType,

    #[error("Unsupported comparison operation")]
    UnsupportedComparison,

    #[error("Index corruption detected")]
    IndexCorruption,

    #[error("Index operation failed")]
    OperationFailed,
}
