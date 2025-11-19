//! Relationship Processing Optimization Module
//!
//! This module implements specialized relationship storage and processing
//! to achieve Neo4j parity for relationship-heavy workloads.

pub mod indexing;
pub mod storage;
pub mod traversal;

// Re-exports for easier access
pub use indexing::*;
pub use storage::*;
pub use traversal::*;

use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_relationship_storage_basic() {
        let mut storage = RelationshipStorageManager::new();

        // Create test properties
        let mut props = HashMap::new();
        props.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(1.5).unwrap()),
        );
        props.insert("name".to_string(), Value::String("test_rel".to_string()));

        // Create relationship
        let rel_id = storage.create_relationship(1, 2, 1, props).unwrap();

        // Retrieve relationships
        let relationships = storage
            .get_relationships(1, crate::executor::Direction::Outgoing, Some(1))
            .unwrap();
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].id, rel_id);
        assert_eq!(relationships[0].source_id, 1);
        assert_eq!(relationships[0].target_id, 2);
        assert_eq!(relationships[0].type_id, 1);
    }

    #[test]
    fn test_bloom_filter() {
        let mut bloom = BloomFilter::new(1000, 0.01);

        // Insert some items
        bloom.insert(42);
        bloom.insert(100);
        bloom.insert(999);

        // Check existing items (should return true, but might have false positives)
        assert!(bloom.might_contain(42));
        assert!(bloom.might_contain(100));
        assert!(bloom.might_contain(999));

        // Check non-existing items (should usually return false)
        // Note: Bloom filters can have false positives, so we can't assert false here
        // but in practice, with these parameters, false positive rate should be low
        let false_positives = (0..1000u64)
            .filter(|&i| i != 42 && i != 100 && i != 999 && bloom.might_contain(i))
            .count();

        // With 1000 items and 0.01 false positive rate, expect very few false positives
        assert!(
            false_positives < 50,
            "Too many false positives: {}",
            false_positives
        );
    }

    #[test]
    fn test_property_indexing() {
        let mut index = RelationshipPropertyIndex::new();

        // Index some properties
        let mut props = HashMap::new();
        props.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
        );
        props.insert(
            "category".to_string(),
            Value::String("important".to_string()),
        );

        index.index_properties(1, 1, &props).unwrap();

        // Query by property
        let results = index
            .query_by_property(
                Some(1),
                "weight",
                PropertyOperator::Equal,
                &Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
            )
            .unwrap();
        assert_eq!(results, vec![1]);

        let results = index
            .query_by_property(
                Some(1),
                "category",
                PropertyOperator::Equal,
                &Value::String("important".to_string()),
            )
            .unwrap();
        assert_eq!(results, vec![1]);
    }
}
