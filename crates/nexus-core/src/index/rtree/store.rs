//! Persistent page store for the R-tree
//! (phase6_rtree-index-core §5).
//!
//! Two implementations behind one trait so the R-tree owns one
//! storage surface independent of where pages live:
//!
//! - [`MemoryPageStore`] — `HashMap<page_id, [u8; 8192]>`. Used by
//!   tests and the bulk-build path before persistence is wired.
//!   Cheap, deterministic, no I/O.
//! - [`FilePageStore`] — file-backed. Pages are laid out at
//!   `(page_id - 1) * 8192` so the on-disk image mirrors the
//!   B-tree's `index/btree.rs` file shape: a flat array of fixed
//!   8 KB pages, page id 1 lives at offset 0, page id 2 at
//!   offset 8192, etc. Write-through on `write_page`; explicit
//!   [`PageStore::flush`] forces an fsync so a crash after
//!   `flush()` returns durably persists every committed page.
//!
//! ## Why not `crate::page_cache::PageCache`?
//!
//! The cache's `Page` struct embeds a 4-byte `xxh3` checksum at
//! offsets 0-3 and a 16-byte cache-managed header it relies on for
//! its Clock / TinyLFU bookkeeping. The R-tree's page layout
//! (§1) puts the magic + version at those exact offsets — letting
//! the cache stamp a checksum there would corrupt the page. The
//! eviction-aware backing lands once both layouts converge in a
//! follow-up storage refactor; until then the R-tree owns its file
//! directly. Both paths produce byte-identical on-disk pages so
//! the swap is invisible to readers.
//!
//! ## Crash consistency
//!
//! [`FilePageStore::write_page`] writes the 8 KB buffer in one
//! `pwrite`-equivalent call; the OS guarantees the buffer either
//! lands wholly or not at all on most filesystems for sub-page
//! aligned writes (and at minimum at the 4 KB page boundary).
//! [`PageStore::flush`] calls `sync_all` so all pending writes are
//! durable. The crash-recovery test in `tests/` writes pages,
//! drops the store mid-sync, reopens against the same file, and
//! asserts every page that returned from `write_page + flush`
//! decodes back successfully.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use thiserror::Error;

use super::RTREE_PAGE_SIZE;

/// Page-store error surface. Split from
/// [`crate::Error`] so callers that route storage failures
/// differently from query failures can match without losing
/// detail.
#[derive(Debug, Error)]
pub enum PageStoreError {
    /// Underlying I/O error from the OS or filesystem.
    #[error("page store I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Caller asked for a page id that has never been written.
    #[error("page {0} not present in store")]
    NotFound(u64),
    /// Page id 0 is reserved (page ids are 1-based per the §1
    /// allocator); rejecting it surfaces the bug at the storage
    /// boundary instead of further down the read path.
    #[error("page id 0 is reserved")]
    InvalidPageId,
}

/// Persistence surface every R-tree implementation reads through.
/// All methods take `&mut self` for now — the existing single-
/// writer engine model fits, and a future RwLock-backed wrapper
/// can promote concurrent reads without reshaping callers.
pub trait PageStore: Send {
    /// Persist `data` under `page_id`. Caller is responsible for
    /// ensuring `data.len() == RTREE_PAGE_SIZE`. Overwrites any
    /// existing page at the same id.
    fn write_page(&mut self, page_id: u64, data: &[u8]) -> Result<(), PageStoreError>;

    /// Return the 8 KB image of `page_id` or `Err(NotFound)`.
    fn read_page(&mut self, page_id: u64) -> Result<[u8; RTREE_PAGE_SIZE], PageStoreError>;

    /// Remove `page_id`. Subsequent `read_page` returns
    /// `Err(NotFound)`.
    fn delete_page(&mut self, page_id: u64) -> Result<(), PageStoreError>;

    /// `true` iff `page_id` is currently stored.
    fn contains(&self, page_id: u64) -> bool;

    /// Number of currently stored pages.
    fn len(&self) -> usize;

    /// `true` when the store has no pages.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Force every queued write to durable storage. Memory stores
    /// can implement this as a no-op; file-backed stores `fsync`.
    fn flush(&mut self) -> Result<(), PageStoreError>;
}

/// In-memory implementation. Used by tests and any caller that
/// wants the R-tree without a backing file.
pub struct MemoryPageStore {
    pages: HashMap<u64, [u8; RTREE_PAGE_SIZE]>,
}

impl Default for MemoryPageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryPageStore {
    /// Empty store.
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
        }
    }
}

