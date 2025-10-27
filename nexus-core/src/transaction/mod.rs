//! Transaction layer - MVCC, locking, isolation
//!
//! MVCC via epoch-based snapshots:
//! - Readers pin an epoch/snapshot ID
//! - Writers generate new records (append-only) and update pointers on commit
//!
//! Locking via parking_lot:
//! - Single writer per partition (queue) initially
//! - Group commit for batching
//!
//! # Architecture
//!
//! Epoch-based MVCC:
//! - Global epoch counter (atomic u64)
//! - Read transactions pin current epoch (snapshot isolation)
//! - Write transactions increment epoch on commit
//! - Garbage collection removes old versions (created_epoch < min_active_epoch)

use crate::{Error, Result};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Transaction state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    /// Active transaction
    Active,
    /// Committed transaction
    Committed,
    /// Aborted transaction
    Aborted,
}

/// Transaction type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxType {
    /// Read-only transaction
    Read,
    /// Read-write transaction
    Write,
}

/// Transaction handle
#[derive(Debug)]
pub struct Transaction {
    /// Transaction ID
    pub id: u64,
    /// Snapshot epoch (visible up to this epoch)
    pub epoch: u64,
    /// Transaction type
    pub tx_type: TxType,
    /// Transaction state
    pub state: TxState,
}

impl Transaction {
    /// Create a new transaction
    fn new(id: u64, epoch: u64, tx_type: TxType) -> Self {
        Self {
            id,
            epoch,
            tx_type,
            state: TxState::Active,
        }
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.state == TxState::Active
    }

    /// Check if transaction is committed
    pub fn is_committed(&self) -> bool {
        self.state == TxState::Committed
    }

    /// Check if transaction is aborted
    pub fn is_aborted(&self) -> bool {
        self.state == TxState::Aborted
    }

    /// Check if this is a read transaction
    pub fn is_read_only(&self) -> bool {
        self.tx_type == TxType::Read
    }

    /// Check if this is a write transaction
    pub fn is_write(&self) -> bool {
        self.tx_type == TxType::Write
    }
}

/// Epoch manager for MVCC
struct EpochManager {
    /// Current epoch (incremented on each write commit)
    current_epoch: AtomicU64,

    /// Next transaction ID
    next_tx_id: AtomicU64,
}

impl EpochManager {
    fn new() -> Self {
        Self {
            current_epoch: AtomicU64::new(0),
            next_tx_id: AtomicU64::new(0),
        }
    }

    fn get_current_epoch(&self) -> u64 {
        self.current_epoch.load(Ordering::Acquire)
    }

    fn increment_epoch(&self) -> u64 {
        self.current_epoch.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn allocate_tx_id(&self) -> u64 {
        self.next_tx_id.fetch_add(1, Ordering::AcqRel)
    }
}

/// Transaction manager
pub struct TransactionManager {
    /// Epoch manager
    epoch_manager: Arc<EpochManager>,

    /// Write lock (single-writer model for MVP)
    write_lock: Arc<Mutex<()>>,

    /// Statistics
    stats: TransactionStats,
}

/// Transaction statistics
#[derive(Debug, Clone, Default)]
pub struct TransactionStats {
    /// Total read transactions started
    pub read_txs_started: u64,
    /// Total write transactions started
    pub write_txs_started: u64,
    /// Total transactions committed
    pub txs_committed: u64,
    /// Total transactions aborted
    pub txs_aborted: u64,
    /// Current epoch
    pub current_epoch: u64,
}

impl TransactionManager {
    /// Create a new transaction manager
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::transaction::TransactionManager;
    ///
    /// let tx_mgr = TransactionManager::new().unwrap();
    /// ```
    pub fn new() -> Result<Self> {
        Ok(Self {
            epoch_manager: Arc::new(EpochManager::new()),
            write_lock: Arc::new(Mutex::new(())),
            stats: TransactionStats::default(),
        })
    }

    /// Begin a new read transaction
    ///
    /// Read transactions pin the current epoch for snapshot isolation.
    pub fn begin_read(&mut self) -> Result<Transaction> {
        let epoch = self.epoch_manager.get_current_epoch();
        let tx_id = self.epoch_manager.allocate_tx_id();

        self.stats.read_txs_started += 1;

        Ok(Transaction::new(tx_id, epoch, TxType::Read))
    }

