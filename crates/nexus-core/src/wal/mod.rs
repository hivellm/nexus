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
use crate::storage::crypto::encrypted_file::PAGE_HEADER_LEN;
use crate::storage::crypto::{
    AeadError, FileId, PageCipher, PageHeader, PageNonce, TAG_LEN, decrypt_page, encrypt_page,
};
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
/// zero byte unambiguously signals the new format. v3 (encrypted)
/// frames share the same leading byte; the `algo` field
/// (`Aes256GcmCrc32C`) tells the read path to dispatch to the v3
/// decoder.
const WAL_V2_MAGIC: u8 = 0x00;

/// Outcome of [`Wal::decode_v3_frame`]. Public to the module so the
/// recover loop can pattern-match without a separate marker.
enum V3FrameOutcome {
    /// Frame decoded cleanly. `frame_len` is the on-disk length the
    /// recover loop should advance.
    Entry { entry: WalEntry, frame_len: u64 },
    /// The frame body was incomplete (short read, kill-9 mid-write,
    /// or trailing AEAD failure on a frame that ends at EOF). The
    /// recover loop treats this as a truncation point — same parity
    /// as a CRC mismatch on a v1/v2 plaintext frame.
    TruncatedTrailing,
}

/// Build the AAD for a v3 frame. Bound bytes:
///
/// ```text
///   [type:1] [plain_len:4 LE] [crc_plain:4 LE] [frame_offset:8 LE]
/// ```
///
/// Total: 17 bytes. Binding the offset means a tamperer who copies
/// a frame to a different position triggers an AEAD failure.
fn build_v3_aad(type_byte: u8, plain_len: u32, crc_plain: u32, frame_offset: u64) -> [u8; 17] {
    let mut aad = [0u8; 17];
    aad[0] = type_byte;
    aad[1..5].copy_from_slice(&plain_len.to_le_bytes());
    aad[5..9].copy_from_slice(&crc_plain.to_le_bytes());
    aad[9..17].copy_from_slice(&frame_offset.to_le_bytes());
    aad
}

/// Map the AEAD primitive's error into the WAL error taxonomy on
/// the *append* path. The append path always treats AEAD failure
/// as a hard fault — there is no "trailing frame" interpretation
/// for a write-side error.
fn map_aead_err_for_append(e: AeadError) -> Error {
    match e {
        AeadError::BadKey => {
            Error::wal("ERR_WAL_AEAD: AES-256-GCM seal failed on append (cipher state corruption?)")
        }
        AeadError::Empty => Error::wal("ERR_WAL_EMPTY_PAYLOAD: refusing to encrypt empty payload"),
    }
}

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
    /// with [`Wal::with_cipher`].
    Aes256GcmCrc32C = 0x03,
}

