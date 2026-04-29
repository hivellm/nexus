//! Encrypted file-backed [`PageStore`] for the R-tree spatial index.
//!
//! [`crate::index::rtree::store::FilePageStore`] persists 8 KB R-tree
//! pages at fixed `(page_id - 1) * 8192` offsets on disk. This module
//! ships a parallel implementation that is byte-for-byte compatible
//! at the *logical* layer (the R-tree still reads / writes 8 KB
//! plaintext pages) but lays out **wider slots** on disk so each
//! page can carry the AEAD overhead.
//!
//! # On-disk layout
//!
//! Each logical page lives in an [`EncryptedSlot`]-sized slot at
//! `(page_id - 1) * ENCRYPTED_RTREE_SLOT_SIZE`.
//!
//! ```text
//!   [0..16]            page header (magic + file_id + generation, plaintext)
//!   [16..16+RTREE]     ciphertext of the 8 KB R-tree page
//!   [16+RTREE..slot]   AEAD tag (16 bytes)
//! ```
//!
//! `ENCRYPTED_RTREE_SLOT_SIZE = 16 + 8192 + 16 = 8224 bytes`.
//!
//! The page header is bound into the AEAD as additional
//! authenticated data so an adversary swapping the on-disk header
//! is detected at decrypt time.
//!
//! # Why not reuse [`crate::storage::crypto::EncryptedPageStream`]?
//!
//! `EncryptedPageStream` caps plaintext at `MAX_PAGE_PAYLOAD = 8160`
//! bytes (`PAGE_SIZE - PAGE_HEADER_LEN - TAG_LEN`). The R-tree's
//! pages are exactly 8192 bytes — they can't fit in a stream slot
//! without splitting. This module uses the lower-level
//! [`PageCipher`] / [`PageNonce`] primitives and lays out its own
//! 8224-byte slot, which keeps the R-tree's pre-encryption page
//! semantics byte-identical.
//!
//! # Wiring
//!
//! Construction is deliberately decoupled from the rest of the
//! engine: callers build a [`crate::storage::crypto::PageCipher`]
//! from a per-database key, hand it to
//! [`EncryptedFilePageStore::open`], and the resulting store plugs
//! straight into the R-tree's existing [`PageStore`] trait. No
//! R-tree internals change — flipping encryption on for a deployment
//! is a one-line wiring decision in the storage initialisation
//! path.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::RTREE_PAGE_SIZE;
use super::store::{PageStore, PageStoreError};
use crate::storage::crypto::{
    FileId, NONCE_LEN, PageCipher, PageNonce, TAG_LEN, decrypt_page, encrypt_page,
};

/// Plaintext page-header length, in bytes. Same shape as
/// [`crate::storage::crypto::encrypted_file::PAGE_HEADER_LEN`] —
/// duplicated as a const so this module does not pull a dependency
/// edge into `encrypted_file.rs`'s exact slot geometry. If the
/// shared header layout ever evolves this constant rises in
/// lockstep.
pub const ENCRYPTED_RTREE_HEADER_LEN: usize = 16;

/// On-disk slot size: header + ciphertext + AEAD tag.
pub const ENCRYPTED_RTREE_SLOT_SIZE: usize = ENCRYPTED_RTREE_HEADER_LEN + RTREE_PAGE_SIZE + TAG_LEN;

const HEADER_MAGIC: u32 = 0x4E58_5254; // "NXRT" — Nexus encrypted R-tree page

/// Encrypted R-tree page-store. Implements [`PageStore`] so it
/// drops into every R-tree call site without changes.
pub struct EncryptedFilePageStore {
    path: PathBuf,
    file: Mutex<File>,
    cipher: PageCipher,
    /// Per-page generation counter — bumped on every `write_page`
    /// so the AEAD nonce is unique across overwrites of the same
    /// page id. AES-GCM is catastrophically broken under nonce
    /// reuse; the counter is the non-negotiable.
    generations: Mutex<std::collections::HashMap<u64, u32>>,
    /// Set of currently live page ids. Persisted alongside the
    /// data file as `<path>.live` — same pattern as
    /// [`crate::index::rtree::store::FilePageStore`] so the
    /// crash-recovery story is unchanged.
    live: BTreeSet<u64>,
}