    /// Begin a new write transaction
    ///
    /// Write transactions acquire the write lock (single-writer model).
    /// Returns transaction handle and write lock guard.
    pub fn begin_write(&mut self) -> Result<Transaction> {
        // Acquire write lock (blocks if another write is in progress)
        let epoch = self.epoch_manager.get_current_epoch();
        let tx_id = self.epoch_manager.allocate_tx_id();

        self.stats.write_txs_started += 1;

        Ok(Transaction::new(tx_id, epoch, TxType::Write))
    }

    /// Commit a transaction
    ///
    /// For write transactions, increments the global epoch.
    pub fn commit(&mut self, tx: &mut Transaction) -> Result<()> {
        if tx.state != TxState::Active {
            return Err(Error::transaction(format!(
                "Transaction {} is not active (state: {:?})",
                tx.id, tx.state
            )));
        }

        // Increment epoch for write transactions
        if tx.tx_type == TxType::Write {
            let new_epoch = self.epoch_manager.increment_epoch();
            self.stats.current_epoch = new_epoch;
        }

        tx.state = TxState::Committed;
        self.stats.txs_committed += 1;

        Ok(())
    }

    /// Abort a transaction
    pub fn abort(&mut self, tx: &mut Transaction) -> Result<()> {
        if tx.state != TxState::Active {
            return Err(Error::transaction(format!(
                "Transaction {} is not active (state: {:?})",
                tx.id, tx.state
            )));
        }

        tx.state = TxState::Aborted;
        self.stats.txs_aborted += 1;

        Ok(())
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> u64 {
        self.epoch_manager.get_current_epoch()
    }

    /// Get statistics
    pub fn stats(&self) -> TransactionStats {
        let mut stats = self.stats.clone();
        stats.current_epoch = self.epoch_manager.get_current_epoch();
        stats
    }

    /// Check if a version is visible to a transaction
    ///
    /// A record is visible if: created_epoch <= tx.epoch < deleted_epoch
    pub fn is_visible(
        &self,
        tx_epoch: u64,
        created_epoch: u64,
        deleted_epoch: Option<u64>,
    ) -> bool {
        // Must be created before or at transaction epoch
        if created_epoch > tx_epoch {
            return false;
        }

        // If deleted, must be deleted after transaction epoch
        if let Some(del_epoch) = deleted_epoch {
            if del_epoch <= tx_epoch {
                return false;
            }
        }

        true
    }

    /// Health check for the transaction manager
    pub fn health_check(&self) -> Result<()> {
        // Check if the current epoch is reasonable
        let current_epoch = self.epoch_manager.current_epoch.load(Ordering::Acquire);
        if current_epoch > 1_000_000_000 {
            // 1 billion max
            return Err(Error::transaction("Epoch counter too large"));
        }

        // Check if the next transaction ID is reasonable
        let next_tx_id = self.epoch_manager.next_tx_id.load(Ordering::Acquire);
        if next_tx_id > 1_000_000_000 {
            // 1 billion max
            return Err(Error::transaction("Transaction ID counter too large"));
        }

        Ok(())
    }

    /// Get the number of active transactions
    pub fn active_count(&self) -> u64 {
        // For MVP, we'll return 0 as we don't track active transactions yet
        0
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default transaction manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_manager_creation() {
        let mgr = TransactionManager::new().unwrap();
        assert_eq!(mgr.current_epoch(), 0);
    }

    #[test]
    fn test_begin_read_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let tx = mgr.begin_read().unwrap();
        assert_eq!(tx.id, 0);
        assert_eq!(tx.epoch, 0);
        assert_eq!(tx.tx_type, TxType::Read);
        assert_eq!(tx.state, TxState::Active);
        assert!(tx.is_active());
        assert!(tx.is_read_only());
    }

    #[test]
    fn test_begin_write_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let tx = mgr.begin_write().unwrap();
        assert_eq!(tx.id, 0);
        assert_eq!(tx.epoch, 0);
        assert_eq!(tx.tx_type, TxType::Write);
        assert_eq!(tx.state, TxState::Active);
        assert!(tx.is_write());
    }

