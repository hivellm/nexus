//! Write-Ahead Log (WAL) - Transaction durability
//!
//! All mutations go through WAL before page table updates.
//! Supports MVCC via epoch-based snapshots.
//! Periodic checkpoints truncate WAL and compact pages.
//!
//! # Format
//!
//! WAL Entry: [epoch:8][tx_id:8][type:1][length:4][payload:N][crc32:4]
//!
//! Entry types:
//! - 0x01: BeginTx
//! - 0x02: CommitTx
//! - 0x03: AbortTx
//! - 0x10: CreateNode
//! - 0x11: DeleteNode
//! - 0x20: CreateRel
//! - 0x21: DeleteRel
//! - 0x30: SetProperty
//! - 0xFF: Checkpoint

use crate::{Error, Result};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod async_wal;
pub use async_wal::{AsyncWalConfig, AsyncWalStats, AsyncWalWriter};

/// WAL entry types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalEntryType {
    /// Begin transaction
    BeginTx = 0x01,
    /// Commit transaction
    CommitTx = 0x02,
    /// Abort transaction
    AbortTx = 0x03,
    /// Create node
    CreateNode = 0x10,
    /// Delete node
    DeleteNode = 0x11,
    /// Create relationship
    CreateRel = 0x20,
    /// Delete relationship
    DeleteRel = 0x21,
    /// Set property
    SetProperty = 0x30,
    /// Delete property
    DeleteProperty = 0x31,
    /// Checkpoint marker
    Checkpoint = 0xFF,
}

/// WAL entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WalEntry {
    /// Begin transaction
    BeginTx {
        /// Transaction ID
        tx_id: u64,
        /// Epoch
        epoch: u64,
    },
    /// Commit transaction
    CommitTx {
        /// Transaction ID
        tx_id: u64,
        /// Epoch
        epoch: u64,
    },
    /// Abort transaction
    AbortTx {
        /// Transaction ID
        tx_id: u64,
        /// Epoch
        epoch: u64,
    },
    /// Node creation
    CreateNode {
        /// Node ID
        node_id: u64,
        /// Label bitmap
        label_bits: u64,
    },
    /// Delete node
    DeleteNode {
        /// Node ID
        node_id: u64,
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
    /// Delete relationship
    DeleteRel {
        /// Relationship ID
        rel_id: u64,
    },
    /// Property update
    SetProperty {
        /// Entity ID (node or rel)
        entity_id: u64,
        /// Property key ID
        key_id: u32,
        /// Property value (serialized)
        value: Vec<u8>,
    },
    /// Delete property
    DeleteProperty {
        /// Entity ID
        entity_id: u64,
        /// Property key ID
        key_id: u32,
    },
    /// Checkpoint marker
    Checkpoint {
        /// Epoch ID
        epoch: u64,
    },
}

impl WalEntry {
    /// Get entry type
    fn entry_type(&self) -> WalEntryType {
        match self {
            Self::BeginTx { .. } => WalEntryType::BeginTx,
            Self::CommitTx { .. } => WalEntryType::CommitTx,
            Self::AbortTx { .. } => WalEntryType::AbortTx,
            Self::CreateNode { .. } => WalEntryType::CreateNode,
            Self::DeleteNode { .. } => WalEntryType::DeleteNode,
            Self::CreateRel { .. } => WalEntryType::CreateRel,
            Self::DeleteRel { .. } => WalEntryType::DeleteRel,
            Self::SetProperty { .. } => WalEntryType::SetProperty,
            Self::DeleteProperty { .. } => WalEntryType::DeleteProperty,
            Self::Checkpoint { .. } => WalEntryType::Checkpoint,
        }
    }

    /// Get transaction ID (if applicable)
    fn tx_id(&self) -> Option<u64> {
        match self {
            Self::BeginTx { tx_id, .. }
            | Self::CommitTx { tx_id, .. }
            | Self::AbortTx { tx_id, .. } => Some(*tx_id),
            _ => None,
        }
    }