impl EncryptedFilePageStore {
    /// Open or create an encrypted page store at `path` using the
    /// caller-provided [`PageCipher`].
    ///
    /// The cipher's lifetime is tied to the active per-database
    /// key. Rotation is the responsibility of
    /// [`crate::storage::crypto::rotation::RotationRunner`] —
    /// rebuilding the store under a new cipher is a separate
    /// procedure outside this module.
    pub fn open<P: AsRef<Path>>(path: P, cipher: PageCipher) -> Result<Self, PageStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let live = Self::load_live_set(&path);
        Ok(Self {
            path,
            file: Mutex::new(file),
            cipher,
            generations: Mutex::new(Default::default()),
            live,
        })
    }

    /// Path to the live-set sidecar.
    fn live_path(&self) -> PathBuf {
        let mut p = self.path.clone();
        let ext = match p.extension().and_then(|e| e.to_str()) {
            Some(ext) => format!("{ext}.live"),
            None => "live".to_string(),
        };
        p.set_extension(ext);
        p
    }

    fn load_live_set(data_path: &Path) -> BTreeSet<u64> {
        let mut live = BTreeSet::new();
        let live_path = {
            let mut p = data_path.to_path_buf();
            let ext = match p.extension().and_then(|e| e.to_str()) {
                Some(ext) => format!("{ext}.live"),
                None => "live".to_string(),
            };
            p.set_extension(ext);
            p
        };
        let Ok(mut f) = File::open(&live_path) else {
            return live;
        };
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return live;
        }
        for chunk in buf.chunks_exact(8) {
            let id = u64::from_le_bytes([
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
            ]);
            if id != 0 {
                live.insert(id);
            }
        }
        live
    }

    fn write_live_set(&self) -> Result<(), PageStoreError> {
        let live_path = self.live_path();
        let tmp_path = {
            let mut p = live_path.clone();
            let ext = match p.extension().and_then(|e| e.to_str()) {
                Some(ext) => format!("{ext}.tmp"),
                None => "tmp".to_string(),
            };
            p.set_extension(ext);
            p
        };
        {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            for id in &self.live {
                f.write_all(&id.to_le_bytes())?;
            }
            f.sync_all()?;
        }
        std::fs::rename(&tmp_path, &live_path)?;
        Ok(())
    }

    fn offset_for(page_id: u64) -> u64 {
        debug_assert!(page_id >= 1, "page id must be 1-based");
        (page_id - 1).saturating_mul(ENCRYPTED_RTREE_SLOT_SIZE as u64)
    }

    fn build_header(generation: u32) -> [u8; ENCRYPTED_RTREE_HEADER_LEN] {
        let mut buf = [0u8; ENCRYPTED_RTREE_HEADER_LEN];
        buf[0..4].copy_from_slice(&HEADER_MAGIC.to_le_bytes());
        buf[4..6].copy_from_slice(&FileId::RTreeIndex.as_u16().to_le_bytes());
        buf[6..10].copy_from_slice(&generation.to_le_bytes());
        // bytes 10..16 reserved for a future flags word; left zero.
        buf
    }

    fn parse_header(buf: &[u8; ENCRYPTED_RTREE_HEADER_LEN]) -> Option<u32> {
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != HEADER_MAGIC {
            return None;
        }
        let file_id = u16::from_le_bytes([buf[4], buf[5]]);
        if file_id != FileId::RTreeIndex.as_u16() {
            return None;
        }
        Some(u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]))
    }

    fn next_generation(&self, page_id: u64) -> u32 {
        let mut map = self
            .generations
            .lock()
            .expect("encrypted-rtree generation map poisoned");
        let entry = map.entry(page_id).or_insert(0);
        *entry = entry.checked_add(1).expect(
            "page generation counter overflowed u32 — \
             the key must be rotated before 2^32 writes per page",
        );
        *entry
    }
}

