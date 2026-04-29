//! Page-level encrypted-file abstraction.
//!
//! [`EncryptedPageStream`] is the *seam* the storage hooks plug
//! into. It owns:
//!
//! * A [`PageCipher`] bound to the active per-database key.
//! * A monotonic generation counter per `(file_id, page_offset)`
//!   so nonce uniqueness is guaranteed across writes.
//! * A standard page header layout that records the generation +
//!   integrity metadata in plaintext so the decrypt path can
//!   reconstruct the nonce without an external sidecar.
//!
//! The stream itself is byte-oriented; it does not know what's
//! inside a page (record store, WAL frame, B-tree leaf). The
//! caller frames the data and chooses a `file_id` from the well-
//! known set documented in [`FileId`].
//!
//! This module **does not** wire into LMDB / record stores / WAL /
//! indexes — that's tracked under `phase8_encryption-at-rest-
//! storage-hooks` and friends. The contract here is stable; the
//! follow-up work consumes it without changing any public type.

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

use super::aes_gcm::{NONCE_LEN, PageCipher, PageNonce, TAG_LEN, decrypt_page, encrypt_page};

/// Standard page size in bytes. Storage hooks use 8 KiB pages
/// today; record stores still operate on smaller record granules
/// internally. Encrypting at the page level rather than the record
/// level limits per-AEAD overhead to one tag per 8 KiB.
pub const PAGE_SIZE: usize = 8192;

/// Page-header length on disk, in bytes. The header is **plaintext**
/// — it carries the generation counter the decrypt path needs to
/// reconstruct the nonce, plus a fixed magic number for crash
/// recovery.
pub const PAGE_HEADER_LEN: usize = 16;

const HEADER_MAGIC: u32 = 0x4E58_4350; // "NXCP" — Nexus Crypto Page

/// Identifier for the on-disk file the page lives in. Used as the
/// 16-bit `file_id` field of the AEAD nonce. The numeric values
/// are stable wire-protocol numbers — a future format reshuffle
/// would bump the [`super::kdf::KDF_DOMAIN_TAG`] constant rather
/// than mutate these.
#[repr(u16)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FileId {
    /// LMDB catalog (label / type / key mappings).
    Catalog = 1,
    /// Node record store.
    NodeStore = 2,
    /// Relationship record store.
    RelStore = 3,
    /// Property record store.
    PropertyStore = 4,
    /// String store.
    StringStore = 5,
    /// Write-ahead log segment.
    Wal = 6,
    /// B-tree property index.
    BTreeIndex = 7,
    /// Full-text Tantivy segment (one segment file per index).
    FullTextIndex = 8,
    /// HNSW KNN index.
    KnnIndex = 9,
    /// R-tree spatial index.
    RTreeIndex = 10,
}

impl FileId {
    #[must_use]
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Per-page header laid down at the front of every encrypted page.
///
/// Layout on disk (little-endian, total = [`PAGE_HEADER_LEN`]):
///
/// ```text
///   [0..4]   magic  (u32) = HEADER_MAGIC
///   [4..6]   file_id (u16)
///   [6..10]  generation (u32)
///   [10..16] reserved (must be zero)
/// ```
///
/// The `(file_id, page_offset, generation)` triple drives the AEAD
/// nonce. `page_offset` is the file offset where the page starts
/// — never serialised explicitly because the reader already knows
/// it from the seek position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageHeader {
    pub file_id: FileId,
    pub generation: u32,
}

impl PageHeader {
    /// Encode to disk bytes.
    pub fn to_bytes(self) -> [u8; PAGE_HEADER_LEN] {
        let mut buf = [0u8; PAGE_HEADER_LEN];
        buf[0..4].copy_from_slice(&HEADER_MAGIC.to_le_bytes());
        buf[4..6].copy_from_slice(&self.file_id.as_u16().to_le_bytes());
        buf[6..10].copy_from_slice(&self.generation.to_le_bytes());
        // bytes 10..16 stay zero — reserved for a future flags word.
        buf
    }

    /// Decode from disk bytes. Returns `None` on a magic mismatch
    /// — useful for crash-recovery code that scans for valid pages.
    #[must_use]
    pub fn from_bytes(buf: &[u8; PAGE_HEADER_LEN]) -> Option<Self> {
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != HEADER_MAGIC {
            return None;
        }
        let file_id_raw = u16::from_le_bytes([buf[4], buf[5]]);
        let file_id = match file_id_raw {
            1 => FileId::Catalog,
            2 => FileId::NodeStore,
            3 => FileId::RelStore,
            4 => FileId::PropertyStore,
            5 => FileId::StringStore,
            6 => FileId::Wal,
            7 => FileId::BTreeIndex,
            8 => FileId::FullTextIndex,
            9 => FileId::KnnIndex,
            10 => FileId::RTreeIndex,
            _ => return None,
        };
        let generation = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
        Some(Self {
            file_id,
            generation,
        })
    }
}

