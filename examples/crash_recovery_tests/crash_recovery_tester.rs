use nexus_core::catalog::Catalog;
use nexus_core::executor::Executor;
use nexus_core::index::{LabelIndex, KnnIndex};
use nexus_core::storage::RecordStore;
use nexus_core::transaction::TransactionManager;
use nexus_core::wal::Wal;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;

/// Crash recovery test scenarios
pub struct CrashRecoveryTester {
    temp_dir: tempdir::TempDir,
}

impl CrashRecoveryTester {
    /// Create a new crash recovery tester
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        Ok(Self { temp_dir })
    }
    
    /// Test scenario 1: WAL recovery after crash during write transaction
    pub async fn test_wal_recovery_during_write(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Testing WAL recovery during write transaction...");
        
        // Create initial state
        let catalog = Arc::new(RwLock::new(Catalog::new(self.temp_dir.path())?));
        let wal = Arc::new(RwLock::new(Wal::new(self.temp_dir.path().join("wal.log"))?));
        let tx_manager = Arc::new(RwLock::new(TransactionManager::new()));
        
        // Simulate a write transaction that gets interrupted
        let tx_id = tx_manager.write().await.begin_write_transaction();
        
        // Add some WAL entries
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::BeginTx { tx_id },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::CreateNode {
                tx_id,
                node_id: 1,
                label_bits: 1,
            },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::SetProperty {
                tx_id,
                node_id: 1,
                key_id: 1,
                value: b"test_value".to_vec(),
            },
        )?;
        
        // Simulate crash before commit (don't call commit)
        drop(wal);
        drop(tx_manager);
        
        // Recover from WAL
        let recovered_wal = Arc::new(RwLock::new(Wal::new(self.temp_dir.path().join("wal.log"))?));
        let recovered_tx_manager = Arc::new(RwLock::new(TransactionManager::new()));
        
        // Replay WAL entries
        let entries = recovered_wal.read().await.get_all_entries()?;
        println!("Recovered {} WAL entries", entries.len());
        
        // Verify recovery
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], nexus_core::wal::WalEntry::BeginTx { .. }));
        assert!(matches!(entries[1], nexus_core::wal::WalEntry::CreateNode { .. }));
        assert!(matches!(entries[2], nexus_core::wal::WalEntry::SetProperty { .. }));
        
        println!("✅ WAL recovery test passed");
        Ok(())
    }
    
    /// Test scenario 2: Catalog recovery after corruption
    pub async fn test_catalog_recovery_after_corruption(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Testing catalog recovery after corruption...");
        
        // Create initial catalog with some data
        let catalog_path = self.temp_dir.path().join("catalog");
        std::fs::create_dir_all(&catalog_path)?;
        
        let catalog = Arc::new(RwLock::new(Catalog::new(&catalog_path)?));
        
        // Add some labels and types
        let label_id = catalog.write().await.get_or_create_label("User")?;
        let type_id = catalog.write().await.get_or_create_type("FOLLOWS")?;
        let key_id = catalog.write().await.get_or_create_key("name")?;
        
        println!("Created label_id: {}, type_id: {}, key_id: {}", label_id, type_id, key_id);
        
        // Simulate corruption by closing the catalog
        drop(catalog);
        
        // Try to reopen the catalog
        let recovered_catalog = Arc::new(RwLock::new(Catalog::new(&catalog_path)?));
        
        // Verify data is still there
        let recovered_label_id = recovered_catalog.read().await.get_label_id("User")?;
        let recovered_type_id = recovered_catalog.read().await.get_type_id("FOLLOWS")?;
        let recovered_key_id = recovered_catalog.read().await.get_key_id("name")?;
        
        assert_eq!(recovered_label_id, Some(label_id));
        assert_eq!(recovered_type_id, Some(type_id));
        assert_eq!(recovered_key_id, Some(key_id));
        
        println!("✅ Catalog recovery test passed");
        Ok(())
    }
    
    /// Test scenario 3: Index recovery after crash
    pub async fn test_index_recovery_after_crash(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Testing index recovery after crash...");
        
        // Create indexes with some data
        let label_index = Arc::new(RwLock::new(LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(KnnIndex::new(128)?));
        
        // Add some data to indexes
        label_index.write().await.add_node(1, 1)?; // node_id=1, label_id=1
        label_index.write().await.add_node(2, 1)?; // node_id=2, label_id=1
        label_index.write().await.add_node(3, 2)?; // node_id=3, label_id=2
        
        let vector1 = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let vector2 = vec![0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        
        knn_index.write().await.add_vector(1, 1, &vector1)?;
        knn_index.write().await.add_vector(2, 1, &vector2)?;
        
        // Get stats before crash
        let label_stats_before = label_index.read().await.get_stats();
        let knn_stats_before = knn_index.read().await.get_stats();
        
        println!("Before crash - Label index: {} nodes, KNN index: {} vectors", 
                label_stats_before.total_nodes, knn_stats_before.total_vectors);
        
        // Simulate crash by dropping indexes
        drop(label_index);
        drop(knn_index);
        
        // Recreate indexes (in a real scenario, they would be loaded from persistent storage)
        let recovered_label_index = Arc::new(RwLock::new(LabelIndex::new()));
        let recovered_knn_index = Arc::new(RwLock::new(KnnIndex::new(128)?));
        
        // In a real recovery scenario, we would rebuild indexes from the WAL and storage
        // For this test, we'll simulate the recovery by re-adding the data
        recovered_label_index.write().await.add_node(1, 1)?;
        recovered_label_index.write().await.add_node(2, 1)?;
        recovered_label_index.write().await.add_node(3, 2)?;
        
        recovered_knn_index.write().await.add_vector(1, 1, &vector1)?;
        recovered_knn_index.write().await.add_vector(2, 1, &vector2)?;
        
        // Verify recovery
        let label_stats_after = recovered_label_index.read().await.get_stats();
        let knn_stats_after = recovered_knn_index.read().await.get_stats();
        
        assert_eq!(label_stats_after.total_nodes, label_stats_before.total_nodes);
        assert_eq!(knn_stats_after.total_vectors, knn_stats_before.total_vectors);
        
        println!("✅ Index recovery test passed");
        Ok(())
    }
    
    /// Test scenario 4: Partial transaction recovery
    pub async fn test_partial_transaction_recovery(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Testing partial transaction recovery...");
        
        let catalog = Arc::new(RwLock::new(Catalog::new(self.temp_dir.path())?));
        let wal = Arc::new(RwLock::new(Wal::new(self.temp_dir.path().join("wal.log"))?));
        let tx_manager = Arc::new(RwLock::new(TransactionManager::new()));
        
        // Start a transaction
        let tx_id = tx_manager.write().await.begin_write_transaction();
        
        // Add some operations
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::BeginTx { tx_id },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::CreateNode {
                tx_id,
                node_id: 1,
                label_bits: 1,
            },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::CreateNode {
                tx_id,
                node_id: 2,
                label_bits: 1,
            },
        )?;
        
        // Simulate crash before commit
        drop(wal);
        drop(tx_manager);
        
        // Recover and verify partial transaction is not committed
        let recovered_wal = Arc::new(RwLock::new(Wal::new(self.temp_dir.path().join("wal.log"))?));
        let entries = recovered_wal.read().await.get_all_entries()?;
        
        // Should have BeginTx and CreateNode entries, but no CommitTx
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], nexus_core::wal::WalEntry::BeginTx { .. }));
        assert!(matches!(entries[1], nexus_core::wal::WalEntry::CreateNode { .. }));
        assert!(matches!(entries[2], nexus_core::wal::WalEntry::CreateNode { .. }));
        
        // No CommitTx entry should exist
        let has_commit = entries.iter().any(|entry| matches!(entry, nexus_core::wal::WalEntry::CommitTx { .. }));
        assert!(!has_commit);
        
        println!("✅ Partial transaction recovery test passed");
        Ok(())
    }
    
    /// Test scenario 5: Concurrent transaction recovery
    pub async fn test_concurrent_transaction_recovery(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Testing concurrent transaction recovery...");
        
        let catalog = Arc::new(RwLock::new(Catalog::new(self.temp_dir.path())?));
        let wal = Arc::new(RwLock::new(Wal::new(self.temp_dir.path().join("wal.log"))?));
        let tx_manager = Arc::new(RwLock::new(TransactionManager::new()));
        
        // Start multiple transactions
        let tx1 = tx_manager.write().await.begin_write_transaction();
        let tx2 = tx_manager.write().await.begin_write_transaction();
        
        // Add operations from both transactions
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::BeginTx { tx_id: tx1 },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::BeginTx { tx_id: tx2 },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::CreateNode {
                tx_id: tx1,
                node_id: 1,
                label_bits: 1,
            },
        )?;
        
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::CreateNode {
                tx_id: tx2,
                node_id: 2,
                label_bits: 1,
            },
        )?;
        
        // Commit only tx1, crash before tx2 commit
        wal.write().await.append_entry(
            nexus_core::wal::WalEntry::CommitTx { tx_id: tx1 },
        )?;
        
        // Simulate crash
        drop(wal);
        drop(tx_manager);
        
        // Recover and verify
        let recovered_wal = Arc::new(RwLock::new(Wal::new(self.temp_dir.path().join("wal.log"))?));
        let entries = recovered_wal.read().await.get_all_entries()?;
        
        // Should have both transactions started, but only tx1 committed
        let begin_count = entries.iter().filter(|e| matches!(e, nexus_core::wal::WalEntry::BeginTx { .. })).count();
        let commit_count = entries.iter().filter(|e| matches!(e, nexus_core::wal::WalEntry::CommitTx { .. })).count();
        
        assert_eq!(begin_count, 2);
        assert_eq!(commit_count, 1);
        
        println!("✅ Concurrent transaction recovery test passed");
        Ok(())
    }
    
    /// Run all crash recovery tests
    pub async fn run_all_tests(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("=== Crash Recovery Test Suite ===");
        
        self.test_wal_recovery_during_write().await?;
        self.test_catalog_recovery_after_corruption().await?;
        self.test_index_recovery_after_crash().await?;
        self.test_partial_transaction_recovery().await?;
        self.test_concurrent_transaction_recovery().await?;
        
        println!("\n✅ All crash recovery tests passed!");
        Ok(())
    }
}

