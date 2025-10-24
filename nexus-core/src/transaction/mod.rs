//! Transaction layer - MVCC, locking, isolation
//!
//! MVCC via epoch-based snapshots:
//! - Readers pin an epoch/snapshot ID
//! - Writers generate new records (append-only) and update pointers on commit
//!
//! Locking via parking_lot:
//! - Single writer per partition (queue) initially
//! - Group commit for batching

use crate::Result;

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

/// Transaction handle
pub struct Transaction {
    /// Transaction ID
    pub id: u64,
    /// Snapshot epoch
    pub epoch: u64,
    /// Transaction state
    pub state: TxState,
}

impl Transaction {
    /// Begin a new transaction
    pub fn begin(_epoch: u64) -> Result<Self> {
        todo!("Transaction::begin - to be implemented in MVP")
    }

    /// Commit the transaction
    pub fn commit(&mut self) -> Result<()> {
        todo!("commit - to be implemented in MVP")
    }

    /// Abort the transaction
    pub fn abort(&mut self) -> Result<()> {
        todo!("abort - to be implemented in MVP")
    }
}

/// Transaction manager
pub struct TransactionManager {
    // Will use parking_lot for locking
}

impl TransactionManager {
    /// Create a new transaction manager
    pub fn new() -> Result<Self> {
        todo!("TransactionManager::new - to be implemented in MVP")
    }

    /// Begin a new read transaction
    pub fn begin_read(&mut self) -> Result<Transaction> {
        todo!("begin_read - to be implemented in MVP")
    }

    /// Begin a new write transaction
    pub fn begin_write(&mut self) -> Result<Transaction> {
        todo!("begin_write - to be implemented in MVP")
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default transaction manager")
    }
}