    /// Get epoch (if applicable)
    fn epoch(&self) -> Option<u64> {
        match self {
            Self::BeginTx { epoch, .. }
            | Self::CommitTx { epoch, .. }
            | Self::AbortTx { epoch, .. }
            | Self::Checkpoint { epoch } => Some(*epoch),
            _ => None,
        }
    }
}

/// WAL statistics
#[derive(Debug, Clone, Default)]
pub struct WalStats {
    /// Total entries written
    pub entries_written: u64,
    /// Total entries read (during recovery)
    pub entries_read: u64,
    /// Total checkpoints
    pub checkpoints: u64,
    /// Current WAL file size
    pub file_size: u64,
    /// Number of entries since last checkpoint
    pub entries_since_checkpoint: u64,
}

/// Write-Ahead Log manager
pub struct Wal {
    /// WAL file path
    path: PathBuf,

    /// WAL file handle (shared via Arc to prevent file descriptor leaks)
    file: Arc<File>,

    /// Current offset in file
    offset: u64,

    /// Statistics
    stats: WalStats,
}

impl Wal {
    /// Create a new WAL
    ///
    /// # Arguments
    ///
    /// * `path` - Path to WAL file
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::wal::Wal;
    ///
    /// let wal = Wal::new("./data/wal.log").unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open or create WAL file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        // Get current size
        let metadata = file.metadata()?;
        let offset = metadata.len();