/// Errors the page stream can surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PageStreamError {
    /// AEAD primitive rejected the (de)cipher.
    #[error(transparent)]
    Aead(#[from] super::aes_gcm::AeadError),
    /// On-disk page header is malformed.
    #[error("ERR_PAGE_HEADER: missing or corrupt magic")]
    BadHeader,
    /// Caller asked to write a page whose plaintext exceeds the
    /// configured page payload capacity.
    #[error("ERR_PAGE_TOO_LARGE: payload {actual} > capacity {capacity}")]
    PayloadTooLarge { actual: usize, capacity: usize },
}

/// Which cipher decrypted a page — primary (current epoch) or
/// secondary (rotation source). Used by the rotation runner to
/// decide whether a page still needs to be re-encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// The page was decrypted under the current-epoch key.
    Primary,
    /// The page was decrypted under the previous-epoch key
    /// (set via [`EncryptedPageStream::install_secondary`]).
    /// The rotation runner must re-encrypt this page under the
    /// primary before the next checkpoint.
    Secondary,
}

/// One-page output buffer: header || ciphertext || tag. Helper
/// type to keep the call sites readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageBuffer(pub Vec<u8>);

impl PageBuffer {
    /// Bytes that go to disk.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Maximum plaintext size that fits in one page after the header
/// and AEAD tag. Storage hooks must respect this bound; pages
/// larger than the cap flow through the existing overflow-page
/// mechanism in the property store.
pub const MAX_PAGE_PAYLOAD: usize = PAGE_SIZE - PAGE_HEADER_LEN - TAG_LEN;

/// Page-level encryption stream.
///
/// `EncryptedPageStream` is **stateful**: it owns the generation
/// counter map keyed by `(file_id, page_offset)`. A storage hook
/// constructs one stream per active database key and threads it
/// through every page write; reads do not touch the counter.
///
/// During an online key rotation, the stream may hold an optional
/// **secondary** cipher — the previous epoch's per-database key.
/// Writes always use the primary; reads probe the primary first and
/// fall back to the secondary on `ERR_BAD_KEY`. Once the rotation
/// runner finishes re-encrypting every page under the primary, the
/// caller calls [`EncryptedPageStream::clear_secondary`] to drop
/// the old key out of memory.
pub struct EncryptedPageStream {
    primary: PageCipher,
    secondary: parking_lot::RwLock<Option<PageCipher>>,
    generations: Mutex<HashMap<(FileId, u64), u32>>,
}

impl EncryptedPageStream {
    /// Build a stream from a per-database key. The cipher is
    /// owned for the lifetime of the stream; rotating the key uses
    /// [`EncryptedPageStream::install_secondary`] to keep the old
    /// key available for reads while the runner re-encrypts.
    #[must_use]
    pub fn new(cipher: PageCipher) -> Self {
        Self {
            primary: cipher,
            secondary: parking_lot::RwLock::new(None),
            generations: Mutex::new(HashMap::new()),
        }
    }

    /// Install the **previous** (rotation-source) cipher as the
    /// secondary read-fallback key. Pass the cipher built from the
    /// old per-database key (e.g. `derive_database_key(master, db,
    /// epoch_old)`) before bumping the primary to the new epoch's
    /// key. This call replaces any existing secondary.
    pub fn install_secondary(&self, secondary: PageCipher) {
        *self.secondary.write() = Some(secondary);
    }

    /// Drop the secondary cipher. Called by the rotation runner once
    /// every page has been re-encrypted under the primary; after
    /// this point the old key is gone from memory.
    pub fn clear_secondary(&self) {
        *self.secondary.write() = None;
    }

    /// True when a rotation is in progress (secondary key installed).
    pub fn has_secondary(&self) -> bool {
        self.secondary.read().is_some()
    }