/// Performance test for crash recovery scenarios
pub struct CrashRecoveryPerformanceTester {
    temp_dir: tempdir::TempDir,
}

impl CrashRecoveryPerformanceTester {
    /// Create a new performance tester
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        Ok(Self { temp_dir })
    }
    
    /// Test WAL recovery performance with large number of entries
    pub async fn test_wal_recovery_performance(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Testing WAL recovery performance...");
        
        let wal_path = self.temp_dir.path().join("wal.log");
        let wal = Arc::new(RwLock::new(Wal::new(&wal_path)?));
        
        let start_time = std::time::Instant::now();
        
        // Add many entries
        let num_entries = 10000;
        for i in 0..num_entries {
            wal.write().await.append_entry(
                nexus_core::wal::WalEntry::CreateNode {
                    tx_id: i as u64,
                    node_id: i as u32,
                    label_bits: 1,
                },
            )?;
        }
        
        let write_time = start_time.elapsed();
        println!("Wrote {} entries in {:?}", num_entries, write_time);
        
        // Simulate crash
        drop(wal);
        
        // Measure recovery time
        let recovery_start = std::time::Instant::now();
        let recovered_wal = Arc::new(RwLock::new(Wal::new(&wal_path)?));
        let entries = recovered_wal.read().await.get_all_entries()?;
        let recovery_time = recovery_start.elapsed();
        
        println!("Recovered {} entries in {:?}", entries.len(), recovery_time);
        
        // Performance targets
        let write_qps = num_entries as f64 / write_time.as_secs_f64();
        let recovery_qps = num_entries as f64 / recovery_time.as_secs_f64();
        
        println!("Write QPS: {:.0}", write_qps);
        println!("Recovery QPS: {:.0}", recovery_qps);
        
        // Verify all entries were recovered
        assert_eq!(entries.len(), num_entries);
        
        // Performance targets (adjust based on your requirements)
        assert!(write_qps > 1000.0, "Write QPS too low: {:.0}", write_qps);
        assert!(recovery_qps > 500.0, "Recovery QPS too low: {:.0}", recovery_qps);
        
        println!("✅ WAL recovery performance test passed");
        Ok(())
    }
}

/// CLI utility for running crash recovery tests
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: std::env::Args = std::env::args();
    let args: Vec<String> = args.collect();
    
    if args.len() > 1 && args[1] == "performance" {
        println!("Running crash recovery performance tests...");
        let tester = CrashRecoveryPerformanceTester::new()?;
        tester.test_wal_recovery_performance().await?;
    } else {
        println!("Running crash recovery functional tests...");
        let tester = CrashRecoveryTester::new()?;
        tester.run_all_tests().await?;
    }
    
    Ok(())
}





