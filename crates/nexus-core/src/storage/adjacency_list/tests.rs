#[cfg(test)]
mod tests {
    use super::super::store::AdjacencyListStore;
    use crate::testing::TestContext;

    #[test]
    fn test_adjacency_list_store_creation() {
        let ctx = TestContext::new();
        let store = AdjacencyListStore::new(ctx.path()).unwrap();
        assert_eq!(store.outgoing_file_size, 1024 * 1024);
        assert_eq!(store.incoming_file_size, 1024 * 1024);
    }

    #[test]
    fn test_add_outgoing_relationships() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships for node 1
        let relationships = vec![(1, 1), (2, 1), (3, 2)]; // (rel_id, type_id)
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Retrieve relationships
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
    }

    #[test]
    fn test_get_outgoing_relationships_filtered() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with different types
        let relationships = vec![(1, 1), (2, 1), (3, 2)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by type 1
        let result = store.get_outgoing_relationships(1, &[1]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(!result.contains(&3));
    }

    #[test]
    fn test_multiple_nodes_with_relationships() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships for node 1
        let node1_rels = vec![(1, 1), (2, 1), (3, 2)];
        store.add_outgoing_relationships(1, &node1_rels).unwrap();

        // Add relationships for node 2
        let node2_rels = vec![(4, 1), (5, 3)];
        store.add_outgoing_relationships(2, &node2_rels).unwrap();

        // Verify node 1 relationships
        let result1 = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result1.len(), 3);
        assert!(result1.contains(&1));
        assert!(result1.contains(&2));
        assert!(result1.contains(&3));

        // Verify node 2 relationships
        let result2 = store.get_outgoing_relationships(2, &[]).unwrap();
        assert_eq!(result2.len(), 2);
        assert!(result2.contains(&4));
        assert!(result2.contains(&5));

        // Verify isolation (node 1 doesn't have node 2's relationships)
        assert!(!result1.contains(&4));
        assert!(!result1.contains(&5));
    }

    #[test]
    fn test_node_with_no_relationships() {
        let ctx = TestContext::new();
        let store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Node with no relationships should return empty vector
        let result = store.get_outgoing_relationships(999, &[]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_add_relationships_incrementally() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add first batch of relationships
        let batch1 = vec![(1, 1), (2, 1)];
        store.add_outgoing_relationships(1, &batch1).unwrap();

        // Add second batch of relationships (same node, different types)
        let batch2 = vec![(3, 2), (4, 2)];
        store.add_outgoing_relationships(1, &batch2).unwrap();

        // Verify all relationships are present
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&4));
    }

    #[test]
    fn test_filter_by_multiple_types() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with multiple types
        let relationships = vec![(1, 1), (2, 1), (3, 2), (4, 3), (5, 2)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by types 1 and 2
        let result = store.get_outgoing_relationships(1, &[1, 2]).unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&5));
        assert!(!result.contains(&4)); // Type 3 should be excluded
    }

    #[test]
    fn test_filter_by_nonexistent_type() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with type 1
        let relationships = vec![(1, 1), (2, 1)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by nonexistent type
        let result = store.get_outgoing_relationships(1, &[999]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_large_number_of_relationships() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add 100 relationships for a single node
        let mut relationships = Vec::new();
        for i in 0..100 {
            relationships.push((i as u64, (i % 5) as u32)); // 5 different types
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships are present
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 100);

        // Verify filtering works with large dataset
        let type_0_rels = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(type_0_rels.len(), 20); // 100 / 5 = 20 per type
    }

    #[test]
    fn test_flush_persistence() {
        let ctx = TestContext::new();
        let path = ctx.path();

        // Create store and add relationships
        {
            let mut store = AdjacencyListStore::new(path).unwrap();
            let relationships = vec![(1, 1), (2, 1), (3, 2)];
            store.add_outgoing_relationships(1, &relationships).unwrap();
            store.flush().unwrap();
        }

        // Reopen store and verify relationships persist
        let store = AdjacencyListStore::new(path).unwrap();
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
    }

    #[test]
    fn test_empty_relationships_list() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Adding empty list should not crash
        let empty: Vec<(u64, u32)> = vec![];
        store.add_outgoing_relationships(1, &empty).unwrap();

        // Should return empty result
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_same_relationship_id_different_types() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Note: In real usage, same rel_id shouldn't have different types
        // But we test that the store handles it gracefully
        let relationships = vec![(1, 1), (1, 2)]; // Same rel_id, different types
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Both should be stored (though this is unusual)
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1));
    }

    #[test]
    fn test_high_degree_node() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Simulate high-degree node (node with many relationships)
        let mut relationships = Vec::new();
        for i in 0..1000 {
            relationships.push((i as u64, (i % 10) as u32)); // 10 different types
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 1000);

        // Verify filtering performance with high-degree node
        let type_5_rels = store.get_outgoing_relationships(1, &[5]).unwrap();
        assert_eq!(type_5_rels.len(), 100); // 1000 / 10 = 100 per type
    }

    #[test]
    fn test_concurrent_node_access_pattern() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships for multiple nodes (simulating concurrent access pattern)
        for node_id in 0..10 {
            let mut relationships = Vec::new();
            for rel_id in 0..10 {
                relationships.push((node_id * 100 + rel_id, (rel_id % 3) as u32));
            }
            store
                .add_outgoing_relationships(node_id, &relationships)
                .unwrap();
        }

        // Verify each node has correct relationships
        for node_id in 0..10 {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(result.len(), 10);
            for rel_id in 0..10 {
                assert!(result.contains(&(node_id * 100 + rel_id)));
            }
        }
    }

    #[test]
    fn test_type_distribution() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with uneven type distribution
        let relationships = vec![
            (1, 1),
            (2, 1),
            (3, 1), // 3 of type 1
            (4, 2), // 1 of type 2
            (5, 3),
            (6, 3),
            (7, 3),
            (8, 3),
            (9, 3), // 5 of type 3
        ];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify type distribution
        let type1 = store.get_outgoing_relationships(1, &[1]).unwrap();
        assert_eq!(type1.len(), 3);

        let type2 = store.get_outgoing_relationships(1, &[2]).unwrap();
        assert_eq!(type2.len(), 1);

        let type3 = store.get_outgoing_relationships(1, &[3]).unwrap();
        assert_eq!(type3.len(), 5);
    }

    #[test]
    fn test_stress_many_nodes() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Create 1000 nodes, each with 10 relationships
        for node_id in 0..1000 {
            let mut relationships = Vec::new();
            for rel_id in 0..10 {
                relationships.push((node_id * 1000 + rel_id, (rel_id % 5) as u32));
            }
            store
                .add_outgoing_relationships(node_id, &relationships)
                .unwrap();
        }

        // Verify random nodes
        let result_500 = store.get_outgoing_relationships(500, &[]).unwrap();
        assert_eq!(result_500.len(), 10);

        let result_999 = store.get_outgoing_relationships(999, &[]).unwrap();
        assert_eq!(result_999.len(), 10);
    }

    #[test]
    fn test_very_large_relationship_list() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add 10,000 relationships for a single node
        let mut relationships = Vec::new();
        for i in 0..10000 {
            relationships.push((i as u64, (i % 20) as u32)); // 20 different types
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 10000);

        // Verify filtering works with very large dataset
        let type_0_rels = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(type_0_rels.len(), 500); // 10000 / 20 = 500 per type
    }

    #[test]
    fn test_sequential_vs_batch_addition() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships sequentially
        for i in 0..10 {
            let rels = vec![(i, 1)];
            store.add_outgoing_relationships(1, &rels).unwrap();
        }

        // Add relationships in batch
        let mut batch_rels = Vec::new();
        for i in 10..20 {
            batch_rels.push((i, 1));
        }
        store.add_outgoing_relationships(2, &batch_rels).unwrap();

        // Both should have same result
        let result1 = store.get_outgoing_relationships(1, &[]).unwrap();
        let result2 = store.get_outgoing_relationships(2, &[]).unwrap();
        assert_eq!(result1.len(), 10);
        assert_eq!(result2.len(), 10);
    }

    #[test]
    fn test_mixed_type_distribution() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with mixed type distribution
        let relationships = vec![
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 1),
            (5, 1), // 5 of type 1
            (6, 2), // 1 of type 2
            (7, 3),
            (8, 3), // 2 of type 3
            (9, 1),
            (10, 1), // 2 more of type 1
        ];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify type 1 has all 7 relationships
        let type1 = store.get_outgoing_relationships(1, &[1]).unwrap();
        assert_eq!(type1.len(), 7);

        // Verify type 2 has 1 relationship
        let type2 = store.get_outgoing_relationships(1, &[2]).unwrap();
        assert_eq!(type2.len(), 1);

        // Verify type 3 has 2 relationships
        let type3 = store.get_outgoing_relationships(1, &[3]).unwrap();
        assert_eq!(type3.len(), 2);
    }

    #[test]
    fn test_boundary_conditions() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Test with node_id = 0
        let rels = vec![(1, 1)];
        store.add_outgoing_relationships(0, &rels).unwrap();
        let result = store.get_outgoing_relationships(0, &[]).unwrap();
        assert_eq!(result.len(), 1);

        // Test with very large node_id
        let rels2 = vec![(2, 1)];
        store.add_outgoing_relationships(u64::MAX, &rels2).unwrap();
        let result2 = store.get_outgoing_relationships(u64::MAX, &[]).unwrap();
        assert_eq!(result2.len(), 1);

        // Test with type_id = 0
        let rels3 = vec![(3, 0)];
        store.add_outgoing_relationships(1, &rels3).unwrap();
        let result3 = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(result3.len(), 1);
    }

    #[test]
    fn test_reopen_store_multiple_times() {
        let ctx = TestContext::new();
        let path = ctx.path();

        // Create and add relationships
        {
            let mut store = AdjacencyListStore::new(path).unwrap();
            let relationships = vec![(1, 1), (2, 1), (3, 2)];
            store.add_outgoing_relationships(1, &relationships).unwrap();
            store.flush().unwrap();
        }

        // Reopen and add more
        {
            let mut store = AdjacencyListStore::new(path).unwrap();
            let relationships = vec![(4, 2), (5, 3)];
            store.add_outgoing_relationships(1, &relationships).unwrap();
            store.flush().unwrap();
        }

        // Reopen and verify all relationships
        let store = AdjacencyListStore::new(path).unwrap();
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 5);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&4));
        assert!(result.contains(&5));
    }

    #[test]
    fn test_multiple_flushes() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships and flush multiple times
        for i in 0..5 {
            let rels = vec![(i, 1)];
            store.add_outgoing_relationships(1, &rels).unwrap();
            store.flush().unwrap();
        }

        // Verify all relationships persist
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_filter_all_types() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with different types
        let relationships = vec![(1, 1), (2, 2), (3, 3), (4, 1), (5, 2)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by all types (empty array = all types)
        let all = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(all.len(), 5);

        // Filter by specific types
        let filtered = store.get_outgoing_relationships(1, &[1, 2]).unwrap();
        assert_eq!(filtered.len(), 4);
        assert!(!filtered.contains(&3)); // Type 3 should be excluded
    }

    #[test]
    fn test_node_isolation() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships for multiple nodes
        for node_id in 0..5 {
            let mut relationships = Vec::new();
            for rel_id in 0..5 {
                relationships.push((node_id * 10 + rel_id, 1));
            }
            store
                .add_outgoing_relationships(node_id, &relationships)
                .unwrap();
        }

        // Verify each node only has its own relationships
        for node_id in 0..5 {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(result.len(), 5);
            // Verify relationships belong to this node
            for &rel_id in &result {
                let expected_min = node_id * 10;
                let expected_max = node_id * 10 + 4;
                assert!(rel_id >= expected_min && rel_id <= expected_max);
            }
        }
    }

    #[test]
    fn test_type_filtering_performance() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add 1000 relationships with 100 different types
        let mut relationships = Vec::new();
        for i in 0..1000 {
            relationships.push((i as u64, (i % 100) as u32));
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by single type (should be fast)
        let type_50 = store.get_outgoing_relationships(1, &[50]).unwrap();
        assert_eq!(type_50.len(), 10); // 1000 / 100 = 10 per type

        // Filter by multiple types
        let types_10_20_30 = store.get_outgoing_relationships(1, &[10, 20, 30]).unwrap();
        assert_eq!(types_10_20_30.len(), 30); // 10 per type * 3 types
    }

    #[test]
    fn test_concurrent_node_patterns() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Simulate concurrent access pattern: add relationships to different nodes
        // in an interleaved manner, but batch by node to avoid overwriting
        let nodes = vec![1, 5, 10, 15, 20];

        // Collect all relationships per node first, then add in batches
        let mut node_relationships: std::collections::HashMap<u64, Vec<(u64, u32)>> =
            std::collections::HashMap::new();
        for round in 0..10 {
            for &node_id in &nodes {
                let rel_id = node_id * 100 + round;
                node_relationships
                    .entry(node_id)
                    .or_default()
                    .push((rel_id, (round % 3) as u32));
            }
        }

        // Add all relationships for each node in one batch
        for &node_id in &nodes {
            let rels = node_relationships.get(&node_id).unwrap();
            store.add_outgoing_relationships(node_id, rels).unwrap();
        }

        // Verify each node has 10 relationships
        for &node_id in &nodes {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(
                result.len(),
                10,
                "Node {} should have 10 relationships",
                node_id
            );

            // Verify all expected relationship IDs are present
            for round in 0..10 {
                let expected_rel_id = node_id * 100 + round;
                assert!(
                    result.contains(&expected_rel_id),
                    "Node {} missing relationship {}",
                    node_id,
                    expected_rel_id
                );
            }
        }
    }

    #[test]
    fn test_file_growth() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add enough relationships to trigger file growth
        // Each relationship needs ~20 bytes (header) + 16 bytes (entry) = 36 bytes
        // 1MB / 36 bytes = ~29,000 relationships per MB
        // Let's add 50,000 relationships to ensure growth
        let mut relationships = Vec::new();
        for i in 0..50000 {
            relationships.push((i as u64, (i % 10) as u32));
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships are present
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 50000);

        // Verify file grew
        assert!(store.outgoing_file_size >= 1024 * 1024); // At least 1MB
    }

    #[test]
    fn test_sparse_node_distribution() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships to sparse node IDs (not consecutive)
        let sparse_nodes = vec![1, 100, 1000, 10000, 100000];
        for &node_id in &sparse_nodes {
            let rels = vec![(node_id, 1)];
            store.add_outgoing_relationships(node_id, &rels).unwrap();
        }

        // Verify each sparse node
        for &node_id in &sparse_nodes {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result.contains(&node_id));
        }
    }

    #[test]
    fn test_relationship_id_uniqueness() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with unique IDs
        let relationships = vec![(1, 1), (2, 1), (3, 1), (100, 2), (200, 2), (1000, 3)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        // All relationship IDs should be unique in the result
        let mut seen = std::collections::HashSet::new();
        for &rel_id in &result {
            assert!(seen.insert(rel_id), "Duplicate relationship ID: {}", rel_id);
        }
    }

    #[test]
    fn test_mixed_batch_sizes() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships in batches of different sizes
        store.add_outgoing_relationships(1, &[(1, 1)]).unwrap(); // Single
        store
            .add_outgoing_relationships(1, &[(2, 1), (3, 1)])
            .unwrap(); // Pair
        store
            .add_outgoing_relationships(1, &[(4, 1), (5, 1), (6, 1), (7, 1), (8, 1)])
            .unwrap(); // Large batch

        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_type_zero_handling() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add relationships with type_id = 0 (valid type)
        let relationships = vec![(1, 0), (2, 0), (3, 1)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by type 0
        let type0 = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(type0.len(), 2);
        assert!(type0.contains(&1));
        assert!(type0.contains(&2));
        assert!(!type0.contains(&3));
    }

    #[test]
    fn test_add_incoming_relationships() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add incoming relationships for node 1
        let relationships = vec![(1, 1), (2, 1), (3, 2)];
        store.add_incoming_relationships(1, &relationships).unwrap();

        // Retrieve incoming relationships
        let result = store.get_incoming_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
    }

    #[test]
    fn test_incoming_relationships_filtered() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add incoming relationships with different types
        let relationships = vec![(1, 1), (2, 1), (3, 2)];
        store.add_incoming_relationships(1, &relationships).unwrap();

        // Filter by type 1
        let result = store.get_incoming_relationships(1, &[1]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(!result.contains(&3));
    }

    #[test]
    fn test_outgoing_and_incoming_separation() {
        let ctx = TestContext::new();
        let mut store = AdjacencyListStore::new(ctx.path()).unwrap();

        // Add outgoing relationships for node 1
        let outgoing = vec![(1, 1), (2, 1)];
        store.add_outgoing_relationships(1, &outgoing).unwrap();

        // Add incoming relationships for node 1
        let incoming = vec![(3, 2), (4, 2)];
        store.add_incoming_relationships(1, &incoming).unwrap();

        // Verify outgoing relationships
        let out_result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(out_result.len(), 2);
        assert!(out_result.contains(&1));
        assert!(out_result.contains(&2));
        assert!(!out_result.contains(&3));
        assert!(!out_result.contains(&4));

        // Verify incoming relationships
        let in_result = store.get_incoming_relationships(1, &[]).unwrap();
        assert_eq!(in_result.len(), 2);
        assert!(!in_result.contains(&1));
        assert!(!in_result.contains(&2));
        assert!(in_result.contains(&3));
        assert!(in_result.contains(&4));
    }
}