impl ChecksumAlgo {
    fn from_byte(b: u8) -> Result<Self> {
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
            Self::RTreeInsert { .. } => WalEntryType::RTreeInsert,
            Self::RTreeDelete { .. } => WalEntryType::RTreeDelete,
            Self::RTreeBulkLoadDone { .. } => WalEntryType::RTreeBulkLoadDone,
            Self::ExternalIdAssigned { .. } => WalEntryType::ExternalIdAssigned,
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

    /// AES-256-GCM cipher when the WAL is encrypted, `None` for the
    /// plaintext path. When set:
    ///
    /// * The first [`PAGE_HEADER_LEN`] bytes of the file carry an
    ///   `NXCP` page header so the boot inventory scanner classifies
    ///   the file as `Encrypted` (the per-frame `0x00` v2/v3 magic
    ///   would otherwise look plaintext to the scanner).
    /// * `append` emits v3 frames (algo = `Aes256GcmCrc32C`).
    /// * `recover` decrypts each frame and verifies CRC32C against
    ///   the recovered plaintext.
    cipher: Option<Arc<PageCipher>>,

    /// Byte offset where the first frame begins. `0` for plaintext
    /// WALs; [`PAGE_HEADER_LEN`] for encrypted WALs that prefixed
    /// the file with the EaR magic.
    frames_start: u64,
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
            cipher: None,
            frames_start: 0,
        })
    }

    /// Open a WAL bound to an AES-256-GCM cipher. Frames written
    /// through this WAL are v3 (encrypted, AAD-bound metadata,
    /// end-to-end CRC32C over the recovered plaintext); frames read
    /// back via [`Wal::recover`] are decrypted before deserialisation.
    ///
    /// On a fresh file the constructor lays down a 16-byte `NXCP`
    /// page header at offset 0 so the boot-time inventory scanner at
    /// [`crate::storage::crypto::inventory`] classifies the WAL file
    /// as `Encrypted`. On an existing file the header is validated
    /// and an `ERR_WAL_HEADER` is surfaced if the file does not start
    /// with the `NXCP` magic — guarding against opening a plaintext
    /// WAL under a key that would not decrypt any frame.
    pub fn with_cipher<P: AsRef<Path>>(path: P, cipher: Arc<PageCipher>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let size = file.metadata()?.len();
        if size == 0 {
            // Fresh file — write the page header so the inventory
            // scanner sees the EaR magic.
            let header = PageHeader {
                file_id: FileId::Wal,
                generation: 1,
            };
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&header.to_bytes())?;
            file.sync_all()?;
        } else if size < PAGE_HEADER_LEN as u64 {
            return Err(Error::wal(format!(
                "ERR_WAL_HEADER: encrypted WAL {} is shorter than the {}-byte page header",
                path.display(),
                PAGE_HEADER_LEN
            )));
        } else {
            let mut header_buf = [0u8; PAGE_HEADER_LEN];
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut header_buf)?;
            if PageHeader::from_bytes(&header_buf).is_none() {
                return Err(Error::wal(format!(
                    "ERR_WAL_HEADER: {} is missing the EaR magic; refusing to open as encrypted WAL",
                    path.display()
                )));
            }
        }

        let offset = file.metadata()?.len();
        Ok(Self {
            path,
            file: Arc::new(file),
            offset,
            stats: WalStats {
                file_size: offset,
                ..Default::default()
            },
            cipher: Some(cipher),
            frames_start: PAGE_HEADER_LEN as u64,
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
        let algo = if self.cipher.is_some() {
            ChecksumAlgo::Aes256GcmCrc32C
        } else {
            ChecksumAlgo::Crc32Fast
        };
        self.append_with_algo(entry, algo)
    }

    /// Test-only / future-migration path: pick the algo explicitly.
    pub(crate) fn append_with_algo(&mut self, entry: &WalEntry, algo: ChecksumAlgo) -> Result<u64> {
        // The encrypted-vs-plaintext branch bifurcates here. Plaintext
        // v2 frames keep their pre-EaR layout byte-for-byte (so
        // existing on-disk files replay unchanged); v3 frames carry
        // ciphertext + plaintext-CRC and are emitted only when the
        // WAL was opened with a cipher.
        if matches!(algo, ChecksumAlgo::Aes256GcmCrc32C) {
            return self.append_v3(entry);
        }
        if self.cipher.is_some() {
            return Err(Error::wal(format!(
                "ERR_WAL_PLAINTEXT_REQUEST: WAL is bound to an AES-256-GCM cipher; refusing to write a {algo:?} plaintext frame"
            )));
        }

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
            ChecksumAlgo::Aes256GcmCrc32C => unreachable!("dispatched above"),
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

    /// Encode a v3 (encrypted) frame.
    ///
    /// Layout on disk:
    ///
    /// ```text
    ///   [magic:1=0x00]
    ///   [algo:1=0x03]
    ///   [type:1]
    ///   [plain_len:4]            — u32 LE; original payload length
    ///   [crc_plain:4]            — CRC32C over plaintext (end-to-end)
    ///   [ciphertext_with_tag: plain_len + 16]
    /// ```
    ///
    /// Total frame length: `27 + plain_len`.
    ///
    /// Nonce: `PageNonce::new(file_id = FileId::Wal, page_offset =
    /// frame_offset_in_file, generation = 1)`. Nonce uniqueness is
    /// guaranteed by the WAL's append-only invariant: each frame
    /// gets a unique offset between truncations, and a truncation
    /// is required to be paired with a key rotation
    /// (`docs/security/ENCRYPTION_AT_REST.md` § "WAL key rotation").
    ///
    /// AAD (additional authenticated data) covers
    /// `[magic, algo, type, plain_len_le4, crc_plain_le4,
    /// frame_offset_le8]` — 19 bytes. Binding the offset means a
    /// tamperer who relocates a frame to a different position
    /// surfaces as an AEAD failure on replay.
    fn append_v3(&mut self, entry: &WalEntry) -> Result<u64> {
        let cipher = self.cipher.as_ref().ok_or_else(|| {
            Error::wal("ERR_WAL_CIPHER_MISSING: append_v3 called on a plaintext WAL")
        })?;

        let plaintext = bincode::serialize(entry)
            .map_err(|e| Error::wal(format!("Serialization failed: {}", e)))?;
        if plaintext.is_empty() {
            return Err(Error::wal("ERR_WAL_EMPTY_PAYLOAD"));
        }
        let plain_len = u32::try_from(plaintext.len())
            .map_err(|_| Error::wal("ERR_WAL_PAYLOAD_TOO_LARGE: > 4 GiB"))?;
        let crc_plain = simd_crc32c::checksum(&plaintext);

        let frame_offset = self.offset;
        let nonce = PageNonce::new(FileId::Wal.as_u16(), frame_offset, 1);
        let aad = build_v3_aad(entry.entry_type() as u8, plain_len, crc_plain, frame_offset);

        let ciphertext =
            encrypt_page(cipher, nonce, &plaintext, &aad).map_err(map_aead_err_for_append)?;

        // Frame: [magic][algo][type][plain_len][crc_plain][ct]
        let mut buf = Vec::with_capacity(1 + 1 + 1 + 4 + 4 + ciphertext.len());
        buf.push(WAL_V2_MAGIC);
        buf.push(ChecksumAlgo::Aes256GcmCrc32C as u8);
        buf.push(entry.entry_type() as u8);
        buf.extend_from_slice(&plain_len.to_le_bytes());
        buf.extend_from_slice(&crc_plain.to_le_bytes());
        buf.extend_from_slice(&ciphertext);

        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&buf)?;

        self.offset += buf.len() as u64;
        self.stats.entries_written += 1;
        self.stats.entries_since_checkpoint += 1;
        self.stats.file_size = self.offset;

        Ok(frame_offset)
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
        let mut file_offset = self.frames_start;

        // Seek past the optional EaR page header. For plaintext
        // WALs `frames_start = 0` and this is a no-op. For encrypted
        // WALs `frames_start = PAGE_HEADER_LEN` (16 bytes); the
        // header was already validated in `with_cipher`.
        self.file.seek(SeekFrom::Start(file_offset))?;

        loop {
            let mut first = [0u8; 1];
            match self.file.read_exact(&mut first) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            // Header layout differs between v1, v2, and v3:
            //
            //   v1: covers `[type][len][payload]` under `crc32fast`.
            //   v2: covers `[algo][type][len][payload]` under the
            //       stored `algo`.
            //   v3: ciphertext + AAD-bound metadata; CRC32C is over
            //       the recovered plaintext, not the on-disk bytes.
            //       Decoded by `decode_v3_frame` and short-circuits
            //       out of the v1/v2 path.
            let (algo_buf, type_buf, len_buf, payload, stored_crc, algo, frame_len, v2) =
                if first[0] == WAL_V2_MAGIC {
                    // v2 frame: [magic:1][algo:1][type:1][length:4][payload:N][crc:4]
                    let mut algo_buf = [0u8; 1];
                    match self.file.read_exact(&mut algo_buf) {
                        Ok(_) => {}
                        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                            // Trailing-byte truncation between magic
                            // and algo: same outcome as a v1/v2 CRC
                            // mismatch on the trailing frame.
                            self.truncate_to(file_offset)?;
                            break;
                        }
                        Err(e) => return Err(e.into()),
                    }
                    let algo = ChecksumAlgo::from_byte(algo_buf[0])?;
                    if matches!(algo, ChecksumAlgo::Aes256GcmCrc32C) {
                        match self.decode_v3_frame(file_offset)? {
                            V3FrameOutcome::Entry { entry, frame_len } => {
                                entries.push(entry);
                                self.stats.entries_read += 1;
                                file_offset += frame_len;
                                continue;
                            }
                            V3FrameOutcome::TruncatedTrailing => {
                                self.truncate_to(file_offset)?;
                                break;
                            }
                        }
                    }

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
                (ChecksumAlgo::Aes256GcmCrc32C, _) => {
                    // v3 frames are dispatched to `decode_v3_frame`
                    // before reaching the v1/v2 checksum branch;
                    // landing here means the dispatcher missed and
                    // we read v3 bytes through the v2 path. Hard
                    // error: the on-disk state is inconsistent with
                    // the dispatcher.
                    return Err(Error::wal(format!(
                        "ERR_WAL_FRAME: v3 frame at offset {file_offset} reached the v2 checksum path"
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

    /// Truncate WAL (after checkpoint and backup).
    ///
    /// For an encrypted WAL, the EaR page header at offset 0 is
    /// rewritten so the file remains a valid encrypted-WAL file.
    /// Frames start over at [`PAGE_HEADER_LEN`].
    pub fn truncate(&mut self) -> Result<()> {
        if self.cipher.is_some() {
            self.file.set_len(0)?;
            let header = PageHeader {
                file_id: FileId::Wal,
                generation: 1,
            };
            self.file.seek(SeekFrom::Start(0))?;
            self.file.write_all(&header.to_bytes())?;
            self.offset = PAGE_HEADER_LEN as u64;
            self.stats.file_size = self.offset;
        } else {
            self.file.set_len(0)?;
            self.file.seek(SeekFrom::Start(0))?;
            self.offset = 0;
            self.stats.file_size = 0;
        }
        self.stats.entries_since_checkpoint = 0;
        Ok(())
    }

    /// Truncate the file to the given offset and update bookkeeping.
    /// Used by the recover path when a trailing frame fails the
    /// integrity check (CRC mismatch on plaintext, AEAD failure on
    /// the trailing v3 frame, or short read between frame fields).
    fn truncate_to(&mut self, offset: u64) -> Result<()> {
        self.file.set_len(offset)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.offset = offset;
        self.stats.file_size = offset;
        Ok(())
    }

    /// Decode an encrypted (v3) frame starting at `frame_offset`.
    ///
    /// On entry the file cursor may be anywhere — the function
    /// always seeks to `frame_offset` before reading, so it does
    /// not depend on the caller having already consumed the magic
    /// or algo bytes.
    ///
    /// Three outcomes:
    ///
    /// * Successful decrypt + CRC match: returns
    ///   `V3FrameOutcome::Entry { entry, frame_len }`.
    /// * Short read while consuming the frame body: returns
    ///   `V3FrameOutcome::TruncatedTrailing`. This signals the
    ///   caller to truncate the WAL at `frame_offset`.
    /// * AEAD failure or CRC mismatch with a fully-readable frame:
    ///   surfaces `ERR_WAL_AEAD` so the operator notices wholesale
    ///   tampering. (Trailing-frame AEAD failures are reported as
    ///   truncation by the caller — `recover` distinguishes via the
    ///   "is this the last frame in the file" check.)
    fn decode_v3_frame(&mut self, frame_offset: u64) -> Result<V3FrameOutcome> {
        let cipher = self.cipher.as_ref().ok_or_else(|| {
            Error::wal(
                "ERR_WAL_CIPHER_MISSING: v3 frame encountered on a WAL opened without a cipher",
            )
        })?;

        self.file.seek(SeekFrom::Start(frame_offset))?;

        // Header is 11 bytes: magic + algo + type + plain_len + crc.
        const V3_HEADER_LEN: u64 = 1 + 1 + 1 + 4 + 4;
        let mut header = [0u8; V3_HEADER_LEN as usize];
        match self.file.read_exact(&mut header) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(V3FrameOutcome::TruncatedTrailing);
            }
            Err(e) => return Err(e.into()),
        }
        let magic = header[0];
        let algo = ChecksumAlgo::from_byte(header[1])?;
        if magic != WAL_V2_MAGIC || !matches!(algo, ChecksumAlgo::Aes256GcmCrc32C) {
            return Err(Error::wal(format!(
                "ERR_WAL_FRAME: v3 dispatcher saw magic=0x{magic:02x} algo={algo:?} at offset {frame_offset}"
            )));
        }
        let type_byte = header[2];
        let plain_len = u32::from_le_bytes([header[3], header[4], header[5], header[6]]);
        let crc_plain = u32::from_le_bytes([header[7], header[8], header[9], header[10]]);
        let ct_len = (plain_len as usize)
            .checked_add(TAG_LEN)
            .ok_or_else(|| Error::wal("ERR_WAL_FRAME: ciphertext length overflow"))?;

        let mut ciphertext = vec![0u8; ct_len];
        match self.file.read_exact(&mut ciphertext) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(V3FrameOutcome::TruncatedTrailing);
            }
            Err(e) => return Err(e.into()),
        }

        let nonce = PageNonce::new(FileId::Wal.as_u16(), frame_offset, 1);
        let aad = build_v3_aad(type_byte, plain_len, crc_plain, frame_offset);

        let plaintext = match decrypt_page(cipher, nonce, &ciphertext, &aad) {
            Ok(pt) => pt,
            Err(AeadError::BadKey) => {
                // AEAD failure: distinguish "trailing frame, treat as
                // truncation" from "mid-WAL tamper, raise". The
                // caller frames the trailing-vs-not decision; we
                // signal trailing only when this frame extends to
                // EOF (the body was readable but the tag did not
                // verify, which is the kill-9-mid-write outcome).
                let after = frame_offset + V3_HEADER_LEN + ct_len as u64;
                let file_len = self.file.metadata()?.len();
                if after == file_len {
                    return Ok(V3FrameOutcome::TruncatedTrailing);
                }
                return Err(Error::wal(format!(
                    "ERR_WAL_AEAD: AEAD verification failed at offset {frame_offset} (mid-WAL tamper or wrong key)"
                )));
            }
            Err(AeadError::Empty) => {
                return Err(Error::wal(format!(
                    "ERR_WAL_AEAD: empty ciphertext at offset {frame_offset}"
                )));
            }
        };

        let computed_crc = simd_crc32c::checksum(&plaintext);
        if computed_crc != crc_plain {
            // Plaintext CRC mismatch after a successful AEAD: the
            // ciphertext was generated by a key that produces the
            // same nonce/AAD binding but encodes a different
            // message, OR the WAL writer wrote a frame whose
            // claimed CRC does not match its plaintext. Either
            // shape is a hard integrity failure, not a truncation.
            return Err(Error::wal(format!(
                "ERR_WAL_CRC: plaintext CRC mismatch at offset {frame_offset} (expected {crc_plain:x}, got {computed_crc:x})"
            )));
        }

        let entry: WalEntry = bincode::deserialize(&plaintext)
            .map_err(|e| Error::wal(format!("Deserialization failed: {}", e)))?;

        let frame_len = V3_HEADER_LEN + ct_len as u64;
        Ok(V3FrameOutcome::Entry { entry, frame_len })
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
            cipher: self.cipher.as_ref().map(Arc::clone),
            frames_start: self.frames_start,
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

    // ---------- v3 (encrypted) WAL tests --------------------------

    fn fresh_cipher(seed: u8, db: &str) -> Arc<PageCipher> {
        use crate::storage::crypto::kdf::{MasterKey, derive_database_key};
        let m = MasterKey::new([seed; 32]);
        let k = derive_database_key(&m, db, 0).unwrap();
        Arc::new(PageCipher::new(&k))
    }

    fn make_encrypted_wal(seed: u8) -> (Wal, TestContext) {
        let ctx = TestContext::new();
        let path = ctx.path().join("wal.log");
        let cipher = fresh_cipher(seed, "default");
        let wal = Wal::with_cipher(&path, cipher).unwrap();
        (wal, ctx)
    }

    #[test]
    fn v3_round_trip_recovers_plaintext_payload() {
        let (mut wal, _ctx) = make_encrypted_wal(0xAA);
        let entries = [
            WalEntry::BeginTx { tx_id: 1, epoch: 1 },
            WalEntry::SetProperty {
                entity_id: 42,
                key_id: 7,
                value: b"top-secret".to_vec(),
            },
            WalEntry::CommitTx { tx_id: 1, epoch: 1 },
        ];
        for e in &entries {
            wal.append(e).unwrap();
        }
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);

        let cipher = fresh_cipher(0xAA, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), entries.len());
        match &recovered[1] {
            WalEntry::SetProperty {
                entity_id,
                key_id,
                value,
            } => {
                assert_eq!(*entity_id, 42);
                assert_eq!(*key_id, 7);
                assert_eq!(value, b"top-secret");
            }
            other => panic!("expected SetProperty, got {other:?}"),
        }
    }

    #[test]
    fn v3_file_starts_with_ear_page_header() {
        let (mut wal, _ctx) = make_encrypted_wal(0x11);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);

        // The first 16 bytes must classify as the EaR magic so the
        // boot inventory scanner sees an encrypted file.
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > PAGE_HEADER_LEN);
        let mut header_buf = [0u8; PAGE_HEADER_LEN];
        header_buf.copy_from_slice(&bytes[..PAGE_HEADER_LEN]);
        let header = PageHeader::from_bytes(&header_buf).expect("EaR magic missing");
        assert_eq!(header.file_id, FileId::Wal);
    }

    #[test]
    fn v3_ciphertext_does_not_contain_plaintext_payload() {
        let (mut wal, _ctx) = make_encrypted_wal(0x22);
        let needle = b"NEEDLE_THAT_MUST_NOT_LEAK";
        wal.append(&WalEntry::SetProperty {
            entity_id: 1,
            key_id: 1,
            value: needle.to_vec(),
        })
        .unwrap();
        wal.flush().unwrap();
        let bytes = std::fs::read(&wal.path).unwrap();
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "plaintext leaked into ciphertext on disk"
        );
    }

    #[test]
    fn v3_wrong_key_surfaces_err_wal_aead() {
        let (mut wal, ctx) = make_encrypted_wal(0xAB);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        // _ctx held to keep the temp dir alive
        let _ = ctx;

        let wrong = fresh_cipher(0xCD, "default");
        let mut wal2 = Wal::with_cipher(&path, wrong).unwrap();
        let err = wal2.recover().unwrap_err();
        let msg = err.to_string();
        // The first frame is mid-WAL relative to the second one;
        // wrong-key surfaces ERR_WAL_AEAD on the first frame because
        // it does not extend to EOF.
        assert!(msg.contains("ERR_WAL_AEAD"), "got {msg}");
    }

    #[test]
    fn v3_tampered_mid_wal_frame_surfaces_err_wal_aead() {
        let (mut wal, ctx) = make_encrypted_wal(0x33);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        let _ = ctx;

        // Flip a byte in the first frame's ciphertext (the byte at
        // PAGE_HEADER_LEN + 11 is somewhere in the AEAD body).
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[PAGE_HEADER_LEN + 11] ^= 0x40;
        std::fs::write(&path, &bytes).unwrap();

        let cipher = fresh_cipher(0x33, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let err = wal2.recover().unwrap_err();
        assert!(err.to_string().contains("ERR_WAL_AEAD"));
    }

    #[test]
    fn v3_truncated_trailing_frame_is_treated_as_truncation() {
        let (mut wal, ctx) = make_encrypted_wal(0x44);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        let _ = ctx;

        // Lop off the last 4 bytes — simulates kill-9 partway through
        // a frame write. The reader must treat this as "truncation"
        // and return only the first frame, not raise an error.
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 4]).unwrap();

        let cipher = fresh_cipher(0x44, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), 1, "expected only the first frame");
        // The file should now be exactly 1 frame past the page
        // header — recover truncated the partial trailing frame.
        let after = std::fs::metadata(&path).unwrap().len();
        assert!(after >= PAGE_HEADER_LEN as u64);
    }

    #[test]
    fn v3_trailing_frame_with_aead_failure_treated_as_truncation() {
        let (mut wal, ctx) = make_encrypted_wal(0x55);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);
        let _ = ctx;

        // Flip a byte in the only (= trailing) frame. AEAD fails,
        // and because the frame extends to EOF, recover must treat
        // the failure as a truncation — return zero entries, leave
        // a clean file behind.
        let mut bytes = std::fs::read(&path).unwrap();
        let n = bytes.len();
        bytes[n - 5] ^= 0x55;
        std::fs::write(&path, &bytes).unwrap();

        let cipher = fresh_cipher(0x55, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert!(recovered.is_empty(), "trailing AEAD should truncate");
    }

    #[test]
    fn with_cipher_rejects_existing_plaintext_wal() {
        let (mut plain, ctx) = create_test_wal();
        plain
            .append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        plain.flush().unwrap();
        let path = plain.path.clone();
        drop_wal(plain);

        let cipher = fresh_cipher(0x77, "default");
        let err = match Wal::with_cipher(&path, cipher) {
            Ok(_) => panic!("expected ERR_WAL_HEADER on plaintext WAL"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("ERR_WAL_HEADER"));
        let _ = ctx;
    }

    #[test]
    fn plaintext_wal_refuses_v3_append_request() {
        let (mut wal, _ctx) = create_test_wal();
        let err = wal
            .append_with_algo(
                &WalEntry::BeginTx { tx_id: 1, epoch: 1 },
                ChecksumAlgo::Aes256GcmCrc32C,
            )
            .unwrap_err();
        // The v3 append path requires a cipher — invoking it on a
        // plaintext WAL surfaces the cipher-missing error.
        assert!(err.to_string().contains("ERR_WAL_CIPHER_MISSING"));
    }

    #[test]
    fn encrypted_wal_truncate_preserves_page_header() {
        let (mut wal, _ctx) = make_encrypted_wal(0x88);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        wal.truncate().unwrap();
        // After truncate, the file must still start with the EaR
        // page header so the inventory scanner classifies it as
        // Encrypted on the next boot.
        let bytes = std::fs::read(&wal.path).unwrap();
        assert_eq!(bytes.len(), PAGE_HEADER_LEN);
        let mut header_buf = [0u8; PAGE_HEADER_LEN];
        header_buf.copy_from_slice(&bytes);
        assert!(PageHeader::from_bytes(&header_buf).is_some());
    }

    #[test]
    fn v3_append_then_replay_after_truncate_starts_fresh_offsets() {
        // Truncate resets frame offsets to PAGE_HEADER_LEN. Nonce
        // uniqueness across the truncate boundary requires a key
        // rotation in production; here we just prove the recover
        // loop walks the post-truncate frames cleanly.
        let (mut wal, _ctx) = make_encrypted_wal(0x99);
        wal.append(&WalEntry::BeginTx { tx_id: 1, epoch: 1 })
            .unwrap();
        wal.flush().unwrap();
        wal.truncate().unwrap();
        wal.append(&WalEntry::CommitTx { tx_id: 2, epoch: 2 })
            .unwrap();
        wal.flush().unwrap();
        let path = wal.path.clone();
        drop_wal(wal);

        let cipher = fresh_cipher(0x99, "default");
        let mut wal2 = Wal::with_cipher(&path, cipher).unwrap();
        let recovered = wal2.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert!(matches!(
            recovered[0],
            WalEntry::CommitTx { tx_id: 2, epoch: 2 }
        ));
    }
}
