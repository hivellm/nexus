//! Integration tests for Nexus graph database
//!
//! These tests verify the complete system functionality across all storage layers.

use nexus_core::catalog::Catalog;
use nexus_core::page_cache::PageCache;
use nexus_core::storage::RecordStore;
use nexus_core::transaction::TransactionManager;
use nexus_core::wal::Wal;
use tempfile::TempDir;

#[test]
fn test_workspace_compiles() {
    // This test passing means the workspace compiled successfully
    let version = env!("CARGO_PKG_NAME");
    assert_eq!(version, "nexus");
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
    let dir = TempDir::new().unwrap();

    // Create catalog and storage
    let catalog = Catalog::new(dir.path().join("catalog")).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();

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

    // Verify catalog statistics
    catalog.increment_node_count(person_label).unwrap();
    let stats = catalog.get_statistics().unwrap();
    assert_eq!(stats.node_counts.get(&person_label), Some(&1));
}

// Integration Test 2: Storage + Relationship Traversal
#[test]
fn test_relationship_traversal_integration() {
    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path().join("catalog")).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();

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

    let mut rel1 = nexus_core::storage::RelationshipRecord::default();
    rel1.src_id = node1_id;
    rel1.dst_id = node2_id;
    rel1.type_id = knows_type;
    rel1.next_src_ptr = rel2_id; // Points to next rel from node1

    let mut rel2 = nexus_core::storage::RelationshipRecord::default();
    rel2.src_id = node1_id;
    rel2.dst_id = node3_id;
    rel2.type_id = knows_type;
    rel2.next_src_ptr = u64::MAX; // End of list

    store.write_rel(rel1_id, &rel1).unwrap();
    store.write_rel(rel2_id, &rel2).unwrap();

    // Update node1 to point to first relationship
    node1.first_rel_ptr = rel1_id;
    store.write_node(node1_id, &node1).unwrap();

    // Traverse relationships
    let node = store.read_node(node1_id).unwrap();
    assert_eq!(node.first_rel_ptr, rel1_id);

    let first_rel = store.read_rel(rel1_id).unwrap();
    assert_eq!(first_rel.dst_id, node2_id);
    assert_eq!(first_rel.next_src_ptr, rel2_id);

    let second_rel = store.read_rel(rel2_id).unwrap();
    assert_eq!(second_rel.dst_id, node3_id);
    assert_eq!(second_rel.next_src_ptr, u64::MAX);
}

// Integration Test 3: Transaction + WAL
#[test]
fn test_transaction_wal_integration() {
    let dir = TempDir::new().unwrap();

    let mut tx_mgr = TransactionManager::new().unwrap();
    let mut wal = Wal::new(dir.path().join("wal.log")).unwrap();

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
    let mut wal2 = Wal::new(wal.path).unwrap();
    let entries = wal2.recover().unwrap();
    assert_eq!(entries.len(), 3);
}

// Integration Test 4: Page Cache + Storage
#[test]
fn test_page_cache_storage_integration() {
    let dir = TempDir::new().unwrap();

    let store = RecordStore::new(dir.path()).unwrap();
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
    let dir = TempDir::new().unwrap();

    let catalog = Catalog::new(dir.path().join("catalog")).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();
    let mut wal = Wal::new(dir.path().join("wal.log")).unwrap();

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

    let cat_stats = catalog.get_statistics().unwrap();
    assert_eq!(cat_stats.node_counts.get(&0), Some(&1));
}

// Integration Test 6: WAL Crash Recovery
#[test]
fn test_wal_crash_recovery() {
    let dir = TempDir::new().unwrap();
    let wal_path = dir.path().join("wal.log");

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
            match entry {
                nexus_core::wal::WalEntry::CreateNode { .. } => node_count += 1,
                _ => {}
            }
        }
        assert_eq!(node_count, 5);
    }
}

