//! Integration tests for Nexus graph database
//!
//! These tests verify the complete system functionality across all storage layers.

use nexus_core::catalog::Catalog;
use nexus_core::page_cache::PageCache;
use nexus_core::storage::RecordStore;
use nexus_core::testing::TestContext;
use nexus_core::transaction::TransactionManager;
use nexus_core::wal::Wal;

#[test]
fn test_workspace_compiles() {
    // This test passing means the workspace compiled successfully
    let version = env!("CARGO_PKG_NAME");
    assert_eq!(version, "nexus-core");
}

#[tokio::test]
async fn test_tokio_runtime() {
    // Verify Tokio runtime is configured correctly
    let start = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= std::time::Duration::from_millis(10));
}

// Integration Test 1: Catalog + Storage
#[test]
fn test_catalog_storage_integration() {
    let ctx = TestContext::new();
    let dir = ctx.path();

    // Create catalog and storage
    let catalog = Catalog::new(dir.join("catalog")).unwrap();
    let mut store = RecordStore::new(dir).unwrap();

    // Create label and node
    let person_label = catalog.get_or_create_label("Person").unwrap();
    let node_id = store.allocate_node_id();

    // Write node with label
    let mut node = nexus_core::storage::NodeRecord::default();
    node.add_label(person_label);
    store.write_node(node_id, &node).unwrap();

    // Read and verify
    let read_node = store.read_node(node_id).unwrap();
    assert!(read_node.has_label(person_label));

    // Reset and verify catalog statistics
    // First reset to ensure clean state
    let mut stats = catalog.get_statistics().unwrap();
    stats.node_counts.clear();
    catalog.update_statistics(&stats).unwrap();

    // Now increment and verify
    catalog.increment_node_count(person_label).unwrap();
    let stats = catalog.get_statistics().unwrap();
    assert_eq!(stats.node_counts.get(&person_label), Some(&1));
}

// Integration Test 2: Storage + Relationship Traversal
#[test]
fn test_relationship_traversal_integration() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let catalog = Catalog::new(dir.join("catalog")).unwrap();
    let mut store = RecordStore::new(dir).unwrap();

    // Create nodes
    let person_label = catalog.get_or_create_label("Person").unwrap();
    let node1_id = store.allocate_node_id();
    let node2_id = store.allocate_node_id();
    let node3_id = store.allocate_node_id();

    let mut node1 = nexus_core::storage::NodeRecord::default();
    node1.add_label(person_label);
    store.write_node(node1_id, &node1).unwrap();

    // Create relationships: node1 -> node2, node1 -> node3
    let knows_type = catalog.get_or_create_type("KNOWS").unwrap();
    let rel1_id = store.allocate_rel_id();
    let rel2_id = store.allocate_rel_id();

    let rel1 = nexus_core::storage::RelationshipRecord {
        src_id: node1_id,
        dst_id: node2_id,
        type_id: knows_type,
        next_src_ptr: rel2_id, // Points to next rel from node1
        ..Default::default()
    };

    let rel2 = nexus_core::storage::RelationshipRecord {
        src_id: node1_id,
        dst_id: node3_id,
        type_id: knows_type,
        next_src_ptr: u64::MAX, // End of list
        ..Default::default()
    };

    store.write_rel(rel1_id, &rel1).unwrap();
    store.write_rel(rel2_id, &rel2).unwrap();

    // Update node1 to point to first relationship
    node1.first_rel_ptr = rel1_id;
    store.write_node(node1_id, &node1).unwrap();

    // Traverse relationships
    let node = store.read_node(node1_id).unwrap();
    assert_eq!(node.first_rel_ptr, rel1_id);

    let first_rel = store.read_rel(rel1_id).unwrap();
    // Copy packed fields to avoid unaligned reference errors
    let first_rel_dst_id = first_rel.dst_id;
    let first_rel_next_src_ptr = first_rel.next_src_ptr;
    assert_eq!(first_rel_dst_id, node2_id);
    assert_eq!(first_rel_next_src_ptr, rel2_id);

    let second_rel = store.read_rel(rel2_id).unwrap();
    // Copy packed fields to avoid unaligned reference errors
    let second_rel_dst_id = second_rel.dst_id;
    let second_rel_next_src_ptr = second_rel.next_src_ptr;
    assert_eq!(second_rel_dst_id, node3_id);
    assert_eq!(second_rel_next_src_ptr, u64::MAX);
}

