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

use crate::simd::crc32c as simd_crc32c;
use crate::{Error, Result};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod async_wal;
pub use async_wal::{AsyncWalConfig, AsyncWalStats, AsyncWalStatsSnapshot, AsyncWalWriter};

/// Magic leading byte identifying a "v2" WAL frame that carries an
/// explicit `checksum_algo` field. Old v1 frames always start with a
/// non-zero `WalEntryType` (the enum does not define `0x00`), so a
/// zero byte unambiguously signals the new format.
const WAL_V2_MAGIC: u8 = 0x00;

/// Checksum algorithm identifiers stamped in v2 frames.
///
/// `Crc32Fast` is the legacy IEEE polynomial used by old files (and
/// by `crc32fast`). `Crc32C` is the Castagnoli polynomial used by
/// SSE4.2 / ARMv8 CRC, wrapped by `simd::crc32c`. New frames are
/// always written with `Crc32C`; the read path honours the stored
/// algo byte so old files replay unchanged.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgo {
    /// Legacy `crc32fast` (IEEE polynomial).
    Crc32Fast = 0x01,
    /// Hardware-accelerated CRC32C (Castagnoli).
    Crc32C = 0x02,
}

impl ChecksumAlgo {
    fn from_byte(b: u8) -> Result<Self> {
        match b {
            0x01 => Ok(Self::Crc32Fast),
            0x02 => Ok(Self::Crc32C),
            other => Err(Error::wal(format!(
                "Unknown WAL checksum algo: 0x{other:02x}"
            ))),
        }
    }
}

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
    /// Full-text index create (phase6_fulltext-wal-integration)
    FtsCreateIndex = 0x40,
    /// Full-text index drop
    FtsDropIndex = 0x41,
    /// Full-text document add
    FtsAdd = 0x42,
    /// Full-text document delete
    FtsDel = 0x43,
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
    /// phase6_fulltext-wal-integration — FTS index creation.
    /// Carries every field needed to rebuild the index on replay:
    /// name, entity scope, labels-or-types, properties, resolved
    /// analyzer display name (e.g. `"standard"` / `"ngram(3,5)"`).
    FtsCreateIndex {
        /// Registry name
        name: String,
        /// 0 = Node, 1 = Relationship (entity scope)
        entity: u8,
        /// Labels (for node scope) or types (for relationship scope)
        labels_or_types: Vec<String>,
        /// Indexed properties
        properties: Vec<String>,
        /// Resolved analyzer name as persisted in the registry meta
        analyzer: String,
    },
    /// FTS index drop.
    FtsDropIndex {
        /// Registry name
        name: String,
    },
    /// FTS document add.
    FtsAdd {
        /// Registry name
        name: String,
        /// Node (or relationship) id
        entity_id: u64,
        /// Label (or type) id this doc was registered under
        label_or_type_id: u32,
        /// Property key id
        key_id: u32,
        /// Indexed text payload
        content: String,
    },
    /// FTS document delete by entity id.
    FtsDel {
        /// Registry name
        name: String,
        /// Node (or relationship) id to remove
        entity_id: u64,
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
            Self::FtsCreateIndex { .. } => WalEntryType::FtsCreateIndex,
            Self::FtsDropIndex { .. } => WalEntryType::FtsDropIndex,
            Self::FtsAdd { .. } => WalEntryType::FtsAdd,
            Self::FtsDel { .. } => WalEntryType::FtsDel,
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

    /// Append an entry to the WAL.
    ///
    /// Writes a v2 frame:
    /// `[WAL_V2_MAGIC: 1][algo: 1][type: 1][length: 4][payload: N][crc: 4]`
    ///
    /// The checksum covers `[algo][type][length][payload]` — everything
    /// after the magic. The algo field lets us swap checksum algorithms
    /// in the future without breaking old files.
    ///
    /// v1 frames (written by older binaries) are recognised on read via
    /// a non-zero first byte and verified with the matching algorithm.
    ///
    /// Default algorithm choice: `Crc32Fast`. On modern x86_64 with
    /// PCLMUL, `crc32fast` runs a 3-way parallel CLMUL accumulator that
    /// hits ~15 GB/s — measurably faster than hardware CRC32C on the
    /// same hardware, which is sequential-instruction-bound at ~7 GB/s
    /// (see `benches/simd_crc.rs`). `Crc32C` stays available via the
    /// algo field for future migration when/if AVX-512 VPCLMULQDQ or
    /// a parallel CRC32C reduction makes it win.
    ///
    /// Returns the offset where the entry was written.
    pub fn append(&mut self, entry: &WalEntry) -> Result<u64> {
        self.append_with_algo(entry, ChecksumAlgo::Crc32Fast)
    }

    /// Test-only / future-migration path: pick the algo explicitly.
    pub(crate) fn append_with_algo(&mut self, entry: &WalEntry, algo: ChecksumAlgo) -> Result<u64> {
        let payload = bincode::serialize(entry)
            .map_err(|e| Error::wal(format!("Serialization failed: {}", e)))?;

        // Build entry buffer:
        //   [magic:1][algo:1][type:1][length:4][payload:N][crc:4]
        let mut buf = Vec::with_capacity(1 + 1 + 1 + 4 + payload.len() + 4);
        buf.push(WAL_V2_MAGIC);
        buf.push(algo as u8);
        buf.push(entry.entry_type() as u8);
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload);

        // Checksum covers every byte after the magic (algo..=payload).
        let crc = match algo {
            ChecksumAlgo::Crc32Fast => {
                let mut hasher = Hasher::new();
                hasher.update(&buf[1..]);
                hasher.finalize()
            }
            ChecksumAlgo::Crc32C => simd_crc32c::checksum(&buf[1..]),
        };
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
    /// Read and return all entries from the WAL file.
    ///
    /// Supports both v1 frames (written by older binaries with
    /// `crc32fast`) and v2 frames (written with `CRC32C` after the
    /// phase-3 SIMD rollout). The first byte of each frame
    /// disambiguates:
    ///
    /// * `0x00` → v2 frame: `[magic][algo][type][length][payload][crc]`
    /// * anything else → v1 frame: `[type][length][payload][crc]`
    pub fn recover(&mut self) -> Result<Vec<WalEntry>> {
        let mut entries = Vec::new();
        let mut file_offset = 0u64;

        // Seek to start
        self.file.seek(SeekFrom::Start(0))?;

        loop {
            let mut first = [0u8; 1];
            match self.file.read_exact(&mut first) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            // Header layout differs between v1 and v2; the hash range
            // also differs (v1 covers type+len+payload, v2 additionally
            // covers the algo byte written after the magic).
            let (algo_buf, type_buf, len_buf, payload, stored_crc, algo, frame_len, v2) =
                if first[0] == WAL_V2_MAGIC {
                    // v2 frame: [magic:1][algo:1][type:1][length:4][payload:N][crc:4]
                    let mut algo_buf = [0u8; 1];
                    self.file.read_exact(&mut algo_buf)?;
                    let algo = ChecksumAlgo::from_byte(algo_buf[0])?;

                    let mut type_buf = [0u8; 1];
                    self.file.read_exact(&mut type_buf)?;

                    let mut len_buf = [0u8; 4];
                    self.file.read_exact(&mut len_buf)?;
                    let payload_len = u32::from_le_bytes(len_buf) as usize;

                    let mut payload = vec![0u8; payload_len];
                    self.file.read_exact(&mut payload)?;

                    let mut crc_buf = [0u8; 4];
                    self.file.read_exact(&mut crc_buf)?;
                    let stored_crc = u32::from_le_bytes(crc_buf);

                    (
                        algo_buf,
                        type_buf,
                        len_buf,
                        payload,
                        stored_crc,
                        algo,
                        // magic + algo + type + length + payload + crc
                        1 + 1 + 1 + 4 + payload_len as u64 + 4,
                        true,
                    )
                } else {
                    // v1 frame: the byte we already read is the type byte.
                    let type_buf = first;

                    let mut len_buf = [0u8; 4];
                    self.file.read_exact(&mut len_buf)?;
                    let payload_len = u32::from_le_bytes(len_buf) as usize;

                    let mut payload = vec![0u8; payload_len];
                    self.file.read_exact(&mut payload)?;

                    let mut crc_buf = [0u8; 4];
                    self.file.read_exact(&mut crc_buf)?;
                    let stored_crc = u32::from_le_bytes(crc_buf);

                    (
                        [0u8; 1], // unused for v1
                        type_buf,
                        len_buf,
                        payload,
                        stored_crc,
                        ChecksumAlgo::Crc32Fast,
                        // type + length + payload + crc
                        1 + 4 + payload_len as u64 + 4,
                        false,
                    )
                };

            // Validate checksum under the algo stamped in the frame.
            // v2 frames include the algo byte in the hashed range; v1
            // frames do not have an algo byte at all.
            let computed_crc = match (algo, v2) {
                (ChecksumAlgo::Crc32Fast, true) => {
                    let mut hasher = Hasher::new();
                    hasher.update(&algo_buf);
                    hasher.update(&type_buf);
                    hasher.update(&len_buf);
                    hasher.update(&payload);
                    hasher.finalize()
                }
                (ChecksumAlgo::Crc32Fast, false) => {
                    let mut hasher = Hasher::new();
                    hasher.update(&type_buf);
                    hasher.update(&len_buf);
                    hasher.update(&payload);
                    hasher.finalize()
                }
                (ChecksumAlgo::Crc32C, true) => {
                    simd_crc32c::checksum_iovecs(&[&algo_buf, &type_buf, &len_buf, &payload])
                }
                (ChecksumAlgo::Crc32C, false) => {
                    // v1 frames never carry the CRC32C algo byte on
                    // the wire; this combination is impossible by
                    // construction.
                    return Err(Error::wal(format!(
                        "internal: v1 frame at offset {} tagged CRC32C",
                        file_offset
                    )));
                }
            };

            if stored_crc != computed_crc {
                return Err(Error::wal(format!(
                    "CRC mismatch at offset {} (algo={:?}): expected {:x}, got {:x}",
                    file_offset, algo, stored_crc, computed_crc
                )));
            }

            // Deserialize entry
            let entry: WalEntry = bincode::deserialize(&payload)
                .map_err(|e| Error::wal(format!("Deserialization failed: {}", e)))?;

            entries.push(entry);
            self.stats.entries_read += 1;

            file_offset += frame_len;
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

    /// Hand-crafted legacy v1 frame: `[type:1][len:4][payload:N][crc32fast:4]`,
    /// no magic byte, no algo tag. Proves the reader still accepts
    /// files written by pre-SIMD binaries.
    fn write_legacy_v1_frame(path: &std::path::Path, entry: &WalEntry) {
        use std::io::Write;
        let payload = bincode::serialize(entry).unwrap();
        let mut buf = Vec::new();
        buf.push(entry.entry_type() as u8);
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&payload);
        let mut hasher = Hasher::new();
        hasher.update(&buf);
        let crc = hasher.finalize();
        buf.extend_from_slice(&crc.to_le_bytes());
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        f.write_all(&buf).unwrap();
        f.sync_all().unwrap();
    }

    #[test]
    fn legacy_v1_frame_recovers_without_magic() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal-v1.log");

        // Write two v1 frames by hand.
        write_legacy_v1_frame(
            &path,
            &WalEntry::BeginTx {
                tx_id: 7,
                epoch: 42,
            },
        );
        write_legacy_v1_frame(
            &path,
            &WalEntry::CreateNode {
                node_id: 999,
                label_bits: 0x3,
            },
        );

        // Open the file through the regular WAL and recover.
        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            WalEntry::BeginTx { tx_id, epoch } => {
                assert_eq!(*tx_id, 7);
                assert_eq!(*epoch, 42);
            }
            _ => panic!("expected BeginTx"),
        }
        match &entries[1] {
            WalEntry::CreateNode {
                node_id,
                label_bits,
            } => {
                assert_eq!(*node_id, 999);
                assert_eq!(*label_bits, 0x3);
            }
            _ => panic!("expected CreateNode"),
        }
    }

    #[test]
    fn v2_frame_with_crc32c_roundtrips() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal-crc32c.log");
        {
            let mut wal = Wal::new(&path).unwrap();
            wal.append_with_algo(
                &WalEntry::BeginTx {
                    tx_id: 3,
                    epoch: 55,
                },
                ChecksumAlgo::Crc32C,
            )
            .unwrap();
            wal.append_with_algo(
                &WalEntry::CreateNode {
                    node_id: 77,
                    label_bits: 0xF,
                },
                ChecksumAlgo::Crc32C,
            )
            .unwrap();
            wal.flush().unwrap();
        }
        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(matches!(
            entries[0],
            WalEntry::BeginTx {
                tx_id: 3,
                epoch: 55
            }
        ));
        assert!(matches!(
            entries[1],
            WalEntry::CreateNode { node_id: 77, .. }
        ));
    }

    #[test]
    fn mixed_v1_then_v2_frames_replay_cleanly() {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal-mixed.log");

        // Prepend a v1 frame written by the legacy helper.
        write_legacy_v1_frame(
            &path,
            &WalEntry::BeginTx {
                tx_id: 1,
                epoch: 100,
            },
        );

        // Append two v2 frames via the production writer.
        {
            let mut wal = Wal::new(&path).unwrap();
            wal.append(&WalEntry::CreateNode {
                node_id: 200,
                label_bits: 0x1,
            })
            .unwrap();
            wal.append(&WalEntry::CommitTx {
                tx_id: 1,
                epoch: 100,
            })
            .unwrap();
            wal.flush().unwrap();
        }

        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], WalEntry::BeginTx { tx_id: 1, .. }));
        assert!(matches!(
            entries[1],
            WalEntry::CreateNode { node_id: 200, .. }
        ));
        assert!(matches!(entries[2], WalEntry::CommitTx { tx_id: 1, .. }));
    }

    // phase6_fulltext-wal-integration — FTS op-code round-trip.
    #[test]
    fn fts_wal_ops_encode_decode_roundtrip() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("fts.wal");
        let mut wal = Wal::new(&path).unwrap();

        let create = WalEntry::FtsCreateIndex {
            name: "movies".to_string(),
            entity: 0,
            labels_or_types: vec!["Movie".to_string()],
            properties: vec!["title".to_string(), "overview".to_string()],
            analyzer: "standard".to_string(),
        };
        let add = WalEntry::FtsAdd {
            name: "movies".to_string(),
            entity_id: 42,
            label_or_type_id: 0,
            key_id: 0,
            content: "The Matrix".to_string(),
        };
        let del = WalEntry::FtsDel {
            name: "movies".to_string(),
            entity_id: 42,
        };
        let drop = WalEntry::FtsDropIndex {
            name: "movies".to_string(),
        };
        for e in [&create, &add, &del, &drop] {
            wal.append(e).unwrap();
        }
        wal.flush().unwrap();
        drop_wal(wal);

        let mut wal = Wal::new(&path).unwrap();
        let entries = wal.recover().unwrap();
        assert_eq!(entries.len(), 4);
        match &entries[0] {
            WalEntry::FtsCreateIndex {
                name,
                entity,
                labels_or_types,
                properties,
                analyzer,
            } => {
                assert_eq!(name, "movies");
                assert_eq!(*entity, 0);
                assert_eq!(labels_or_types, &vec!["Movie".to_string()]);
                assert_eq!(
                    properties,
                    &vec!["title".to_string(), "overview".to_string()]
                );
                assert_eq!(analyzer, "standard");
            }
            other => panic!("expected FtsCreateIndex, got {other:?}"),
        }
        assert!(matches!(entries[1], WalEntry::FtsAdd { entity_id: 42, .. }));
        assert!(matches!(entries[2], WalEntry::FtsDel { entity_id: 42, .. }));
        match &entries[3] {
            WalEntry::FtsDropIndex { name } => assert_eq!(name, "movies"),
            other => panic!("expected FtsDropIndex, got {other:?}"),
        }
    }

    fn drop_wal(_w: Wal) {
        // Explicit drop helper — required because `Wal` holds a file
        // handle that we need closed before reopening for recovery.
    }
}
