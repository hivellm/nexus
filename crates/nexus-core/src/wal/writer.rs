//! Synchronous WAL writer, recovery, and checkpointing.
//!
//! The [`Wal`] struct owns the file handle and exposes:
//! * [`Wal::append`] / [`Wal::append_with_algo`] — write frames (v2 plaintext
//!   or v3 encrypted).
//! * [`Wal::recover`] — replay all frames from disk after a crash.
//! * [`Wal::checkpoint`] / [`Wal::truncate`] — compaction helpers.
//!
//! All crypto helpers (`WAL_V2_MAGIC`, `V3FrameOutcome`, `build_v3_aad`,
//! `map_aead_err_for_append`) are private to this module.

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

use super::record::{ChecksumAlgo, WalEntry, WalStats};

// ──────────────────────────────────────────────────────────────────────────────
// Private crypto helpers
// ──────────────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────────────
// Wal
// ──────────────────────────────────────────────────────────────────────────────

/// Write-Ahead Log manager
pub struct Wal {
    /// WAL file path
    pub(super) path: PathBuf,

    /// WAL file handle (shared via Arc to prevent file descriptor leaks)
    pub(super) file: Arc<File>,

    /// Current offset in file
    pub(super) offset: u64,

    /// Statistics
    pub(super) stats: WalStats,

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
    pub(super) cipher: Option<Arc<PageCipher>>,

    /// Byte offset where the first frame begins. `0` for plaintext
    /// WALs; [`PAGE_HEADER_LEN`] for encrypted WALs that prefixed
    /// the file with the EaR magic.
    pub(super) frames_start: u64,
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