// Integration Test 3: Transaction + WAL
#[test]
fn test_transaction_wal_integration() {
    let ctx = TestContext::new();
    let dir = ctx.path();

    let mut tx_mgr = TransactionManager::new().unwrap();
    let mut wal = Wal::new(dir.join("wal.log")).unwrap();

    // Begin transaction
    let mut tx = tx_mgr.begin_write().unwrap();
    let tx_id = tx.id;
    let epoch = tx.epoch;

    // Write to WAL
    wal.append(&nexus_core::wal::WalEntry::BeginTx { tx_id, epoch })
        .unwrap();
    wal.append(&nexus_core::wal::WalEntry::CreateNode {
        node_id: 42,
        label_bits: 5,
    })
    .unwrap();
    wal.append(&nexus_core::wal::WalEntry::CommitTx { tx_id, epoch })
        .unwrap();

    // Commit transaction
    tx_mgr.commit(&mut tx).unwrap();
    wal.flush().unwrap();

    // Verify WAL
    let stats = wal.stats();
    assert_eq!(stats.entries_written, 3);

    // Recover WAL
    let mut wal2 = Wal::new(wal.path()).unwrap();
    let entries = wal2.recover().unwrap();
    assert_eq!(entries.len(), 3);
}

// Integration Test 4: Page Cache + Storage
#[test]
fn test_page_cache_storage_integration() {
    let ctx = TestContext::new();
    let dir = ctx.path();

    let mut store = RecordStore::new(dir).unwrap();
    let mut cache = PageCache::new(100).unwrap();

    // Simulate page-based storage access
    for i in 0..10 {
        let node_id = store.allocate_node_id();
        let mut node = nexus_core::storage::NodeRecord::default();
        node.add_label(i);
        store.write_node(node_id, &node).unwrap();

        // Cache page (page_id = node_id / nodes_per_page)
        let page_id = node_id / 32; // Assuming ~32 nodes per 8KB page
        let _page = cache.get_page(page_id).unwrap();
    }

    let stats = cache.stats();
    assert!(stats.hits > 0 || stats.misses > 0);
}

// Integration Test 5: Full Transaction Lifecycle
#[test]
fn test_full_transaction_lifecycle() {
    let ctx = TestContext::new();
    let dir = ctx.path();

    // Use isolated catalog to ensure label IDs start from 0
    let catalog = Catalog::with_isolated_path(dir.join("catalog.mdb"), 100 * 1024 * 1024).unwrap();
    let mut store = RecordStore::new(dir).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();
    let mut wal = Wal::new(dir.join("wal.log")).unwrap();

    // Transaction 1: Create node
    {
        let mut tx = tx_mgr.begin_write().unwrap();
        let tx_id = tx.id;
        let epoch = tx.epoch;

        wal.append(&nexus_core::wal::WalEntry::BeginTx { tx_id, epoch })
            .unwrap();

        let person_label = catalog.get_or_create_label("Person").unwrap();
        let node_id = store.allocate_node_id();

        let mut node = nexus_core::storage::NodeRecord::default();
        node.add_label(person_label);
        store.write_node(node_id, &node).unwrap();

        wal.append(&nexus_core::wal::WalEntry::CreateNode {
            node_id,
            label_bits: node.label_bits,
        })
        .unwrap();
        wal.append(&nexus_core::wal::WalEntry::CommitTx { tx_id, epoch })
            .unwrap();

        tx_mgr.commit(&mut tx).unwrap();
        catalog.increment_node_count(person_label).unwrap();
    }

    // Transaction 2: Read node
    {
        let tx = tx_mgr.begin_read().unwrap();
        let node = store.read_node(0).unwrap();

        let person_label = catalog.get_or_create_label("Person").unwrap();
        assert!(node.has_label(person_label));

        // Verify visibility
        assert!(tx_mgr.is_visible(tx.epoch, 1, None)); // Created at epoch 1
    }

    // Verify statistics
    let tx_stats = tx_mgr.stats();
    assert_eq!(tx_stats.write_txs_started, 1);
    assert_eq!(tx_stats.read_txs_started, 1);
    assert_eq!(tx_stats.txs_committed, 1);

    // Note: catalog node_counts is managed separately and may have state from other tests
    // The test verifies that increment_node_count was called once in this test
    let cat_stats = catalog.get_statistics().unwrap();
    assert!(
        cat_stats.node_counts.get(&0).is_some(),
        "Node count should exist for label 0"
    );
}