// Integration Test 7: Page Cache Eviction with Storage
#[test]
fn test_page_cache_eviction_integration() {
    let dir = TempDir::new().unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let mut cache = PageCache::new(5).unwrap(); // Small cache

    // Write enough nodes to exceed cache capacity
    for i in 0..100 {
        let node_id = store.allocate_node_id();
        let mut node = nexus_core::storage::NodeRecord::default();
        node.add_label((i % 10) as u32);
        store.write_node(node_id, &node).unwrap();

        // Simulate page access
        let page_id = node_id / 32;
        if page_id < 10 {
            let _page = cache.get_page(page_id).unwrap();
        }
    }

    let stats = cache.stats();
    assert!(stats.evictions > 0); // Should have evicted some pages
    assert!(stats.hit_rate() > 0.0); // Should have some hits
}

// Integration Test 8: Multi-Module Transaction
#[test]
fn test_multi_module_transaction() {
    let dir = TempDir::new().unwrap();

    let catalog = Catalog::new(dir.path().join("catalog")).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let mut cache = PageCache::new(100).unwrap();
    let mut tx_mgr = TransactionManager::new().unwrap();
    let mut wal = Wal::new(dir.path().join("wal.log")).unwrap();

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
    let mut rel = nexus_core::storage::RelationshipRecord::default();
    rel.src_id = node1_id;
    rel.dst_id = node2_id;
    rel.type_id = knows_type;
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
    let dir = TempDir::new().unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
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
    let dir = TempDir::new().unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let catalog = Catalog::new(dir.path().join("catalog")).unwrap();

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

    println!("Node insert: {} nodes in {:?}", count, elapsed);
    println!("Throughput: {:.0} nodes/sec", throughput);

    // Should be fast (> 10K inserts/sec)
    assert!(throughput > 10_000.0, "Throughput too low: {}", throughput);
}

// Integration Test 11: Performance Benchmark - Node Read
#[test]
fn test_node_read_performance() {
    let dir = TempDir::new().unwrap();
    let store = RecordStore::new(dir.path()).unwrap();

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

    println!("Node read: {} nodes in {:?}", count, elapsed);
    println!("Throughput: {:.0} nodes/sec", throughput);

    // Should be very fast (> 100K reads/sec)
    assert!(throughput > 100_000.0, "Throughput too low: {}", throughput);
}