    /// Encrypt one page. Increments the generation counter for the
    /// `(file_id, page_offset)` pair; subsequent calls for the same
    /// page produce a fresh nonce automatically.
    pub fn encrypt(
        &self,
        file_id: FileId,
        page_offset: u64,
        plaintext: &[u8],
    ) -> Result<PageBuffer, PageStreamError> {
        if plaintext.len() > MAX_PAGE_PAYLOAD {
            return Err(PageStreamError::PayloadTooLarge {
                actual: plaintext.len(),
                capacity: MAX_PAGE_PAYLOAD,
            });
        }

        let generation = {
            let mut map = self
                .generations
                .lock()
                .expect("encrypted-page generation map poisoned");
            let entry = map.entry((file_id, page_offset)).or_insert(0);
            *entry = entry.checked_add(1).expect(
                "page generation counter overflowed u32 — \
                 the key must be rotated before 2^32 writes per page",
            );
            *entry
        };

        let header = PageHeader {
            file_id,
            generation,
        };
        let header_bytes = header.to_bytes();
        let nonce = PageNonce::new(file_id.as_u16(), page_offset, generation);

        // The header is bound into the AEAD as AAD so an adversary
        // who swaps the on-disk header is detected at decrypt time.
        // Writes always use the primary key — the secondary exists
        // only to decrypt pages that the rotation runner has not yet
        // touched.
        let ct = encrypt_page(&self.primary, nonce, plaintext, &header_bytes)?;

        // Output layout: header || ciphertext-with-tag.
        let mut out = Vec::with_capacity(PAGE_HEADER_LEN + ct.len());
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(&ct);
        Ok(PageBuffer(out))
    }

    /// Decrypt one page. The generation is read from the on-disk
    /// header; the caller's known `page_offset` plus the encoded
    /// `file_id` reconstruct the AEAD nonce.
    ///
    /// During a rotation, the read path probes the primary cipher
    /// first; on `ERR_BAD_KEY` (which happens on every page that
    /// the runner has not yet re-encrypted), it falls back to the
    /// secondary. If both fail, surfaces the primary's error so
    /// downstream callers see the post-rotation key.
    pub fn decrypt(&self, page_offset: u64, page: &[u8]) -> Result<Vec<u8>, PageStreamError> {
        if page.len() < PAGE_HEADER_LEN + TAG_LEN {
            return Err(PageStreamError::BadHeader);
        }
        let header_bytes: [u8; PAGE_HEADER_LEN] = page[..PAGE_HEADER_LEN]
            .try_into()
            .expect("len already checked");
        let header = PageHeader::from_bytes(&header_bytes).ok_or(PageStreamError::BadHeader)?;
        let ciphertext = &page[PAGE_HEADER_LEN..];
        let nonce = PageNonce::new(header.file_id.as_u16(), page_offset, header.generation);

        match decrypt_page(&self.primary, nonce, ciphertext, &header_bytes) {
            Ok(pt) => Ok(pt),
            Err(primary_err) => {
                // During a rotation the secondary holds the previous
                // epoch's key. Probe it before surfacing the failure.
                let secondary_guard = self.secondary.read();
                if let Some(ref secondary) = *secondary_guard
                    && let Ok(pt) = decrypt_page(secondary, nonce, ciphertext, &header_bytes)
                {
                    return Ok(pt);
                }
                Err(primary_err.into())
            }
        }
    }

    /// Same as [`Self::decrypt`] but reports which cipher actually
    /// validated the page. Used by the rotation runner to detect
    /// pages that still need re-encryption.
    pub fn decrypt_with_source(
        &self,
        page_offset: u64,
        page: &[u8],
    ) -> Result<(Vec<u8>, KeySource), PageStreamError> {
        if page.len() < PAGE_HEADER_LEN + TAG_LEN {
            return Err(PageStreamError::BadHeader);
        }
        let header_bytes: [u8; PAGE_HEADER_LEN] = page[..PAGE_HEADER_LEN]
            .try_into()
            .expect("len already checked");
        let header = PageHeader::from_bytes(&header_bytes).ok_or(PageStreamError::BadHeader)?;
        let ciphertext = &page[PAGE_HEADER_LEN..];
        let nonce = PageNonce::new(header.file_id.as_u16(), page_offset, header.generation);

        match decrypt_page(&self.primary, nonce, ciphertext, &header_bytes) {
            Ok(pt) => Ok((pt, KeySource::Primary)),
            Err(primary_err) => {
                let secondary_guard = self.secondary.read();
                if let Some(ref secondary) = *secondary_guard
                    && let Ok(pt) = decrypt_page(secondary, nonce, ciphertext, &header_bytes)
                {
                    return Ok((pt, KeySource::Secondary));
                }
                Err(primary_err.into())
            }
        }
    }

    /// Snapshot of the generation map. Test-only.
    #[doc(hidden)]
    pub fn snapshot_generations(&self) -> HashMap<(FileId, u64), u32> {
        self.generations
            .lock()
            .expect("encrypted-page generation map poisoned")
            .clone()
    }
}

// Compile-time assertion that the page geometry leaves room for
// the AEAD tag.
const _: [(); 0] = [(); (MAX_PAGE_PAYLOAD < PAGE_SIZE) as usize - 1];
const _: () = assert!(NONCE_LEN == 12);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::crypto::kdf::{MasterKey, derive_database_key};

    fn fresh_stream(seed: u8, db: &str) -> EncryptedPageStream {
        let m = MasterKey::new([seed; 32]);
        let dbk = derive_database_key(&m, db, 0).unwrap();
        EncryptedPageStream::new(PageCipher::new(&dbk))
    }