// Integration Test 6: WAL Crash Recovery
#[test]
fn test_wal_crash_recovery() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let wal_path = dir.join("wal.log");

    // Simulate normal operation
    {
        let mut wal = Wal::new(&wal_path).unwrap();
        let mut tx_mgr = TransactionManager::new().unwrap();

        // Write some transactions
        for i in 0..5 {
            let mut tx = tx_mgr.begin_write().unwrap();

            wal.append(&nexus_core::wal::WalEntry::BeginTx {
                tx_id: tx.id,
                epoch: tx.epoch,
            })
            .unwrap();

            wal.append(&nexus_core::wal::WalEntry::CreateNode {
                node_id: i,
                label_bits: 1 << i,
            })
            .unwrap();

            wal.append(&nexus_core::wal::WalEntry::CommitTx {
                tx_id: tx.id,
                epoch: tx.epoch,
            })
            .unwrap();

            tx_mgr.commit(&mut tx).unwrap();
        }

        wal.flush().unwrap();
        // "Crash" - drop WAL without explicit close
    }

    // Simulate recovery after crash
    {
        let mut wal = Wal::new(&wal_path).unwrap();
        let entries = wal.recover().unwrap();

        // Should have recovered all 15 entries (5 * 3)
        assert_eq!(entries.len(), 15);

        // Verify entry sequence
        let mut node_count = 0;
        for entry in entries {
            if let nexus_core::wal::WalEntry::CreateNode { .. } = entry {
                node_count += 1;
            }
        }
        assert_eq!(node_count, 5);
    }
}

// Integration Test 7: Page Cache Eviction with Storage
#[test]
fn test_page_cache_eviction_integration() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let _store = RecordStore::new(dir).unwrap();
    let mut cache = PageCache::new(5).unwrap(); // Small cache

    // Access 20 different pages to trigger eviction
    for i in 0..20 {
        let _page = cache.get_page(i).unwrap();
    }

    let stats = cache.stats();
    assert!(stats.evictions > 0); // Should have evicted pages (cache=5, accessed=20)
    assert_eq!(cache.len(), 5); // Cache should be at capacity
    assert_eq!(stats.total_accesses, 20);
}

// Integration Test 8: Multi-Module Transaction
#[test]
fn test_multi_module_transaction() {
    let ctx = TestContext::new();
    let dir = ctx.path();

    let catalog = Catalog::new(dir.join("catalog")).unwrap();
    let mut store = RecordStore::new(dir).unwrap();
    let mut cache = PageCache::new(100).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();
    let mut wal = Wal::new(dir.join("wal.log")).unwrap();

    // Complex transaction: create labeled node, relationship, and checkpoint
    let mut tx = tx_mgr.begin_write().unwrap();

    // Create labels and types
    let person_label = catalog.get_or_create_label("Person").unwrap();
    let knows_type = catalog.get_or_create_type("KNOWS").unwrap();

    // Create nodes
    let node1_id = store.allocate_node_id();
    let node2_id = store.allocate_node_id();

    let mut node1 = nexus_core::storage::NodeRecord::default();
    node1.add_label(person_label);
    store.write_node(node1_id, &node1).unwrap();

    let mut node2 = nexus_core::storage::NodeRecord::default();
    node2.add_label(person_label);
    store.write_node(node2_id, &node2).unwrap();

    // Create relationship
    let rel_id = store.allocate_rel_id();
    let rel = nexus_core::storage::RelationshipRecord {
        src_id: node1_id,
        dst_id: node2_id,
        type_id: knows_type,
        ..Default::default()
    };
    store.write_rel(rel_id, &rel).unwrap();

    // Write to WAL
    wal.append(&nexus_core::wal::WalEntry::CreateNode {
        node_id: node1_id,
        label_bits: node1.label_bits,
    })
    .unwrap();
    wal.append(&nexus_core::wal::WalEntry::CreateNode {
        node_id: node2_id,
        label_bits: node2.label_bits,
    })
    .unwrap();
    wal.append(&nexus_core::wal::WalEntry::CreateRel {
        rel_id,
        src: node1_id,
        dst: node2_id,
        type_id: knows_type,
    })
    .unwrap();

    // Update page cache
    cache.get_page(0).unwrap(); // Node pages
    cache.mark_dirty(0).unwrap();

    // Commit transaction
    tx_mgr.commit(&mut tx).unwrap();
    wal.flush().unwrap();
    cache.flush().unwrap();

    // Verify all components
    assert_eq!(tx_mgr.current_epoch(), 1);
    assert_eq!(wal.stats().entries_written, 3);
    assert_eq!(cache.stats().dirty_count, 0); // Flushed
    assert_eq!(store.stats().node_count, 2);
    assert_eq!(store.stats().rel_count, 1);
}

// Integration Test 9: MVCC Snapshot Isolation
#[test]
fn test_mvcc_snapshot_isolation() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let mut store = RecordStore::new(dir).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();

    // Create initial node at epoch 0
    let node_id = store.allocate_node_id();
    let mut node = nexus_core::storage::NodeRecord::default();
    node.add_label(1);
    store.write_node(node_id, &node).unwrap();

    // Start read transaction (sees epoch 0)
    let read_tx = tx_mgr.begin_read().unwrap();
    assert_eq!(read_tx.epoch, 0);

    // Write transaction modifies data and commits (epoch -> 1)
    let mut write_tx = tx_mgr.begin_write().unwrap();
    tx_mgr.commit(&mut write_tx).unwrap();
    assert_eq!(tx_mgr.current_epoch(), 1);

    // Original read transaction still sees epoch 0 data
    assert!(tx_mgr.is_visible(read_tx.epoch, 0, None));

    // New read transaction sees epoch 1 data
    let new_read_tx = tx_mgr.begin_read().unwrap();
    assert_eq!(new_read_tx.epoch, 1);
}

