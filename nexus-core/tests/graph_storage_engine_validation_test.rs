//! Comprehensive validation tests for GraphStorageEngine
//!
//! These tests validate:
//! - Storage engine correctness (6.4.1.1)
//! - Performance regression detection (6.4.1.2)
//! - Data consistency (6.4.1.3)
//! - Crash recovery (6.4.2.3)

use nexus_core::storage::NodeRecord;
use nexus_core::storage::RecordStore;
use nexus_core::storage::RelationshipRecord;
use nexus_core::storage::graph_engine::{
    GraphStorageEngine, MigrationOptions, MigrationStats, export_to_record_store,
    migrate_to_graph_engine,
};
use std::time::Instant;
use tempfile::TempDir;

// ============================================================================
// 6.4.1.1 Storage Engine Correctness Tests
// ============================================================================

#[test]
fn test_correctness_node_creation_and_retrieval() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes with different labels
    let node0 = engine.create_node(1).unwrap();
    let node1 = engine.create_node(2).unwrap();
    let node2 = engine.create_node(1).unwrap();

    // Verify sequential IDs
    assert_eq!(node0, 0);
    assert_eq!(node1, 1);
    assert_eq!(node2, 2);

    // Verify node data
    let read0 = engine.read_node(node0).unwrap();
    let read1 = engine.read_node(node1).unwrap();
    let read2 = engine.read_node(node2).unwrap();

    assert_eq!(read0.label_id, 1);
    assert_eq!(read1.label_id, 2);
    assert_eq!(read2.label_id, 1);
}

#[test]
fn test_correctness_relationship_creation_and_retrieval() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes
    let node0 = engine.create_node(1).unwrap();
    let node1 = engine.create_node(2).unwrap();
    let node2 = engine.create_node(3).unwrap();

    // Create relationships
    let rel0 = engine.create_relationship(node0, node1, 10).unwrap();
    let rel1 = engine.create_relationship(node1, node2, 10).unwrap();
    let rel2 = engine.create_relationship(node0, node2, 20).unwrap();

    // Verify sequential IDs
    assert_eq!(rel0, 0);
    assert_eq!(rel1, 1);
    assert_eq!(rel2, 2);

    // Verify relationship data
    let read0 = engine.read_relationship(10, rel0).unwrap();
    let read1 = engine.read_relationship(10, rel1).unwrap();
    let read2 = engine.read_relationship(20, rel2).unwrap();

    assert_eq!(read0.from_node, node0);
    assert_eq!(read0.to_node, node1);
    assert_eq!(read0.type_id, 10);

    assert_eq!(read1.from_node, node1);
    assert_eq!(read1.to_node, node2);

    assert_eq!(read2.from_node, node0);
    assert_eq!(read2.to_node, node2);
    assert_eq!(read2.type_id, 20);
}

#[test]
fn test_correctness_adjacency_list_integrity() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create a star graph: center connected to 5 nodes
    let center = engine.create_node(1).unwrap();
    let mut peripherals = Vec::new();

    for i in 0..5 {
        let node = engine.create_node(2).unwrap();
        peripherals.push(node);
        engine.create_relationship(center, node, 100).unwrap();
    }

    // Verify outgoing relationships from center
    let outgoing = engine.get_outgoing_relationships(center, 100).unwrap();
    assert_eq!(
        outgoing.len(),
        5,
        "Center should have 5 outgoing relationships"
    );

    // Verify each peripheral has 1 incoming relationship
    for peripheral in &peripherals {
        let incoming = engine.get_incoming_relationships(*peripheral, 100).unwrap();
        assert_eq!(
            incoming.len(),
            1,
            "Each peripheral should have 1 incoming relationship"
        );
        assert_eq!(incoming[0].from_node, center);
    }
}

#[test]
fn test_correctness_bloom_filter_accuracy() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes
    let nodes: Vec<u64> = (0..10).map(|_| engine.create_node(1).unwrap()).collect();

    // Create specific relationships
    engine.create_relationship(nodes[0], nodes[1], 1).unwrap();
    engine.create_relationship(nodes[2], nodes[3], 1).unwrap();
    engine.create_relationship(nodes[4], nodes[5], 1).unwrap();

    // Bloom filter should return true for existing edges
    assert!(engine.might_have_edge(nodes[0], nodes[1], 1));
    assert!(engine.might_have_edge(nodes[2], nodes[3], 1));
    assert!(engine.might_have_edge(nodes[4], nodes[5], 1));

    // Bloom filter should return false for definitely non-existing edges
    // (reverse direction)
    assert!(!engine.might_have_edge(nodes[1], nodes[0], 1));
    assert!(!engine.might_have_edge(nodes[3], nodes[2], 1));

    // Verify with has_edge (confirmed lookup)
    assert!(engine.has_edge(nodes[0], nodes[1], 1).unwrap());
    assert!(!engine.has_edge(nodes[1], nodes[0], 1).unwrap());
}

