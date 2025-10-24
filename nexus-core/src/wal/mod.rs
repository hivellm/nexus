//! Write-Ahead Log (WAL) - Transaction durability
//!
//! All mutations go through WAL before page table updates.
//! Supports MVCC via epoch-based snapshots.
//! Periodic checkpoints truncate WAL and compact pages.

use crate::{Error, Result};

/// WAL entry types
#[derive(Debug, Clone)]
pub enum WalEntry {
    /// Node creation
    CreateNode {
        /// Node ID
        node_id: u64,
        /// Labels
        labels: Vec<u32>,
    },
    /// Relationship creation
    CreateRel {
        /// Relationship ID
        rel_id: u64,
        /// Source node ID
        src: u64,
        /// Destination node ID
        dst: u64,
        /// Type ID
        type_id: u32,
    },
    /// Property update
    SetProperty {
        /// Entity ID (node or rel)
        entity_id: u64,
        /// Property key ID
        key_id: u32,
        /// Property value
        value: Vec<u8>,
    },
    /// Checkpoint marker
    Checkpoint {
        /// Epoch ID
        epoch: u64,
    },
}

/// Write-Ahead Log manager
pub struct Wal {
    // Will use append-only file with periodic checkpoints
}

impl Wal {
    /// Create a new WAL
    pub fn new() -> Result<Self> {
        todo!("Wal::new - to be implemented in MVP")
    }

    /// Append an entry to the WAL
    pub fn append(&mut self, _entry: WalEntry) -> Result<u64> {
        todo!("append - to be implemented in MVP")
    }

    /// Flush WAL to disk (fsync)
    pub fn flush(&mut self) -> Result<()> {
        todo!("flush - to be implemented in MVP")
    }

    /// Create a checkpoint
    pub fn checkpoint(&mut self, _epoch: u64) -> Result<()> {
        todo!("checkpoint - to be implemented in MVP")
    }

    /// Recover from WAL after crash
    pub fn recover(&mut self) -> Result<Vec<WalEntry>> {
        todo!("recover - to be implemented in MVP")
    }
}

impl Default for Wal {
    fn default() -> Self {
        Self::new().expect("Failed to create default WAL")
    }
}