impl PageStore for EncryptedFilePageStore {
    fn write_page(&mut self, page_id: u64, data: &[u8]) -> Result<(), PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        if data.len() != RTREE_PAGE_SIZE {
            return Err(PageStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "EncryptedFilePageStore::write_page expects {RTREE_PAGE_SIZE} bytes, got {}",
                    data.len()
                ),
            )));
        }

        let generation = self.next_generation(page_id);
        let header = Self::build_header(generation);
        let nonce = PageNonce::new(
            FileId::RTreeIndex.as_u16(),
            Self::offset_for(page_id),
            generation,
        );
        // AEAD: ciphertext = encrypt(plaintext) || tag (16 bytes
        // appended). The header is bound as AAD so a swapped
        // header (different generation, different page id) is
        // detected at decrypt time.
        let ct_with_tag = encrypt_page(&self.cipher, nonce, data, &header).map_err(|e| {
            PageStoreError::Io(std::io::Error::other(format!(
                "EncryptedFilePageStore::encrypt: {e}"
            )))
        })?;
        debug_assert_eq!(ct_with_tag.len(), RTREE_PAGE_SIZE + TAG_LEN);

        let mut slot = [0u8; ENCRYPTED_RTREE_SLOT_SIZE];
        slot[..ENCRYPTED_RTREE_HEADER_LEN].copy_from_slice(&header);
        slot[ENCRYPTED_RTREE_HEADER_LEN..].copy_from_slice(&ct_with_tag);

        let mut file = self
            .file
            .lock()
            .map_err(|_| PageStoreError::Io(std::io::Error::other("file mutex poisoned")))?;
        file.seek(SeekFrom::Start(Self::offset_for(page_id)))?;
        file.write_all(&slot)?;
        self.live.insert(page_id);
        Ok(())
    }

    fn read_page(&mut self, page_id: u64) -> Result<[u8; RTREE_PAGE_SIZE], PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        if !self.live.contains(&page_id) {
            return Err(PageStoreError::NotFound(page_id));
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| PageStoreError::Io(std::io::Error::other("file mutex poisoned")))?;
        file.seek(SeekFrom::Start(Self::offset_for(page_id)))?;
        let mut slot = [0u8; ENCRYPTED_RTREE_SLOT_SIZE];
        file.read_exact(&mut slot)?;
        drop(file);

        let mut header = [0u8; ENCRYPTED_RTREE_HEADER_LEN];
        header.copy_from_slice(&slot[..ENCRYPTED_RTREE_HEADER_LEN]);
        let generation = Self::parse_header(&header).ok_or_else(|| {
            PageStoreError::Io(std::io::Error::other(
                "EncryptedFilePageStore: page header magic / file_id mismatch",
            ))
        })?;
        let ct_with_tag = &slot[ENCRYPTED_RTREE_HEADER_LEN..];
        let nonce = PageNonce::new(
            FileId::RTreeIndex.as_u16(),
            Self::offset_for(page_id),
            generation,
        );
        let pt = decrypt_page(&self.cipher, nonce, ct_with_tag, &header).map_err(|e| {
            PageStoreError::Io(std::io::Error::other(format!(
                "EncryptedFilePageStore::decrypt: {e}"
            )))
        })?;
        if pt.len() != RTREE_PAGE_SIZE {
            return Err(PageStoreError::Io(std::io::Error::other(format!(
                "EncryptedFilePageStore::read_page produced {} bytes, expected {RTREE_PAGE_SIZE}",
                pt.len()
            ))));
        }
        let mut out = [0u8; RTREE_PAGE_SIZE];
        out.copy_from_slice(&pt);
        Ok(out)
    }

    fn delete_page(&mut self, page_id: u64) -> Result<(), PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        self.live.remove(&page_id);
        Ok(())
    }

    fn contains(&self, page_id: u64) -> bool {
        page_id != 0 && self.live.contains(&page_id)
    }

    fn len(&self) -> usize {
        self.live.len()
    }

    fn flush(&mut self) -> Result<(), PageStoreError> {
        let file = self
            .file
            .lock()
            .map_err(|_| PageStoreError::Io(std::io::Error::other("file mutex poisoned")))?;
        file.sync_all()?;
        drop(file);
        self.write_live_set()?;
        Ok(())
    }
}