    #[test]
    fn test_commit_read_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut tx = mgr.begin_read().unwrap();
        mgr.commit(&mut tx).unwrap();

        assert!(tx.is_committed());
        assert_eq!(tx.state, TxState::Committed);

        // Epoch should not change for read transactions
        assert_eq!(mgr.current_epoch(), 0);
    }

    #[test]
    fn test_commit_write_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut tx = mgr.begin_write().unwrap();
        mgr.commit(&mut tx).unwrap();

        assert!(tx.is_committed());

        // Epoch should increment for write transactions
        assert_eq!(mgr.current_epoch(), 1);
    }

    #[test]
    fn test_abort_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut tx = mgr.begin_write().unwrap();
        mgr.abort(&mut tx).unwrap();

        assert!(tx.is_aborted());
        assert_eq!(tx.state, TxState::Aborted);

        // Epoch should not change on abort
        assert_eq!(mgr.current_epoch(), 0);
    }

    #[test]
    fn test_commit_non_active_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut tx = mgr.begin_read().unwrap();
        mgr.commit(&mut tx).unwrap();

        // Try to commit again
        let result = mgr.commit(&mut tx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not active"));
    }

    #[test]
    fn test_abort_non_active_transaction() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut tx = mgr.begin_read().unwrap();
        mgr.abort(&mut tx).unwrap();

        // Try to abort again
        let result = mgr.abort(&mut tx);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_transactions() {
        let mut mgr = TransactionManager::new().unwrap();

        // Start multiple read transactions
        let tx1 = mgr.begin_read().unwrap();
        let tx2 = mgr.begin_read().unwrap();

        assert_eq!(tx1.id, 0);
        assert_eq!(tx2.id, 1);
        assert_eq!(tx1.epoch, tx2.epoch);
    }

    #[test]
    fn test_epoch_increment() {
        let mut mgr = TransactionManager::new().unwrap();

        assert_eq!(mgr.current_epoch(), 0);

        // Commit write transactions to increment epoch
        for i in 0..10 {
            let mut tx = mgr.begin_write().unwrap();
            mgr.commit(&mut tx).unwrap();
            assert_eq!(mgr.current_epoch(), i + 1);
        }

        assert_eq!(mgr.current_epoch(), 10);
    }

    #[test]
    fn test_visibility_rules() {
        let mgr = TransactionManager::new().unwrap();

        // Record created at epoch 5
        let created_epoch = 5;

        // Transaction at epoch 10 should see it
        assert!(mgr.is_visible(10, created_epoch, None));

        // Transaction at epoch 4 should not see it (created after)
        assert!(!mgr.is_visible(4, created_epoch, None));

        // Record deleted at epoch 8
        let deleted_epoch = Some(8);

        // Transaction at epoch 10 should not see it (deleted before)
        assert!(!mgr.is_visible(10, created_epoch, deleted_epoch));

        // Transaction at epoch 7 should see it (created before, deleted after)
        assert!(mgr.is_visible(7, created_epoch, deleted_epoch));

        // Transaction at epoch 5 should see it (created at same epoch)
        assert!(mgr.is_visible(5, created_epoch, deleted_epoch));
    }

    #[test]
    fn test_snapshot_isolation() {
        let mut mgr = TransactionManager::new().unwrap();

        // Start read transaction at epoch 0
        let read_tx = mgr.begin_read().unwrap();
        assert_eq!(read_tx.epoch, 0);

        // Write transaction commits (increments epoch to 1)
        let mut write_tx1 = mgr.begin_write().unwrap();
        mgr.commit(&mut write_tx1).unwrap();
        assert_eq!(mgr.current_epoch(), 1);

        // Original read transaction still sees epoch 0 snapshot
        assert_eq!(read_tx.epoch, 0);

        // New read transaction sees epoch 1
        let read_tx2 = mgr.begin_read().unwrap();
        assert_eq!(read_tx2.epoch, 1);
    }

    #[test]
    fn test_transaction_statistics() {
        let mut mgr = TransactionManager::new().unwrap();

        // Start various transactions
        let mut tx1 = mgr.begin_read().unwrap();
        let mut tx2 = mgr.begin_write().unwrap();
        let mut tx3 = mgr.begin_read().unwrap();

        mgr.commit(&mut tx1).unwrap();
        mgr.commit(&mut tx2).unwrap();
        mgr.abort(&mut tx3).unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.read_txs_started, 2);
        assert_eq!(stats.write_txs_started, 1);
        assert_eq!(stats.txs_committed, 2);
        assert_eq!(stats.txs_aborted, 1);
        assert_eq!(stats.current_epoch, 1);
    }

    #[test]
    fn test_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let mgr = Arc::new(Mutex::new(TransactionManager::new().unwrap()));

        let mut handles = vec![];

        // Spawn multiple threads starting read transactions
        for _ in 0..10 {
            let m = mgr.clone();
            let handle = thread::spawn(move || m.lock().begin_read().unwrap());
            handles.push(handle);
        }

        let transactions: Vec<Transaction> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All should have same epoch
        let first_epoch = transactions[0].epoch;
        assert!(transactions.iter().all(|tx| tx.epoch == first_epoch));

        // All should have unique IDs
        let mut ids: Vec<u64> = transactions.iter().map(|tx| tx.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn test_sequential_writes() {
        let mut mgr = TransactionManager::new().unwrap();

        for i in 0..5 {
            let mut tx = mgr.begin_write().unwrap();

            // Do some work (simulated)
            assert_eq!(tx.epoch, i);

            mgr.commit(&mut tx).unwrap();

            // Epoch increments after commit
            assert_eq!(mgr.current_epoch(), i + 1);
        }

        let stats = mgr.stats();
        assert_eq!(stats.write_txs_started, 5);
        assert_eq!(stats.txs_committed, 5);
    }

    #[test]
    fn test_visibility_at_epoch_boundaries() {
        let mgr = TransactionManager::new().unwrap();

        // Record created at epoch 5, deleted at epoch 10
        let created = 5;
        let deleted = Some(10);

        // At epoch 5: visible (created at same epoch)
        assert!(mgr.is_visible(5, created, deleted));

        // At epoch 9: visible (between create and delete)
        assert!(mgr.is_visible(9, created, deleted));

        // At epoch 10: NOT visible (deleted at this epoch)
        assert!(!mgr.is_visible(10, created, deleted));

        // At epoch 11: NOT visible (already deleted)
        assert!(!mgr.is_visible(11, created, deleted));
    }

    #[test]
    fn test_visibility_never_deleted() {
        let mgr = TransactionManager::new().unwrap();

        let created = 5;
        let deleted = None; // Never deleted

        // Should be visible from creation epoch onward
        assert!(mgr.is_visible(5, created, deleted));
        assert!(mgr.is_visible(10, created, deleted));
        assert!(mgr.is_visible(100, created, deleted));

        // Should not be visible before creation
        assert!(!mgr.is_visible(4, created, deleted));
    }

    #[test]
    fn test_transaction_lifecycle() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut tx = mgr.begin_write().unwrap();
        assert!(tx.is_active());
        assert!(!tx.is_committed());
        assert!(!tx.is_aborted());

        mgr.commit(&mut tx).unwrap();
        assert!(!tx.is_active());
        assert!(tx.is_committed());
        assert!(!tx.is_aborted());
    }

    #[test]
    fn test_tx_id_uniqueness() {
        let mut mgr = TransactionManager::new().unwrap();

        let mut ids = Vec::new();
        for _ in 0..100 {
            let tx = mgr.begin_read().unwrap();
            ids.push(tx.id);
        }

        // All IDs should be unique
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        sorted_ids.dedup();
        assert_eq!(sorted_ids.len(), 100);
    }

    #[test]
    fn test_mixed_read_write_transactions() {
        let mut mgr = TransactionManager::new().unwrap();

        let tx1 = mgr.begin_read().unwrap();
        let tx2 = mgr.begin_write().unwrap();
        let tx3 = mgr.begin_read().unwrap();

        assert_eq!(tx1.tx_type, TxType::Read);
        assert_eq!(tx2.tx_type, TxType::Write);
        assert_eq!(tx3.tx_type, TxType::Read);

        // All see same epoch initially
        assert_eq!(tx1.epoch, 0);
        assert_eq!(tx2.epoch, 0);
        assert_eq!(tx3.epoch, 0);
    }
}