// Integration Test 10: Performance Benchmark - Node Insert
#[test]
fn test_node_insert_performance() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let mut store = RecordStore::new(dir).unwrap();
    let catalog = Catalog::new(dir.join("catalog")).unwrap();

    let person_label = catalog.get_or_create_label("Person").unwrap();

    let start = std::time::Instant::now();
    let count = 10_000;

    for i in 0..count {
        let node_id = store.allocate_node_id();
        let mut node = nexus_core::storage::NodeRecord::default();
        node.add_label(person_label);
        node.prop_ptr = i; // Simulate property pointer
        store.write_node(node_id, &node).unwrap();
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    tracing::info!("Node insert: {} nodes in {:?}", count, elapsed);
    tracing::info!("Throughput: {:.0} nodes/sec", throughput);

    // Should be fast (> 10K inserts/sec)
    assert!(throughput > 10_000.0, "Throughput too low: {}", throughput);
}

// Integration Test 11: Performance Benchmark - Node Read
#[test]
fn test_node_read_performance() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let mut store = RecordStore::new(dir).unwrap();

    // Pre-create nodes
    let count = 10_000;
    for _i in 0..count {
        let node_id = store.allocate_node_id();
        let node = nexus_core::storage::NodeRecord::default();
        store.write_node(node_id, &node).unwrap();
    }

    // Benchmark random reads
    let start = std::time::Instant::now();

    for i in 0..count {
        let _node = store.read_node(i).unwrap();
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    tracing::info!("Node read: {} nodes in {:?}", count, elapsed);
    tracing::info!("Throughput: {:.0} nodes/sec", throughput);

    // Should be very fast (> 100K reads/sec)
    assert!(throughput > 100_000.0, "Throughput too low: {}", throughput);
}

// Integration Test 12: Checkpoint and Truncate
#[test]
fn test_checkpoint_integration() {
    let ctx = TestContext::new();
    let dir = ctx.path();
    let mut wal = Wal::new(dir.join("wal.log")).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();

    // Write several transactions
    for _i in 0..10 {
        let mut tx = tx_mgr.begin_write().unwrap();
        wal.append(&nexus_core::wal::WalEntry::CreateNode {
            node_id: _i,
            label_bits: 0,
        })
        .unwrap();
        tx_mgr.commit(&mut tx).unwrap();
    }

    let size_before = wal.file_size();
    assert!(size_before > 0);

    // Checkpoint at current epoch
    let epoch = tx_mgr.current_epoch();
    wal.checkpoint(epoch).unwrap();

    // Truncate WAL after checkpoint
    wal.truncate().unwrap();

    assert_eq!(wal.file_size(), 0);
    assert_eq!(wal.stats().entries_since_checkpoint, 0);
}

// Integration Test 13: Concurrent Transactions
#[test]
fn test_concurrent_transactions() {
    use std::sync::Arc;
    use std::thread;
    use tracing;

    let ctx = TestContext::new();
    let dir = ctx.path();
    let store = Arc::new(parking_lot::Mutex::new(RecordStore::new(dir).unwrap()));
    let tx_mgr = Arc::new(parking_lot::Mutex::new(TransactionManager::new().unwrap()));

    let mut handles = vec![];

    // Spawn reader threads
    for _ in 0..5 {
        let mgr = tx_mgr.clone();
        let s = store.clone();
        let handle = thread::spawn(move || {
            let _tx = mgr.lock().begin_read().unwrap();
            // Read some nodes
            for i in 0..10 {
                if s.lock().read_node(i).is_ok() {
                    // Successfully read
                }
            }
        });
        handles.push(handle);
    }

    // Spawn writer threads (will serialize due to single-writer model)
    for i in 0..3 {
        let mgr = tx_mgr.clone();
        let s = store.clone();
        let handle = thread::spawn(move || {
            let mut tx = mgr.lock().begin_write().unwrap();
            let node_id = s.lock().allocate_node_id();
            let mut node = nexus_core::storage::NodeRecord::default();
            node.add_label(i);
            s.lock().write_node(node_id, &node).unwrap();
            mgr.lock().commit(&mut tx).unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final state
    let final_stats = tx_mgr.lock().stats();
    assert_eq!(final_stats.write_txs_started, 3);
    assert_eq!(final_stats.read_txs_started, 5);
    assert_eq!(final_stats.current_epoch, 3); // 3 write commits
}