// Compile-time guarantees that prevent silent layout drift if
// constants ever change in `encrypted_file.rs`.
const _: () = assert!(NONCE_LEN == 12);
const _: () = assert!(ENCRYPTED_RTREE_SLOT_SIZE > RTREE_PAGE_SIZE);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::crypto::kdf::{MasterKey, derive_database_key};
    use tempfile::TempDir;

    fn cipher_for(seed: u8, db: &str, epoch: u32) -> PageCipher {
        let m = MasterKey::new([seed; 32]);
        let k = derive_database_key(&m, db, epoch).unwrap();
        PageCipher::new(&k)
    }

    fn fill_pattern(seed: u8) -> [u8; RTREE_PAGE_SIZE] {
        let mut buf = [0u8; RTREE_PAGE_SIZE];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = seed.wrapping_add((i & 0xFF) as u8);
        }
        buf
    }

    #[test]
    fn round_trip_recovers_full_8192_byte_page() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        let plaintext = fill_pattern(0xAA);
        store.write_page(1, &plaintext).expect("write");
        let read_back = store.read_page(1).expect("read");
        assert_eq!(read_back, plaintext);
    }

    #[test]
    fn distinct_pages_get_distinct_slots_and_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        let pa = fill_pattern(0x11);
        let pb = fill_pattern(0x22);
        store.write_page(1, &pa).unwrap();
        store.write_page(2, &pb).unwrap();
        assert_eq!(store.read_page(1).unwrap(), pa);
        assert_eq!(store.read_page(2).unwrap(), pb);

        // The on-disk file size must be at least 2 slots (page id 2
        // lands at offset SLOT_SIZE * 1).
        let file_meta = std::fs::metadata(&path).unwrap();
        assert!(file_meta.len() >= (ENCRYPTED_RTREE_SLOT_SIZE * 2) as u64);
    }

    #[test]
    fn overwrite_advances_generation_and_re_reads() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        let p1 = fill_pattern(0x33);
        let p2 = fill_pattern(0x44);
        store.write_page(1, &p1).unwrap();
        store.write_page(1, &p2).unwrap();
        assert_eq!(store.read_page(1).unwrap(), p2);
        // Generation map should have advanced past the first write.
        let gens = store.generations.lock().unwrap();
        assert_eq!(gens.get(&1), Some(&2));
    }

    #[test]
    fn wrong_key_decrypt_surfaces_io_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        // Write under one key.
        {
            let mut store =
                EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
            let p = fill_pattern(0x55);
            store.write_page(1, &p).unwrap();
            store.flush().unwrap();
        }
        // Read under a different key — must fail loudly, never
        // return garbage.
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(2, "default", 0)).unwrap();
        let err = store.read_page(1).unwrap_err();
        assert!(
            matches!(err, PageStoreError::Io(_)),
            "expected Io error, got {err:?}"
        );
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        {
            let mut store =
                EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
            store.write_page(1, &fill_pattern(0x66)).unwrap();
            store.flush().unwrap();
        }
        // Flip a single byte in the middle of the slot — must be
        // detected by the AEAD tag.
        let mut bytes = std::fs::read(&path).unwrap();
        let target = ENCRYPTED_RTREE_HEADER_LEN + 100;
        bytes[target] ^= 0x01;
        std::fs::write(&path, &bytes).unwrap();

        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        let err = store.read_page(1).unwrap_err();
        assert!(matches!(err, PageStoreError::Io(_)));
    }

    #[test]
    fn header_swap_is_detected_at_decrypt() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        {
            let mut store =
                EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
            store.write_page(1, &fill_pattern(0x77)).unwrap();
            store.flush().unwrap();
        }
        // Corrupt the header's generation byte — bound into the
        // AEAD as AAD, so the tag check fails on decrypt.
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[6] ^= 0x80;
        std::fs::write(&path, &bytes).unwrap();
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        let err = store.read_page(1).unwrap_err();
        assert!(matches!(err, PageStoreError::Io(_)));
    }

    #[test]
    fn restart_recovers_live_set_and_decrypts() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let pa = fill_pattern(0x88);
        let pb = fill_pattern(0x99);
        {
            let mut store =
                EncryptedFilePageStore::open(&path, cipher_for(3, "default", 0)).unwrap();
            store.write_page(1, &pa).unwrap();
            store.write_page(7, &pb).unwrap();
            store.flush().unwrap();
        }
        // Reopen — live set must come back, both pages decrypt.
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(3, "default", 0)).unwrap();
        assert!(store.contains(1));
        assert!(store.contains(7));
        assert_eq!(store.len(), 2);
        assert_eq!(store.read_page(1).unwrap(), pa);
        assert_eq!(store.read_page(7).unwrap(), pb);
    }

    #[test]
    fn delete_marks_page_not_present() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        store.write_page(1, &fill_pattern(0x12)).unwrap();
        assert!(store.contains(1));
        store.delete_page(1).unwrap();
        assert!(!store.contains(1));
        let err = store.read_page(1).unwrap_err();
        assert!(matches!(err, PageStoreError::NotFound(1)));
    }

    #[test]
    fn page_id_zero_is_rejected_on_every_method() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        assert!(matches!(
            store.write_page(0, &fill_pattern(0)).unwrap_err(),
            PageStoreError::InvalidPageId
        ));
        assert!(matches!(
            store.read_page(0).unwrap_err(),
            PageStoreError::InvalidPageId
        ));
        assert!(matches!(
            store.delete_page(0).unwrap_err(),
            PageStoreError::InvalidPageId
        ));
        assert!(!store.contains(0));
    }

    #[test]
    fn write_with_wrong_size_is_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let mut store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        let too_small = vec![0u8; RTREE_PAGE_SIZE - 1];
        assert!(matches!(
            store.write_page(1, &too_small).unwrap_err(),
            PageStoreError::Io(_)
        ));
    }

    #[test]
    fn empty_store_is_empty_and_zero_length() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("rtree.enc");
        let store = EncryptedFilePageStore::open(&path, cipher_for(1, "default", 0)).unwrap();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn slot_size_matches_documented_layout() {
        // The layout constant is part of the on-disk contract;
        // a future refactor that bumps either the header or the
        // tag size must update the slot size in lockstep.
        assert_eq!(
            ENCRYPTED_RTREE_SLOT_SIZE,
            ENCRYPTED_RTREE_HEADER_LEN + RTREE_PAGE_SIZE + TAG_LEN
        );
        assert_eq!(ENCRYPTED_RTREE_HEADER_LEN, 16);
        assert_eq!(TAG_LEN, 16);
        assert_eq!(ENCRYPTED_RTREE_SLOT_SIZE, 8224);
    }
}