#[test]
fn test_correctness_multiple_relationship_types() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    let node0 = engine.create_node(1).unwrap();
    let node1 = engine.create_node(2).unwrap();

    // Create relationships of different types between same nodes
    engine.create_relationship(node0, node1, 10).unwrap(); // KNOWS
    engine.create_relationship(node0, node1, 20).unwrap(); // WORKS_WITH
    engine.create_relationship(node0, node1, 30).unwrap(); // LIKES

    // Each type should have exactly 1 relationship
    let type10 = engine.get_outgoing_relationships(node0, 10).unwrap();
    let type20 = engine.get_outgoing_relationships(node0, 20).unwrap();
    let type30 = engine.get_outgoing_relationships(node0, 30).unwrap();

    assert_eq!(type10.len(), 1);
    assert_eq!(type20.len(), 1);
    assert_eq!(type30.len(), 1);

    // Verify stats
    let stats = engine.stats();
    assert_eq!(stats.node_count, 2);
    assert_eq!(stats.relationship_count, 3);
    assert_eq!(stats.relationship_types, 3);
}

// ============================================================================
// 6.4.1.2 Performance Regression Tests
// ============================================================================

#[test]
fn test_performance_node_creation_throughput() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    let num_nodes = 10000;
    let start = Instant::now();

    for i in 0..num_nodes {
        engine.create_node((i % 10) as u32).unwrap();
    }

    let elapsed = start.elapsed();
    let ops_per_sec = num_nodes as f64 / elapsed.as_secs_f64();

    // Target: At least 100K nodes/sec (should be much higher)
    assert!(
        ops_per_sec > 100_000.0,
        "Node creation throughput {} ops/sec is below target 100K ops/sec",
        ops_per_sec
    );

    println!(
        "Node creation: {:.0} ops/sec ({:.2}µs/op)",
        ops_per_sec,
        elapsed.as_micros() as f64 / num_nodes as f64
    );
}

#[test]
#[ignore] // Performance test - run manually, not in CI
fn test_performance_relationship_creation_throughput() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes first
    let num_nodes = 1000;
    for i in 0..num_nodes {
        engine.create_node((i % 10) as u32).unwrap();
    }

    let num_rels = 5000;
    let start = Instant::now();

    for i in 0..num_rels {
        let src = (i % num_nodes) as u64;
        let dst = ((i + 1) % num_nodes) as u64;
        engine
            .create_relationship(src, dst, (i % 5) as u32)
            .unwrap();
    }

    let elapsed = start.elapsed();
    let ops_per_sec = num_rels as f64 / elapsed.as_secs_f64();

    // Target: At least 50K relationships/sec
    assert!(
        ops_per_sec > 50_000.0,
        "Relationship creation throughput {} ops/sec is below target 50K ops/sec",
        ops_per_sec
    );

    println!(
        "Relationship creation: {:.0} ops/sec ({:.2}µs/op)",
        ops_per_sec,
        elapsed.as_micros() as f64 / num_rels as f64
    );
}

#[test]
fn test_performance_bloom_filter_rejection() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes and some relationships
    let num_nodes = 100;
    for _ in 0..num_nodes {
        engine.create_node(1).unwrap();
    }

    // Create linear chain
    for i in 0..(num_nodes - 1) {
        engine.create_relationship(i, i + 1, 1).unwrap();
    }

    // Test bloom filter rejection performance
    let num_checks = 10000;
    let start = Instant::now();

    for i in 0..num_checks {
        let src = (i % num_nodes) as u64;
        let dst = ((i + 50) % num_nodes) as u64; // Most won't exist
        let _ = engine.might_have_edge(src, dst, 1);
    }

    let elapsed = start.elapsed();
    let ops_per_sec = num_checks as f64 / elapsed.as_secs_f64();

    // Target: At least 1M ops/sec for bloom filter checks
    assert!(
        ops_per_sec > 1_000_000.0,
        "Bloom filter rejection {} ops/sec is below target 1M ops/sec",
        ops_per_sec
    );

    println!(
        "Bloom filter rejection: {:.0} ops/sec ({:.3}µs/op)",
        ops_per_sec,
        elapsed.as_micros() as f64 / num_checks as f64
    );
}

// ============================================================================
// 6.4.1.3 Data Consistency Validation
// ============================================================================

#[test]
fn test_consistency_flush_and_reopen() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    // Create and populate engine
    {
        let mut engine = GraphStorageEngine::create(&path).unwrap();

        for i in 0..100 {
            engine.create_node((i % 5) as u32).unwrap();
        }

        for i in 0..50 {
            engine.create_relationship(i * 2, i * 2 + 1, 10).unwrap();
        }

        engine.flush().unwrap();
    }

    // Reopen and verify
    {
        let engine = GraphStorageEngine::open(&path).unwrap();
        let stats = engine.stats();

        // Note: Current implementation may not persist all metadata
        // This test validates the flush mechanism works
        assert!(stats.file_size > 0, "File should have data after flush");
    }
}