        Ok(Self {
            path,
            file: Arc::new(file),
            offset,
            stats: WalStats {
                file_size: offset,
                ..Default::default()
            },
        })
    }

    /// Append an entry to the WAL
    ///
    /// Returns the offset where the entry was written.
    pub fn append(&mut self, entry: &WalEntry) -> Result<u64> {
        // Serialize payload with bincode
        let payload = bincode::serialize(entry)
            .map_err(|e| Error::wal(format!("Serialization failed: {}", e)))?;

        // Build entry buffer: [type:1][length:4][payload:N]
        let mut buf = Vec::with_capacity(1 + 4 + payload.len() + 4);
        buf.push(entry.entry_type() as u8);
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload);

        // Compute CRC32 of everything so far
        let mut hasher = Hasher::new();
        hasher.update(&buf);
        let crc = hasher.finalize();
        buf.extend_from_slice(&crc.to_le_bytes());

        // Write to file
        let entry_offset = self.offset;
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&buf)?;

        // Update offset and stats
        self.offset += buf.len() as u64;
        self.stats.entries_written += 1;
        self.stats.entries_since_checkpoint += 1;
        self.stats.file_size = self.offset;

        Ok(entry_offset)
    }

    /// Flush WAL to disk (fsync)
    pub fn flush(&mut self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }

    /// Reopen WAL file (useful for recovery from permission errors)
    ///
    /// This method attempts to reopen the WAL file, which can help recover
    /// from temporary permission issues or file locking problems.
    pub fn reopen(&mut self) -> Result<()> {
        // Close current file handle
        // Note: File will be closed when dropped

        // Reopen the file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)?;

        // Get current size (in case file was truncated or modified)
        let metadata = file.metadata()?;
        let current_size = metadata.len();

        // Update offset to current file size
        self.offset = current_size;
        self.file = Arc::new(file);

        Ok(())
    }

    /// Create a checkpoint
    ///
    /// Writes checkpoint marker and flushes to disk.
    pub fn checkpoint(&mut self, epoch: u64) -> Result<()> {
        let entry = WalEntry::Checkpoint { epoch };
        self.append(&entry)?;
        self.flush()?;

        self.stats.checkpoints += 1;
        self.stats.entries_since_checkpoint = 0;

        Ok(())
    }

    /// Recover from WAL after crash
    ///
    /// Reads and returns all entries from WAL file.
    pub fn recover(&mut self) -> Result<Vec<WalEntry>> {
        let mut entries = Vec::new();
        let mut file_offset = 0u64;

        // Seek to start
        self.file.seek(SeekFrom::Start(0))?;

        loop {
            // Read entry type (1 byte)
            let mut type_buf = [0u8; 1];
            match self.file.read_exact(&mut type_buf) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            // Read payload length (4 bytes)
            let mut len_buf = [0u8; 4];
            self.file.read_exact(&mut len_buf)?;
            let payload_len = u32::from_le_bytes(len_buf) as usize;

            // Read payload
            let mut payload = vec![0u8; payload_len];
            self.file.read_exact(&mut payload)?;

            // Read CRC32
            let mut crc_buf = [0u8; 4];
            self.file.read_exact(&mut crc_buf)?;
            let stored_crc = u32::from_le_bytes(crc_buf);

            // Validate CRC32
            let mut hasher = Hasher::new();
            hasher.update(&type_buf);
            hasher.update(&len_buf);
            hasher.update(&payload);
            let computed_crc = hasher.finalize();

            if stored_crc != computed_crc {
                return Err(Error::wal(format!(
                    "CRC mismatch at offset {}: expected {:x}, got {:x}",
                    file_offset, stored_crc, computed_crc
                )));
            }

            // Deserialize entry
            let entry: WalEntry = bincode::deserialize(&payload)
                .map_err(|e| Error::wal(format!("Deserialization failed: {}", e)))?;

            entries.push(entry);
            self.stats.entries_read += 1;

            // Update offset
            file_offset += 1 + 4 + payload_len as u64 + 4;
        }

        Ok(entries)
    }

    /// Get WAL statistics
    pub fn stats(&self) -> WalStats {
        self.stats.clone()
    }

    /// Get current WAL file size
    pub fn file_size(&self) -> u64 {
        self.stats.file_size
    }

    /// Get WAL file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Truncate WAL (after checkpoint and backup)
    pub fn truncate(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.offset = 0;
        self.stats.file_size = 0;
        self.stats.entries_since_checkpoint = 0;
        Ok(())
    }

    /// Health check for the WAL
    pub fn health_check(&self) -> Result<()> {
        // Check if the WAL file is accessible
        if !self.path.exists() {
            return Err(Error::wal("WAL file does not exist"));
        }

        // Check if we can read from the file
        let mut file = File::open(&self.path)?;
        let _ = file.seek(SeekFrom::Start(0))?;

        // Check if the file size is reasonable
        let metadata = file.metadata()?;
        if metadata.len() > 1024 * 1024 * 1024 {
            // 1GB max
            return Err(Error::wal("WAL file too large"));
        }

        Ok(())
    }

    /// Get the number of WAL entries
    pub fn entry_count(&self) -> u64 {
        self.stats.entries_written
    }
}

impl Clone for Wal {
    fn clone(&self) -> Self {
        // Clone the WAL by sharing file handle via Arc
        // This prevents file descriptor leaks during testing
        Self {
            path: self.path.clone(),
            file: Arc::clone(&self.file),
            offset: self.offset,
            stats: self.stats.clone(),
        }
    }
}

impl Default for Wal {
    fn default() -> Self {
        Self::new("./data/wal.log").expect("Failed to create default WAL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;

    fn create_test_wal() -> (Wal, TestContext) {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");
        let wal = Wal::new(&path).unwrap();
        (wal, ctx)
    }

    #[test]
    fn test_wal_creation() {
        let (wal, _dir) = create_test_wal();
        assert_eq!(wal.offset, 0);
        assert_eq!(wal.stats.entries_written, 0);
    }

    #[test]
    fn test_append_entry() {
        let (mut wal, _dir) = create_test_wal();

        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };

        let offset = wal.append(&entry).unwrap();
        assert_eq!(offset, 0);
        assert_eq!(wal.stats.entries_written, 1);
        assert!(wal.stats.file_size > 0);
    }

    #[test]
    fn test_append_multiple_entries() {
        let (mut wal, _dir) = create_test_wal();

        for i in 0..10 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 1 << i,
            };
            wal.append(&entry).unwrap();
        }

