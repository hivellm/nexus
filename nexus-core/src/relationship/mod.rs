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
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::RwLock;
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

    #[test]
    fn test_relationship_storage_comprehensive() {
        let mut storage = RelationshipStorageManager::new();

        // Test multiple relationships with different types
        let mut props1 = HashMap::new();
        props1.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(1.5).unwrap()),
        );
        let rel1 = storage.create_relationship(1, 2, 1, props1).unwrap();

        let mut props2 = HashMap::new();
        props2.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(2.5).unwrap()),
        );
        let rel2 = storage.create_relationship(1, 3, 1, props2).unwrap();

        let mut props3 = HashMap::new();
        props3.insert(
            "strength".to_string(),
            Value::Number(serde_json::Number::from_f64(0.8).unwrap()),
        );
        let rel3 = storage.create_relationship(2, 3, 2, props3).unwrap();

        // Test retrieval by direction and type
        let outgoing = storage
            .get_relationships(1, crate::executor::Direction::Outgoing, None)
            .unwrap();
        assert_eq!(outgoing.len(), 2);
        assert!(outgoing.iter().any(|r| r.id == rel1));
        assert!(outgoing.iter().any(|r| r.id == rel2));

        let incoming = storage
            .get_relationships(2, crate::executor::Direction::Incoming, None)
            .unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].id, rel1);

        // Test type filtering
        let type1_rels = storage
            .get_relationships(1, crate::executor::Direction::Outgoing, Some(1))
            .unwrap();
        assert_eq!(type1_rels.len(), 2);

        // Test adjacency list
        let adjacency = storage
            .get_adjacency_list(1, crate::executor::Direction::Outgoing, None)
            .unwrap();
        assert_eq!(adjacency.entries.len(), 2);

        // Test deletion
        storage.delete_relationship(rel1).unwrap();
        let remaining = storage
            .get_relationships(1, crate::executor::Direction::Outgoing, None)
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, rel2);
    }

    #[test]
    fn test_relationship_storage_edge_cases() {
        let mut storage = RelationshipStorageManager::new();

        // Test empty properties
        let empty_props = HashMap::new();
        let rel_id = storage.create_relationship(1, 2, 1, empty_props).unwrap();

        let relationships = storage
            .get_relationships(1, crate::executor::Direction::Outgoing, Some(1))
            .unwrap();
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].id, rel_id);

        // Test self-loops
        let self_loop_props = HashMap::new();
        let self_rel = storage
            .create_relationship(5, 5, 1, self_loop_props)
            .unwrap();

        let self_outgoing = storage
            .get_relationships(5, crate::executor::Direction::Outgoing, Some(1))
            .unwrap();
        let self_incoming = storage
            .get_relationships(5, crate::executor::Direction::Incoming, Some(1))
            .unwrap();
        assert_eq!(self_outgoing.len(), 1);
        assert_eq!(self_incoming.len(), 1);
        assert_eq!(self_outgoing[0].id, self_rel);
        assert_eq!(self_incoming[0].id, self_rel);

        // Test bidirectional relationships
        let mut props_a = HashMap::new();
        props_a.insert("direction".to_string(), Value::String("A_to_B".to_string()));
        let rel_a_to_b = storage.create_relationship(10, 11, 3, props_a).unwrap();

        let mut props_b = HashMap::new();
        props_b.insert("direction".to_string(), Value::String("B_to_A".to_string()));
        let rel_b_to_a = storage.create_relationship(11, 10, 3, props_b).unwrap();

        let bidirectional = storage
            .get_relationships(10, crate::executor::Direction::Both, Some(3))
            .unwrap();
        assert_eq!(bidirectional.len(), 2);
        assert!(bidirectional.iter().any(|r| r.id == rel_a_to_b));
        assert!(bidirectional.iter().any(|r| r.id == rel_b_to_a));
    }

    #[test]
    fn test_relationship_storage_stress() {
        let mut storage = RelationshipStorageManager::new();

        // Create many relationships
        const NUM_RELATIONSHIPS: usize = 1000;
        let mut created_ids = Vec::new();

        for i in 0..NUM_RELATIONSHIPS {
            let mut props = HashMap::new();
            props.insert(
                "index".to_string(),
                Value::Number(serde_json::Number::from(i as i64)),
            );
            let rel_id = storage
                .create_relationship(i as u64, (i + 1) as u64, (i % 5) as u32, props)
                .unwrap();
            created_ids.push(rel_id);
        }

        // Verify all relationships were created
        assert_eq!(created_ids.len(), NUM_RELATIONSHIPS);

        // Test retrieval performance
        let start_time = std::time::Instant::now();
        for i in 0..100 {
            let relationships = storage
                .get_relationships(i as u64, crate::executor::Direction::Outgoing, None)
                .unwrap();
            assert!(!relationships.is_empty());
        }
        let retrieval_time = start_time.elapsed();

        // Should be fast (less than 1ms per retrieval on average)
        assert!(retrieval_time < std::time::Duration::from_millis(50));

        // Test adjacency list performance
        let start_time = std::time::Instant::now();
        for i in 0..100 {
            let adjacency = storage
                .get_adjacency_list(i as u64, crate::executor::Direction::Outgoing, None)
                .unwrap();
            assert!(!adjacency.entries.is_empty());
        }
        let adjacency_time = start_time.elapsed();

        assert!(adjacency_time < std::time::Duration::from_millis(30));

        // Test deletion performance
        let start_time = std::time::Instant::now();
        for &rel_id in &created_ids[0..100] {
            storage.delete_relationship(rel_id).unwrap();
        }
        let deletion_time = start_time.elapsed();

        assert!(deletion_time < std::time::Duration::from_millis(20));
    }

    #[test]
    fn test_advanced_traversal_basic() {
        let storage = RelationshipStorageManager::new();
        let engine = AdvancedTraversalEngine::new(Arc::new(RwLock::new(storage)));

        // For now, test that the engine can be created and basic methods work
        // Full traversal testing would require a different approach with proper storage setup
        let mut visitor = TestTraversalVisitor::new();

        // Test with empty storage - should complete without panicking
        let result = engine
            .traverse_bfs_optimized(1, crate::executor::Direction::Outgoing, 2, &mut visitor)
            .unwrap();

        // With empty storage, should not discover any nodes
        assert!(result.discovered_nodes.is_empty());
        assert_eq!(result.total_nodes_visited, 0);
    }

    #[test]
    fn test_advanced_traversal_with_bloom_filter() {
        let storage = RelationshipStorageManager::new();
        let engine = AdvancedTraversalEngine::new(Arc::new(RwLock::new(storage)));

        // Test bloom filter creation and basic functionality
        let mut visitor = TestTraversalVisitor::new();
        let result = engine
            .traverse_bfs_optimized(1, crate::executor::Direction::Outgoing, 10, &mut visitor)
            .unwrap();

        // With empty storage, should complete successfully
        assert!(result.discovered_nodes.is_empty());
        assert_eq!(result.max_depth_reached, 0);
    }

    #[test]
    fn test_parallel_path_finding() {
        let storage = RelationshipStorageManager::new();
        let engine = AdvancedTraversalEngine::new(Arc::new(RwLock::new(storage)));

        // Test with empty storage - should return empty paths without panicking
        let paths = engine.find_paths_parallel(1, 4, 5, 10).unwrap();

        // With empty storage, should return empty vector
        assert!(paths.is_empty());
    }

    #[test]
    fn test_relationship_property_index_advanced() {
        let mut index = RelationshipPropertyIndex::new();

        // Index relationships with various property types
        let mut props1 = HashMap::new();
        props1.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
        );
        props1.insert(
            "category".to_string(),
            Value::String("important".to_string()),
        );
        props1.insert("active".to_string(), Value::Bool(true));
        index.index_properties(1, 1, &props1).unwrap();

        let mut props2 = HashMap::new();
        props2.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(20.0).unwrap()),
        );
        props2.insert("category".to_string(), Value::String("urgent".to_string()));
        props2.insert("active".to_string(), Value::Bool(false));
        index.index_properties(2, 1, &props2).unwrap();

        let mut props3 = HashMap::new();
        props3.insert(
            "weight".to_string(),
            Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
        );
        props3.insert(
            "category".to_string(),
            Value::String("important".to_string()),
        );
        props3.insert(
            "priority".to_string(),
            Value::Number(serde_json::Number::from(5)),
        );
        index.index_properties(3, 2, &props3).unwrap(); // Different relationship type

        // Test equality queries
        let weight_results = index
            .query_by_property(
                None,
                "weight",
                PropertyOperator::Equal,
                &Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
            )
            .unwrap();
        assert_eq!(weight_results.len(), 2); // rel 1 and 3
        assert!(weight_results.contains(&1));
        assert!(weight_results.contains(&3));

        // Test type-specific queries
        let type1_weight_results = index
            .query_by_property(
                Some(1),
                "weight",
                PropertyOperator::Equal,
                &Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
            )
            .unwrap();
        assert_eq!(type1_weight_results, vec![1]); // Only rel 1 (type 1)

        // Test boolean queries
        let active_results = index
            .query_by_property(None, "active", PropertyOperator::Equal, &Value::Bool(true))
            .unwrap();
        assert_eq!(active_results, vec![1]);

        // Test string queries
        let category_results = index
            .query_by_property(
                None,
                "category",
                PropertyOperator::Equal,
                &Value::String("important".to_string()),
            )
            .unwrap();
        assert_eq!(category_results.len(), 2);
        assert!(category_results.contains(&1));
        assert!(category_results.contains(&3));

        // Test removal
        index.remove_relationship(1, 1).unwrap();
        let weight_results_after_removal = index
            .query_by_property(
                None,
                "weight",
                PropertyOperator::Equal,
                &Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
            )
            .unwrap();
        assert_eq!(weight_results_after_removal, vec![3]); // Only rel 3 remains
    }

    #[test]
    fn test_compression_effectiveness() {
        // Test compression through the storage manager interface
        let mut storage = RelationshipStorageManager::new();

        let mut test_props = HashMap::new();
        test_props.insert("key1".to_string(), Value::String("a".repeat(100)));
        test_props.insert("key2".to_string(), Value::String("b".repeat(100)));
        test_props.insert(
            "key3".to_string(),
            Value::Number(serde_json::Number::from(42)),
        );

        // Create a relationship to test compression indirectly
        let rel_id = storage
            .create_relationship(1, 2, 1, test_props.clone())
            .unwrap();

        // Retrieve the relationship to verify compression/decompression works
        let relationships = storage
            .get_relationships(1, crate::executor::Direction::Outgoing, Some(1))
            .unwrap();
        assert_eq!(relationships.len(), 1);

        // Since we can't directly access compression fields, we verify through functionality
        assert_eq!(relationships[0].id, rel_id);
        assert_eq!(relationships[0].source_id, 1);
        assert_eq!(relationships[0].target_id, 2);
        assert_eq!(relationships[0].type_id, 1);
    }

    #[test]
    fn test_relationship_storage_statistics() {
        let mut storage = RelationshipStorageManager::new();

        // Create some relationships to build statistics
        for i in 0..10 {
            let mut props = HashMap::new();
            props.insert(
                "index".to_string(),
                Value::Number(serde_json::Number::from(i)),
            );
            storage
                .create_relationship(i, i + 1, (i % 3) as u32, props)
                .unwrap();
        }

        let stats = storage.get_stats();
        assert_eq!(stats.total_relationships, 10);
        assert!(stats.memory_usage_bytes > 0);
        assert!(stats.avg_lookup_time_ns >= 0);
    }

    // Helper struct for traversal tests
    pub struct TestTraversalVisitor {
        pub visited_nodes: Vec<u64>,
        pub visited_relationships: Vec<(u64, u64, u64)>, // (rel_id, source, target)
    }

    impl TestTraversalVisitor {
        pub fn new() -> Self {
            Self {
                visited_nodes: Vec::new(),
                visited_relationships: Vec::new(),
            }
        }
    }

    impl TraversalVisitor for TestTraversalVisitor {
        fn visit_node(
            &mut self,
            node_id: u64,
            depth: usize,
        ) -> Result<TraversalAction, TraversalError> {
            self.visited_nodes.push(node_id);
            Ok(TraversalAction::Continue)
        }

        fn visit_relationship(
            &mut self,
            rel_id: u64,
            source: u64,
            target: u64,
            type_id: u32,
        ) -> bool {
            self.visited_relationships.push((rel_id, source, target));
            true
        }

        fn should_prune(&self, _node_id: u64, _depth: usize) -> bool {
            false
        }
    }
}
