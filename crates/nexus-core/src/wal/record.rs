//! WAL record types: entry-type discriminants, entry variants, and write statistics.
//!
//! This module is pure data — no I/O, no file handles, no crypto.  All
//! types defined here are re-exported from the parent `wal` module so
//! that the public surface (`crate::wal::WalEntry`, etc.) remains
//! unchanged.

use crate::{Error, Result};

// ──────────────────────────────────────────────────────────────────────────────
// Checksum algorithm identifiers (shared between writer and recovery paths)
// ──────────────────────────────────────────────────────────────────────────────

/// Checksum algorithm identifiers stamped in v2 / v3 frames.
///
/// `Crc32Fast` is the legacy IEEE polynomial used by old files (and
/// by `crc32fast`). `Crc32C` is the Castagnoli polynomial used by
/// SSE4.2 / ARMv8 CRC, wrapped by `simd::crc32c`. `Aes256GcmCrc32C`
/// flags a v3 frame: the payload is AES-256-GCM ciphertext, and the
/// trailing 4-byte checksum is CRC32C taken over the *plaintext*
/// (the proposal's "end-to-end integrity over plaintext" contract).
/// Old frames replay unchanged because the read path honours the
/// stored algo byte before deciding which decoder to run.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgo {
    /// Legacy `crc32fast` (IEEE polynomial). v2 plaintext frames.
    Crc32Fast = 0x01,
    /// Hardware-accelerated CRC32C (Castagnoli). v2 plaintext frames.
    Crc32C = 0x02,
    /// AES-256-GCM ciphertext + CRC32C over the recovered plaintext.
    /// v3 encrypted frames; only emitted when the WAL was opened
    /// with [`crate::wal::Wal::with_cipher`].
    Aes256GcmCrc32C = 0x03,
}

impl ChecksumAlgo {
    pub(super) fn from_byte(b: u8) -> Result<Self> {
        match b {
            0x01 => Ok(Self::Crc32Fast),
            0x02 => Ok(Self::Crc32C),
            0x03 => Ok(Self::Aes256GcmCrc32C),
            other => Err(Error::wal(format!(
                "Unknown WAL checksum algo: 0x{other:02x}"
            ))),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry-type discriminants
// ──────────────────────────────────────────────────────────────────────────────

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
    /// R-tree leaf insert (phase6_rtree-index-core §6).
    RTreeInsert = 0x50,
    /// R-tree leaf delete.
    RTreeDelete = 0x51,
    /// R-tree bulk-load completed marker.
    RTreeBulkLoadDone = 0x52,
    /// External-id assignment (phase9_external-node-ids §3.2).
    /// Appended immediately after the paired `CreateNode` entry so that
    /// crash recovery can rebuild the catalog external-id index even if the
    /// LMDB write had not been flushed to disk.
    ExternalIdAssigned = 0x60,
    /// Checkpoint marker
    Checkpoint = 0xFF,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry variants
// ──────────────────────────────────────────────────────────────────────────────

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
    /// R-tree leaf insert (phase6_rtree-index-core §6.1).
    /// Carries every field needed to replay the insert: which
    /// index the entry belongs to, the owning node id, and the
    /// 2-D point coordinates. The replay dispatcher routes this
    /// to `RTreeRegistry::apply_wal_entry`.
    RTreeInsert {
        /// Registry name (`{label}.{property}` per the spec).
        index_name: String,
        /// Owning node id.
        node_id: u64,
        /// X coordinate.
        x: f64,
        /// Y coordinate.
        y: f64,
    },
    /// R-tree leaf delete.
    RTreeDelete {
        /// Registry name.
        index_name: String,
        /// Owning node id.
        node_id: u64,
    },
    /// R-tree bulk-load completion marker. Replay sees this when
    /// a bulk-rebuild ran to completion before shutdown; absence
    /// of this marker for a journalled bulk-load means the
    /// rebuild was interrupted and the index must be re-built
    /// from scratch on recovery.
    RTreeBulkLoadDone {
        /// Registry name.
        index_name: String,
        /// Root page id of the freshly-built tree.
        root_page_id: u64,
    },
    /// External-id assignment (phase9_external-node-ids §3.2).
    ///
    /// Emitted immediately after the corresponding `CreateNode` entry
    /// whenever a caller-supplied external id was stored.  On crash
    /// recovery the replay path calls
    /// `catalog.external_id_index().put_if_absent` with these values
    /// to rebuild the catalog mapping independently of whether the
    /// LMDB environment had been synced before the crash.
    ///
    /// `external_id_bytes` is the wire encoding produced by
    /// [`crate::catalog::external_id::ExternalId::to_bytes`].
    ExternalIdAssigned {
        /// The internal node id the mapping points to.
        internal_id: u64,
        /// Wire-encoded external id (discriminator + payload).
        external_id_bytes: Vec<u8>,
    },
}

impl WalEntry {
    /// Get entry type
    pub(super) fn entry_type(&self) -> WalEntryType {
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
            Self::RTreeInsert { .. } => WalEntryType::RTreeInsert,
            Self::RTreeDelete { .. } => WalEntryType::RTreeDelete,
            Self::RTreeBulkLoadDone { .. } => WalEntryType::RTreeBulkLoadDone,
            Self::ExternalIdAssigned { .. } => WalEntryType::ExternalIdAssigned,
        }
    }

    /// Get transaction ID (if applicable)
    pub(super) fn tx_id(&self) -> Option<u64> {
        match self {
            Self::BeginTx { tx_id, .. }
            | Self::CommitTx { tx_id, .. }
            | Self::AbortTx { tx_id, .. } => Some(*tx_id),
            _ => None,
        }
    }

    /// Get epoch (if applicable)
    pub(super) fn epoch(&self) -> Option<u64> {
        match self {
            Self::BeginTx { epoch, .. }
            | Self::CommitTx { epoch, .. }
            | Self::AbortTx { epoch, .. }
            | Self::Checkpoint { epoch } => Some(*epoch),
            _ => None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Statistics
// ──────────────────────────────────────────────────────────────────────────────

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