impl PageStore for MemoryPageStore {
    fn write_page(&mut self, page_id: u64, data: &[u8]) -> Result<(), PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        let mut buf = [0u8; RTREE_PAGE_SIZE];
        let copy_len = data.len().min(RTREE_PAGE_SIZE);
        buf[..copy_len].copy_from_slice(&data[..copy_len]);
        self.pages.insert(page_id, buf);
        Ok(())
    }

    fn read_page(&mut self, page_id: u64) -> Result<[u8; RTREE_PAGE_SIZE], PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        self.pages
            .get(&page_id)
            .copied()
            .ok_or(PageStoreError::NotFound(page_id))
    }

    fn delete_page(&mut self, page_id: u64) -> Result<(), PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        self.pages.remove(&page_id);
        Ok(())
    }

    fn contains(&self, page_id: u64) -> bool {
        page_id != 0 && self.pages.contains_key(&page_id)
    }

    fn len(&self) -> usize {
        self.pages.len()
    }

    fn flush(&mut self) -> Result<(), PageStoreError> {
        Ok(())
    }
}

/// File-backed implementation. Lays pages out at
/// `(page_id - 1) * RTREE_PAGE_SIZE` so the on-disk image mirrors
/// the B-tree's storage shape. Page existence is tracked through
/// a side bitmap so a `delete_page` followed by a `read_page` of
/// the same id reports `NotFound` even if the underlying file
/// still carries the bytes.
pub struct FilePageStore {
    path: PathBuf,
    file: Mutex<File>,
    /// Set of currently live page ids. Persisted alongside the
    /// data file as `<path>.live` — a sorted u64 stream so the
    /// reopen path doesn't need to scan the whole data file to
    /// rediscover liveness.
    live: std::collections::BTreeSet<u64>,
}

impl FilePageStore {
    /// Open or create the data file at `path`. The file grows
    /// on-demand as pages are written; reopening picks up every
    /// page that was in the live set when the previous instance
    /// was dropped (which is set by `flush`).
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PageStoreError> {
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
            live,
        })
    }

    fn live_path(&self) -> PathBuf {
        let mut p = self.path.clone();
        let extension_with_live = match p.extension().and_then(|e| e.to_str()) {
            Some(ext) => format!("{ext}.live"),
            None => "live".to_string(),
        };
        p.set_extension(extension_with_live);
        p
    }

    fn load_live_set(data_path: &Path) -> std::collections::BTreeSet<u64> {
        let mut live = std::collections::BTreeSet::new();
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
        (page_id - 1).saturating_mul(RTREE_PAGE_SIZE as u64)
    }
}