        assert_eq!(wal.stats.entries_written, 10);
    }

    #[test]
    fn test_flush() {
        let (mut wal, _dir) = create_test_wal();

        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };

        wal.append(&entry).unwrap();
        wal.flush().unwrap();

        // Flush should not change stats
        assert_eq!(wal.stats.entries_written, 1);
    }

    #[test]
    fn test_checkpoint() {
        let (mut wal, _dir) = create_test_wal();

        // Write some entries
        for i in 0..5 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            };
            wal.append(&entry).unwrap();
        }

        assert_eq!(wal.stats.entries_since_checkpoint, 5);

        // Checkpoint
        wal.checkpoint(100).unwrap();

        assert_eq!(wal.stats.checkpoints, 1);
        assert_eq!(wal.stats.entries_since_checkpoint, 0);
    }

    #[test]
    fn test_recover_empty_wal() {
        let (mut wal, _dir) = create_test_wal();

        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_recover_with_entries() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");

        // Write entries
        {
            let mut wal = Wal::new(&path).unwrap();

            wal.append(&WalEntry::BeginTx {
                tx_id: 1,
                epoch: 100,
            })
            .unwrap();

            wal.append(&WalEntry::CreateNode {
                node_id: 42,
                label_bits: 5,
            })
            .unwrap();

            wal.append(&WalEntry::CommitTx {
                tx_id: 1,
                epoch: 100,
            })
            .unwrap();

            wal.flush().unwrap();
        }

        // Recover
        {
            let mut wal = Wal::new(&path).unwrap();
            let entries = wal.recover().unwrap();

            assert_eq!(entries.len(), 3);
            assert_eq!(wal.stats.entries_read, 3);

            // Verify entry types
            match &entries[0] {
                WalEntry::BeginTx { tx_id, epoch } => {
                    assert_eq!(*tx_id, 1);
                    assert_eq!(*epoch, 100);
                }
                _ => panic!("Expected BeginTx"),
            }

            match &entries[1] {
                WalEntry::CreateNode {
                    node_id,
                    label_bits,
                } => {
                    assert_eq!(*node_id, 42);
                    assert_eq!(*label_bits, 5);
                }
                _ => panic!("Expected CreateNode"),
            }

            match &entries[2] {
                WalEntry::CommitTx { tx_id, .. } => {
                    assert_eq!(*tx_id, 1);
                }
                _ => panic!("Expected CommitTx"),
            }
        }
    }

    #[test]
    fn test_entry_types() {
        let entry1 = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };
        assert_eq!(entry1.entry_type() as u8, 0x01);

        let entry2 = WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert_eq!(entry2.entry_type() as u8, 0x10);

        let entry3 = WalEntry::Checkpoint { epoch: 100 };
        assert_eq!(entry3.entry_type() as u8, 0xFF);
    }

    #[test]
    fn test_truncate() {
        let (mut wal, _dir) = create_test_wal();

        // Write entries
        for i in 0..10 {
            wal.append(&WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            })
            .unwrap();
        }

        assert!(wal.stats.file_size > 0);

        // Truncate
        wal.truncate().unwrap();

        assert_eq!(wal.offset, 0);
        assert_eq!(wal.stats.file_size, 0);
        assert_eq!(wal.stats.entries_since_checkpoint, 0);
    }

    #[test]
    fn test_all_entry_types_serialization() {
        let (mut wal, _dir) = create_test_wal();

        let entries = vec![
            WalEntry::BeginTx {
                tx_id: 1,
                epoch: 100,
            },
            WalEntry::CreateNode {
                node_id: 42,
                label_bits: 7,
            },
            WalEntry::DeleteNode { node_id: 43 },
            WalEntry::CreateRel {
                rel_id: 1,
                src: 10,
                dst: 20,
                type_id: 5,
            },
            WalEntry::DeleteRel { rel_id: 2 },
            WalEntry::SetProperty {
                entity_id: 42,
                key_id: 1,
                value: b"test value".to_vec(),
            },
            WalEntry::DeleteProperty {
                entity_id: 42,
                key_id: 1,
            },
            WalEntry::CommitTx {
                tx_id: 1,
                epoch: 100,
            },
            WalEntry::AbortTx {
                tx_id: 2,
                epoch: 101,
            },
            WalEntry::Checkpoint { epoch: 100 },
        ];

        // Write all entries
        for entry in &entries {
            wal.append(entry).unwrap();
        }

        wal.flush().unwrap();

        // Recover and verify
        let mut wal2 = Wal::new(&wal.path).unwrap();
        let recovered = wal2.recover().unwrap();

        assert_eq!(recovered.len(), entries.len());
    }

    #[test]
    fn test_crc_corruption_detection() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");

        // Write valid entry
        {
            let mut wal = Wal::new(&path).unwrap();
            wal.append(&WalEntry::CreateNode {
                node_id: 1,
                label_bits: 0,
            })
            .unwrap();
            wal.flush().unwrap();
        }

        // Corrupt the file (change a byte in the middle)
        {
            let mut file = OpenOptions::new().write(true).open(&path).unwrap();
            file.seek(SeekFrom::Start(10)).unwrap();
            file.write_all(&[0xFF]).unwrap();
            file.sync_all().unwrap();
        }

        // Recovery should detect corruption
        {
            let mut wal = Wal::new(&path).unwrap();
            let result = wal.recover();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("CRC"));
        }
    }

    #[test]
    fn test_transaction_sequence() {
        let (mut wal, _dir) = create_test_wal();

        // Simulate transaction: begin → create node → commit
        wal.append(&WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        })
        .unwrap();

        wal.append(&WalEntry::CreateNode {
            node_id: 42,
            label_bits: 1,
        })
        .unwrap();

        wal.append(&WalEntry::CreateRel {
            rel_id: 1,
            src: 42,
            dst: 43,
            type_id: 1,
        })
        .unwrap();

        wal.append(&WalEntry::CommitTx {
            tx_id: 1,
            epoch: 100,
        })
        .unwrap();

        assert_eq!(wal.stats.entries_written, 4);
    }

    #[test]
    fn test_entry_tx_id() {
        let entry = WalEntry::BeginTx {
            tx_id: 123,
            epoch: 1,
        };
        assert_eq!(entry.tx_id(), Some(123));

        let entry2 = WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert_eq!(entry2.tx_id(), None);
    }

    #[test]
    fn test_entry_epoch() {
        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 999,
        };
        assert_eq!(entry.epoch(), Some(999));

        let entry2 = WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert_eq!(entry2.epoch(), None);
    }

    #[test]
    fn test_stats() {
        let (mut wal, _dir) = create_test_wal();

        wal.append(&WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        })
        .unwrap();

        let stats = wal.stats();
        assert_eq!(stats.entries_written, 1);
        assert!(stats.file_size > 0);
    }

    #[test]
    fn test_large_payload() {
        let (mut wal, _dir) = create_test_wal();

        // Large property value (1MB)
        let large_value = vec![0xAB; 1024 * 1024];

        let entry = WalEntry::SetProperty {
            entity_id: 1,
            key_id: 1,
            value: large_value.clone(),
        };

        wal.append(&entry).unwrap();
        wal.flush().unwrap();

        // Recover and verify
        let mut wal2 = Wal::new(&wal.path).unwrap();
        let recovered = wal2.recover().unwrap();

        assert_eq!(recovered.len(), 1);
        match &recovered[0] {
            WalEntry::SetProperty { value, .. } => {
                assert_eq!(value.len(), 1024 * 1024);
                assert_eq!(value[0], 0xAB);
            }
            _ => panic!("Expected SetProperty"),
        }
    }
}