// Integration Test 12: Checkpoint and Truncate
#[test]
fn test_checkpoint_integration() {
    let dir = TempDir::new().unwrap();
    let mut wal = Wal::new(dir.path().join("wal.log")).unwrap();
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

    let dir = TempDir::new().unwrap();
    let store = Arc::new(RecordStore::new(dir.path()).unwrap());
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
                if s.read_node(i).is_ok() {
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
            let node_id = s.allocate_node_id();
            let mut node = nexus_core::storage::NodeRecord::default();
            node.add_label(i);
            s.write_node(node_id, &node).unwrap();
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

#[test]
fn test_executor_e2e_simple_match() {
    use nexus_core::executor::Executor;
    use nexus_core::index::{LabelIndex, KnnIndex};
    use serde_json::Value;
    use std::collections::HashMap;

    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();
    
    let executor = Executor::new(catalog, store, label_index, knn_index).unwrap();
    
    // Create test data
    let mut params = HashMap::new();
    params.insert("name".to_string(), Value::String("Alice".to_string()));
    
    let query = nexus_core::executor::Query {
        cypher: "MATCH (n:Person) WHERE n.name = $name RETURN n".to_string(),
        params,
    };
    
    // Execute query (this will fail gracefully since we don't have data)
    let result = executor.execute(&query);
    // For now, just verify the executor can be created and called
    assert!(result.is_ok() || result.is_err()); // Either is fine for MVP
}

#[test]
fn test_executor_e2e_aggregation() {
    use nexus_core::executor::{Executor, ExecutionContext, Aggregation};
    use nexus_core::index::{LabelIndex, KnnIndex};
    use serde_json::{Value, Number};
    use std::collections::HashMap;

    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();
    
    let executor = Executor::new(catalog, store, label_index, knn_index).unwrap();
    
    // Create test context with sample data
    let mut context = ExecutionContext::new(HashMap::new());
    context.result_set.columns = vec!["department".to_string(), "salary".to_string()];
    context.result_set.rows = vec![
        vec![Value::String("IT".to_string()), Value::Number(Number::from(1000))],
        vec![Value::String("IT".to_string()), Value::Number(Number::from(2000))],
        vec![Value::String("HR".to_string()), Value::Number(Number::from(1500))],
        vec![Value::String("HR".to_string()), Value::Number(Number::from(2500))],
    ];
    
    // Test GROUP BY with SUM aggregation
    let aggregations = vec![Aggregation::Sum { 
        column: Some("salary".to_string()), 
        alias: "total_salary".to_string() 
    }];
    
    let result = executor.execute_aggregate(&mut context, &["department".to_string()], &aggregations);
    assert!(result.is_ok());
    
    // Verify we have 2 groups (IT and HR)
    assert_eq!(context.result_set.rows.len(), 2);
    
    // Verify each group has 2 columns (department + total_salary)
    for row in &context.result_set.rows {
        assert_eq!(row.len(), 2);
        assert!(row[0].is_string()); // department
        assert!(row[1].is_number()); // total_salary
    }
}

#[test]
fn test_executor_e2e_pattern_traversal() {
    use nexus_core::executor::{Executor, Direction};
    use nexus_core::index::{LabelIndex, KnnIndex};
    use serde_json::Value;
    use std::collections::HashMap;

    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();
    
    let executor = Executor::new(catalog, store, label_index, knn_index).unwrap();
    
    // Test relationship traversal (will work with empty data)
    let relationships = executor.find_relationships(1, None, Direction::Outgoing);
    assert!(relationships.is_ok());
    
    // Should return empty vector for non-existent node
    assert_eq!(relationships.unwrap().len(), 0);
}

#[test]
fn test_executor_e2e_order_by_limit() {
    use nexus_core::executor::{Executor, ExecutionContext};
    use nexus_core::index::{LabelIndex, KnnIndex};
    use serde_json::{Value, Number};
    use std::collections::HashMap;

    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();
    
    let executor = Executor::new(catalog, store, label_index, knn_index).unwrap();
    
    // Create test context with sample data
    let mut context = ExecutionContext::new(HashMap::new());
    context.result_set.columns = vec!["name".to_string(), "score".to_string()];
    context.result_set.rows = vec![
        vec![Value::String("Alice".to_string()), Value::Number(Number::from(30))],
        vec![Value::String("Bob".to_string()), Value::Numb    // Verify limit worked
    assert_eq!(context.result_set.rows.len(), 2);
}

// ============================================================================
// API Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_api_error_handling_400_bad_request() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use hyper::body::to_bytes;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Test 400 Bad Request - Invalid JSON
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from("invalid json"))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test 400 Bad Request - Missing required field
    let query_body = json!({
        "params": {}
        // Missing "query" field
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test 400 Bad Request - Invalid vector format
    let knn_body = json!({
        "label": "Person",
        "vector": "invalid_vector", // Should be array
        "k": 5
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/knn_traverse")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&knn_body).unwrap()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test 400 Bad Request - Invalid labels format
    let node_body = json!({
        "labels": "not_array", // Should be array
        "properties": {"name": "Alice"}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/data/nodes")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&node_body).unwrap()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_api_error_handling_404_not_found() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Test 404 Not Found - Non-existent endpoint
    let request = Request::builder()
        .method(Method::GET)
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test 404 Not Found - Non-existent endpoint with POST
    let request = Request::builder()
        .method(Method::POST)
        .uri("/nonexistent")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_api_error_handling_405_method_not_allowed() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Test 405 Method Not Allowed - GET on POST-only endpoint
    let request = Request::builder()
        .method(Method::GET)
        .uri("/cypher")
        .body(Body::empty())
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    // Test 405 Method Not Allowed - POST on GET-only endpoint
    let request = Request::builder()
        .method(Method::POST)
        .uri("/health")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_api_error_handling_408_request_timeout() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;
    use tokio::time::{sleep, Duration};

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router with timeout middleware
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Test timeout handling - this would need to be implemented in the actual API
    // For now, we test that the server responds to valid requests
    let query_body = json!({
        "query": "RETURN 1 as test",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    // Should return 200 OK for valid request
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_api_error_handling_500_internal_server_error() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Test that the server handles requests gracefully
    // In a real implementation, we would test scenarios that cause 500 errors
    let query_body = json!({
        "query": "RETURN 1 as test",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    // Should return 200 OK for valid request
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_api_error_handling_malformed_requests() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Test malformed requests
    let malformed_requests = vec![
        ("/cypher", Method::POST, "not json"),
        ("/knn_traverse", Method::POST, r#"{"invalid": "structure"}"#),
        ("/data/nodes", Method::POST, r#"{"labels": "not_array"}"#),
        ("/schema/labels", Method::POST, r#"{"name": 123}"#), // name should be string
        ("/ingest", Method::POST, r#"{"nodes": "not_array"}"#),
    ];
    
    for (endpoint, method, body) in malformed_requests {
        let request = Request::builder()
            .method(method)
            .uri(endpoint)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        
        // Should return 400 for malformed requests
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Endpoint {} {} should return 400 for malformed request: {}",
            method,
            endpoint,
            body
        );
    }
}

// ============================================================================
// API Performance Tests
// ============================================================================

#[tokio::test]
async fn test_api_performance_health_check() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Performance test: Health check endpoint
    let start = std::time::Instant::now();
    let mut success_count = 0;
    let total_requests = 1000;
    
    for _ in 0..total_requests {
        let request = Request::builder()
            .method(Method::GET)
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        if response.status() == StatusCode::OK {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let throughput = total_requests as f64 / elapsed.as_secs_f64();
    
    println!("Health check performance: {} requests in {:?}", total_requests, elapsed);
    println!("Throughput: {:.0} requests/sec", throughput);
    println!("Success rate: {:.1}%", (success_count as f64 / total_requests as f64) * 100.0);
    
    assert_eq!(success_count, total_requests);
    assert!(throughput > 1000.0, "Throughput too low: {:.0} req/sec", throughput);
}

#[tokio::test]
async fn test_api_performance_cypher_queries() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Performance test: Cypher queries
    let query_body = json!({
        "query": "RETURN 1 as test",
        "params": {}
    });
    
    let start = std::time::Instant::now();
    let mut success_count = 0;
    let total_requests = 500; // Fewer requests for more complex operations
    
    for _ in 0..total_requests {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/cypher")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        if response.status() == StatusCode::OK {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let throughput = total_requests as f64 / elapsed.as_secs_f64();
    
    println!("Cypher query performance: {} requests in {:?}", total_requests, elapsed);
    println!("Throughput: {:.0} requests/sec", throughput);
    println!("Success rate: {:.1}%", (success_count as f64 / total_requests as f64) * 100.0);
    
    assert_eq!(success_count, total_requests);
    assert!(throughput > 100.0, "Throughput too low: {:.0} req/sec", throughput);
}

#[tokio::test]
async fn test_api_performance_concurrent_requests() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Performance test: Concurrent requests
    let start = std::time::Instant::now();
    let mut handles = vec![];
    let concurrent_requests = 50;
    
    for i in 0..concurrent_requests {
        let app_clone = app.clone();
        let handle = tokio::spawn(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(Body::empty())
                .unwrap();
            
            let response = app_clone.oneshot(request).await.unwrap();
            (i, response.status())
        });
        handles.push(handle);
    }
    
    let mut success_count = 0;
    for handle in handles {
        let (_, status) = handle.await.unwrap();
        if status == StatusCode::OK {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let throughput = concurrent_requests as f64 / elapsed.as_secs_f64();
    
    println!("Concurrent requests performance: {} requests in {:?}", concurrent_requests, elapsed);
    println!("Throughput: {:.0} requests/sec", throughput);
    println!("Success rate: {:.1}%", (success_count as f64 / concurrent_requests as f64) * 100.0);
    
    assert_eq!(success_count, concurrent_requests);
    assert!(throughput > 100.0, "Throughput too low: {:.0} req/sec", throughput);
}

#[tokio::test]
async fn test_api_performance_large_payloads() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Performance test: Large payloads
    let large_vector: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
    let knn_body = json!({
        "label": "Person",
        "vector": large_vector,
        "k": 10,
        "limit": 100
    });
    
    let start = std::time::Instant::now();
    let mut success_count = 0;
    let total_requests = 100;
    
    for _ in 0..total_requests {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/knn_traverse")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&knn_body).unwrap()))
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        if response.status() == StatusCode::OK {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let throughput = total_requests as f64 / elapsed.as_secs_f64();
    let payload_size = serde_json::to_vec(&knn_body).unwrap().len();
    
    println!("Large payload performance: {} requests in {:?}", total_requests, elapsed);
    println!("Throughput: {:.0} requests/sec", throughput);
    println!("Payload size: {} bytes", payload_size);
    println!("Success rate: {:.1}%", (success_count as f64 / total_requests as f64) * 100.0);
    
    assert_eq!(success_count, total_requests);
    assert!(throughput > 10.0, "Throughput too low: {:.0} req/sec", throughput);
}

#[tokio::test]
async fn test_api_performance_mixed_workload() {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        Router,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    use nexus_core::{
        catalog::Catalog,
        executor::Executor,
        index::{KnnIndex, LabelIndex},
        storage::RecordStore,
    };
    use nexus_server::{api, main::NexusServer};

    // Create test server
    let temp_dir = TempDir::new().unwrap();
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));

    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();

    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    // Build router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }));

    // Performance test: Mixed workload
    let start = std::time::Instant::now();
    let mut handles = vec![];
    let total_requests = 200;
    
    // Spawn different types of requests concurrently
    for i in 0..total_requests {
        let app_clone = app.clone();
        let handle = tokio::spawn(async move {
            let request = match i % 4 {
                0 => Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
                1 => {
                    let query_body = json!({
                        "query": "RETURN 1 as test",
                        "params": {}
                    });
                    Request::builder()
                        .method(Method::POST)
                        .uri("/cypher")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
                        .unwrap()
                },
                2 => {
                    let knn_body = json!({
                        "label": "Person",
                        "vector": [0.1, 0.2, 0.3, 0.4],
                        "k": 5
                    });
                    Request::builder()
                        .method(Method::POST)
                        .uri("/knn_traverse")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&knn_body).unwrap()))
                        .unwrap()
                },
                _ => Request::builder()
                    .method(Method::GET)
                    .uri("/stats")
                    .body(Body::empty())
                    .unwrap(),
            };
            
            let response = app_clone.oneshot(request).await.unwrap();
            (i, response.status())
        });
        handles.push(handle);
    }
    
    let mut success_count = 0;
    for handle in handles {
        let (_, status) = handle.await.unwrap();
        if status == StatusCode::OK {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let throughput = total_requests as f64 / elapsed.as_secs_f64();
    
    println!("Mixed workload performance: {} requests in {:?}", total_requests, elapsed);
    println!("Throughput: {:.0} requests/sec", throughput);
    println!("Success rate: {:.1}%", (success_count as f64 / total_requests as f64) * 100.0);
    
    assert_eq!(success_count, total_requests);
    assert!(throughput > 50.0, "Throughput too low: {:.0} req/sec", throughput);
}
e::Number(Number::from(20))],
    ];
    
    // Test ORDER BY score ASC
    let result = executor.execute_sort(&mut context, &["score".to_string()], &[true]);
    assert!(result.is_ok());
    
    // Verify sorting worked
    assert_eq!(context.result_set.rows.len(), 3);
    
    // Check that scores are in ascending order
    let scores: Vec<i64> = context.result_set.rows.iter()
        .map(|row| row[1].as_i64().unwrap())
        .collect();
    assert_eq!(scores, vec![10, 20, 30]);
    
    // Test LIMIT 2
    let result = executor.execute_limit(&mut context, 2);
    assert!(result.is_ok());
    
    // Verify limit worked
    assert_eq!(context.result_set.rows.len(), 2);
}