    #[test]
    fn header_roundtrips_through_disk_bytes() {
        let h = PageHeader {
            file_id: FileId::NodeStore,
            generation: 0xDEAD_BEEF,
        };
        let bytes = h.to_bytes();
        let parsed = PageHeader::from_bytes(&bytes).expect("parse");
        assert_eq!(parsed, h);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = [0u8; PAGE_HEADER_LEN];
        bytes[0..4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert!(PageHeader::from_bytes(&bytes).is_none());
    }

    #[test]
    fn header_rejects_unknown_file_id() {
        let mut bytes = [0u8; PAGE_HEADER_LEN];
        bytes[0..4].copy_from_slice(&HEADER_MAGIC.to_le_bytes());
        bytes[4..6].copy_from_slice(&999u16.to_le_bytes());
        assert!(PageHeader::from_bytes(&bytes).is_none());
    }

    #[test]
    fn round_trip_recovers_plaintext() {
        let stream = fresh_stream(1, "default");
        let pt = b"node-record".repeat(8);
        let page = stream.encrypt(FileId::NodeStore, 0, &pt).expect("enc");
        let back = stream.decrypt(0, page.as_slice()).expect("dec");
        assert_eq!(back, pt);
    }

    #[test]
    fn generation_advances_on_overwrite() {
        let stream = fresh_stream(1, "default");
        let pt = b"hello".to_vec();
        let p1 = stream.encrypt(FileId::NodeStore, 0, &pt).unwrap();
        let p2 = stream.encrypt(FileId::NodeStore, 0, &pt).unwrap();
        // The two pages have different generations so the
        // ciphertexts MUST differ even though the plaintext is
        // identical.
        assert_ne!(p1.as_slice(), p2.as_slice());
        let snap = stream.snapshot_generations();
        assert_eq!(snap[&(FileId::NodeStore, 0)], 2);
    }

    #[test]
    fn payload_too_large_is_rejected_explicitly() {
        let stream = fresh_stream(1, "default");
        let pt = vec![0u8; MAX_PAGE_PAYLOAD + 1];
        let err = stream.encrypt(FileId::NodeStore, 0, &pt).unwrap_err();
        assert!(matches!(err, PageStreamError::PayloadTooLarge { .. }));
    }

    #[test]
    fn header_swap_is_detected_at_decrypt() {
        let stream = fresh_stream(1, "default");
        let pt = b"secret".to_vec();
        let mut page = stream.encrypt(FileId::NodeStore, 0, &pt).unwrap().0;
        // Swap file_id from NodeStore (2) to RelStore (3).
        page[4] = FileId::RelStore.as_u16().to_le_bytes()[0];
        let err = stream.decrypt(0, &page).unwrap_err();
        // Either BadHeader (if magic still validates but we want
        // strict) or AEAD failure (because the AAD changed).
        assert!(matches!(
            err,
            PageStreamError::Aead(super::super::aes_gcm::AeadError::BadKey)
        ));
    }

    #[test]
    fn truncated_page_is_rejected() {
        let stream = fresh_stream(1, "default");
        let err = stream.decrypt(0, &[0u8; 4]).unwrap_err();
        assert!(matches!(err, PageStreamError::BadHeader));
    }

    #[test]
    fn distinct_pages_use_distinct_nonces_and_diverge() {
        let stream = fresh_stream(1, "default");
        let pt = b"identical".to_vec();
        let p_a = stream.encrypt(FileId::NodeStore, 0, &pt).unwrap();
        let p_b = stream
            .encrypt(FileId::NodeStore, PAGE_SIZE as u64, &pt)
            .unwrap();
        // Different page offsets get different nonces — ciphertexts
        // must diverge on the body bytes (header bytes are
        // plaintext but identical here aside from generation, which
        // for a fresh stream starts fresh per page).
        assert_ne!(
            &p_a.as_slice()[PAGE_HEADER_LEN..],
            &p_b.as_slice()[PAGE_HEADER_LEN..]
        );
    }

    #[test]
    fn key_rotation_via_fresh_stream_invalidates_old_pages() {
        let s_v1 = fresh_stream(1, "default");
        let pt = b"data".to_vec();
        let page = s_v1.encrypt(FileId::NodeStore, 0, &pt).unwrap();

        // New stream with a different master key — operator-style
        // rotation. Decrypt must fail loudly (ERR_BAD_KEY) rather
        // than silently return garbage.
        let s_v2 = fresh_stream(2, "default");
        let err = s_v2.decrypt(0, page.as_slice()).unwrap_err();
        assert!(matches!(
            err,
            PageStreamError::Aead(super::super::aes_gcm::AeadError::BadKey)
        ));
    }
}