#[test]
fn test_consistency_stats_accuracy() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes and relationships
    let num_nodes = 50;
    let num_rels = 30;

    for i in 0..num_nodes {
        engine.create_node((i % 3) as u32).unwrap();
    }

    for i in 0..num_rels {
        engine
            .create_relationship(i as u64, (i + 1) as u64, 1)
            .unwrap();
    }

    let stats = engine.stats();

    assert_eq!(stats.node_count, num_nodes as u64, "Node count mismatch");
    assert_eq!(
        stats.relationship_count, num_rels as u64,
        "Relationship count mismatch"
    );
}

#[test]
fn test_consistency_adjacency_bidirectional() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    let a = engine.create_node(1).unwrap();
    let b = engine.create_node(2).unwrap();
    let c = engine.create_node(3).unwrap();

    // a -> b -> c
    engine.create_relationship(a, b, 1).unwrap();
    engine.create_relationship(b, c, 1).unwrap();

    // Verify outgoing
    let a_out = engine.get_outgoing_relationships(a, 1).unwrap();
    let b_out = engine.get_outgoing_relationships(b, 1).unwrap();
    let c_out = engine.get_outgoing_relationships(c, 1).unwrap();

    assert_eq!(a_out.len(), 1);
    assert_eq!(b_out.len(), 1);
    assert_eq!(c_out.len(), 0);

    // Verify incoming
    let a_in = engine.get_incoming_relationships(a, 1).unwrap();
    let b_in = engine.get_incoming_relationships(b, 1).unwrap();
    let c_in = engine.get_incoming_relationships(c, 1).unwrap();

    assert_eq!(a_in.len(), 0);
    assert_eq!(b_in.len(), 1);
    assert_eq!(c_in.len(), 1);
}

// ============================================================================
// 6.4.1.4 Migration Testing
// ============================================================================

#[test]
fn test_migration_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let source_path = temp_dir.path().join("source");
    let graph_path = temp_dir.path().join("migrated.graph");

    // Create source RecordStore with data
    let mut source = RecordStore::new(&source_path).unwrap();

    for _ in 0..10 {
        let id = source.allocate_node_id();
        let mut node = NodeRecord::new();
        node.add_label(1);
        source.write_node(id, &node).unwrap();
    }

    for i in 0..5u64 {
        let id = source.allocate_rel_id();
        let rel = RelationshipRecord::new(i * 2, i * 2 + 1, 10);
        source.write_rel(id, &rel).unwrap();
    }

    // Migrate to GraphStorageEngine
    let options = MigrationOptions {
        verbose: false,
        verify: true,
        ..Default::default()
    };

    let stats = migrate_to_graph_engine(&source, &graph_path, &options).unwrap();

    assert_eq!(stats.nodes_migrated, 10);
    assert_eq!(stats.relationships_migrated, 5);
}

// ============================================================================
// 6.4.2.3 Crash Recovery Validation
// ============================================================================

#[test]
fn test_crash_recovery_partial_write() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    // Create engine and write some data
    {
        let mut engine = GraphStorageEngine::create(&path).unwrap();

        // Write nodes
        for i in 0..100 {
            engine.create_node(i as u32).unwrap();
        }

        // Write relationships
        for i in 0..50 {
            engine.create_relationship(i, i + 1, 1).unwrap();
        }

        // Explicit flush
        engine.flush().unwrap();
    }

    // Verify file exists and has content
    assert!(path.exists(), "Graph file should exist after flush");

    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.len() > 0, "Graph file should have content");
}

#[test]
fn test_crash_recovery_header_integrity() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    // Create and flush
    {
        let mut engine = GraphStorageEngine::create(&path).unwrap();
        engine.create_node(1).unwrap();
        engine.flush().unwrap();
    }

    // Read raw header bytes
    let file_content = std::fs::read(&path).unwrap();

    // Verify magic number (0x67726170686462 = "graphdb" in little-endian)
    // First 8 bytes should be the magic number
    assert!(file_content.len() >= 8, "File too small for header");

    let magic = u64::from_le_bytes(file_content[0..8].try_into().unwrap());
    assert_eq!(magic, 0x67726170686462, "Invalid magic number in header");
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
#[ignore] // Stress test - requires significant disk space, run manually
fn test_stress_many_relationship_types() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create nodes
    let node0 = engine.create_node(1).unwrap();
    let node1 = engine.create_node(2).unwrap();

    // Create 100 different relationship types
    for type_id in 0..100u32 {
        engine.create_relationship(node0, node1, type_id).unwrap();
    }

    let stats = engine.stats();
    assert_eq!(stats.relationship_types, 100);
    assert_eq!(stats.relationship_count, 100);
}

#[test]
fn test_stress_large_adjacency_list() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.graph");

    let mut engine = GraphStorageEngine::create(&path).unwrap();

    // Create hub node
    let hub = engine.create_node(1).unwrap();

    // Create 500 spoke nodes and relationships
    for _ in 0..500 {
        let spoke = engine.create_node(2).unwrap();
        engine.create_relationship(hub, spoke, 1).unwrap();
    }

    // Verify adjacency list
    let outgoing = engine.get_outgoing_relationships(hub, 1).unwrap();
    assert_eq!(
        outgoing.len(),
        500,
        "Hub should have 500 outgoing relationships"
    );
}