impl PageStore for FilePageStore {
    fn write_page(&mut self, page_id: u64, data: &[u8]) -> Result<(), PageStoreError> {
        if page_id == 0 {
            return Err(PageStoreError::InvalidPageId);
        }
        if data.len() != RTREE_PAGE_SIZE {
            return Err(PageStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "FilePageStore::write_page expects {RTREE_PAGE_SIZE} bytes, got {}",
                    data.len()
                ),
            )));
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| PageStoreError::Io(std::io::Error::other("file mutex poisoned")))?;
        file.seek(SeekFrom::Start(Self::offset_for(page_id)))?;
        file.write_all(data)?;
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
        let mut buf = [0u8; RTREE_PAGE_SIZE];
        let mut file = self
            .file
            .lock()
            .map_err(|_| PageStoreError::Io(std::io::Error::other("file mutex poisoned")))?;
        file.seek(SeekFrom::Start(Self::offset_for(page_id)))?;
        file.read_exact(&mut buf)?;
        Ok(buf)
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
        {
            let file = self
                .file
                .lock()
                .map_err(|_| PageStoreError::Io(std::io::Error::other("file mutex poisoned")))?;
            file.sync_all()?;
        }
        self.write_live_set()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::page::{ChildRef, RTreePageHeader, decode_page, encode_page};
    use super::*;
    use tempfile::TempDir;

    fn sample_page(page_id: u64, leaf_id: u64) -> [u8; RTREE_PAGE_SIZE] {
        let header = RTreePageHeader::new(0, 1, u128::from(page_id));
        encode_page(&header, &[ChildRef::leaf_point_2d(leaf_id, 1.0, 2.0)])
    }

    // -- MemoryPageStore --------------------------------------------

    #[test]
    fn memory_store_round_trips_pages() {
        let mut store = MemoryPageStore::new();
        let buf = sample_page(1, 99);
        store.write_page(1, &buf).unwrap();
        let read = store.read_page(1).unwrap();
        assert_eq!(read, buf);
    }

    #[test]
    fn memory_store_rejects_page_id_zero() {
        let mut store = MemoryPageStore::new();
        let buf = sample_page(1, 1);
        assert!(matches!(
            store.write_page(0, &buf),
            Err(PageStoreError::InvalidPageId)
        ));
    }

    #[test]
    fn memory_store_returns_not_found_for_unknown_id() {
        let mut store = MemoryPageStore::new();
        assert!(matches!(
            store.read_page(42),
            Err(PageStoreError::NotFound(42))
        ));
    }

    #[test]
    fn memory_store_delete_removes_from_subsequent_reads() {
        let mut store = MemoryPageStore::new();
        store.write_page(7, &sample_page(7, 1)).unwrap();
        assert!(store.contains(7));
        store.delete_page(7).unwrap();
        assert!(!store.contains(7));
        assert!(matches!(
            store.read_page(7),
            Err(PageStoreError::NotFound(7))
        ));
    }

    // -- FilePageStore ----------------------------------------------

    #[test]
    fn file_store_round_trips_pages() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rtree.dat");
        let mut store = FilePageStore::open(&path).unwrap();
        let buf = sample_page(3, 314);
        store.write_page(3, &buf).unwrap();
        let read = store.read_page(3).unwrap();
        assert_eq!(read, buf);

        // The decoded page should match what we put in.
        let (header, entries) = decode_page(&read).unwrap();
        assert_eq!(header.page_id, 3);
        assert_eq!(entries[0].child_ptr, 314);
    }

    #[test]
    fn file_store_rejects_wrong_buffer_length() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rtree.dat");
        let mut store = FilePageStore::open(&path).unwrap();
        let short = vec![0u8; RTREE_PAGE_SIZE - 1];
        assert!(matches!(
            store.write_page(1, &short),
            Err(PageStoreError::Io(_))
        ));
    }

    #[test]
    fn file_store_persists_across_reopen() {
        // Crash-consistency test (§5.3): write three pages,
        // flush to fsync the data + live set, drop the store
        // (mimics a clean shutdown), reopen against the same
        // file, and assert every previously committed page is
        // still readable and decodes correctly.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rtree.dat");

        {
            let mut store = FilePageStore::open(&path).unwrap();
            store.write_page(1, &sample_page(1, 11)).unwrap();
            store.write_page(2, &sample_page(2, 22)).unwrap();
            store.write_page(3, &sample_page(3, 33)).unwrap();
            store.flush().unwrap();
            // store dropped here
        }

        let mut reopened = FilePageStore::open(&path).unwrap();
        assert_eq!(reopened.len(), 3);
        for (page_id, leaf_id) in [(1u64, 11u64), (2, 22), (3, 33)] {
            let buf = reopened.read_page(page_id).unwrap();
            let (header, entries) = decode_page(&buf).unwrap();
            assert_eq!(header.page_id, u128::from(page_id));
            assert_eq!(entries[0].child_ptr, leaf_id);
        }
    }

    #[test]
    fn file_store_pages_written_without_flush_lose_liveness_after_drop() {
        // Without flush(), the live set isn't persisted. The data
        // bytes may still be on disk (the OS page cache could have
        // written them), but reopening sees an empty live set so
        // reads return NotFound — the caller's contract was never
        // "durable until flush" so the store correctly forgets
        // them.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rtree.dat");

        {
            let mut store = FilePageStore::open(&path).unwrap();
            store.write_page(7, &sample_page(7, 99)).unwrap();
            // Intentionally skip flush().
        }

        let mut reopened = FilePageStore::open(&path).unwrap();
        assert_eq!(reopened.len(), 0);
        assert!(matches!(
            reopened.read_page(7),
            Err(PageStoreError::NotFound(7))
        ));
    }

    #[test]
    fn file_store_delete_persists_through_reopen() {
        // Write three pages + flush, then delete one + flush
        // again. Reopening should see only the two survivors.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rtree.dat");
        {
            let mut store = FilePageStore::open(&path).unwrap();
            for id in [1u64, 2, 3] {
                store.write_page(id, &sample_page(id, id * 10)).unwrap();
            }
            store.flush().unwrap();

            store.delete_page(2).unwrap();
            store.flush().unwrap();
        }

        let reopened = FilePageStore::open(&path).unwrap();
        assert_eq!(reopened.len(), 2);
        assert!(reopened.contains(1));
        assert!(!reopened.contains(2));
        assert!(reopened.contains(3));
    }
}
