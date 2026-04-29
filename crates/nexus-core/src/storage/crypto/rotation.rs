//! Online key rotation for encrypted pages.
//!
//! NIST SP 800-57 recommends rotating data-encryption keys at most
//! annually. The KDF in [`super::kdf`] already supports per-database
//! epochs; this module adds the **online** runner that re-encrypts
//! every page under the new epoch's key while the server keeps
//! serving traffic.
//!
//! # Two-key window
//!
//! 1. The operator derives the new per-database key
//!    (`derive_database_key(master, db, new_epoch)`) and the
//!    current-epoch key.
//! 2. [`EncryptedPageStream::install_secondary`] installs the
//!    *current* key as the secondary read-fallback.
//! 3. The stream's primary cipher is rebuilt from the new-epoch
//!    key (caller swaps the [`EncryptedPageStream`] inside its
//!    storage handle).
//! 4. [`RotationRunner::run`] walks every `(file_id, page_offset)`
//!    in the [`PageStore`], decrypts under whichever key works
//!    (the secondary on still-old pages, the primary on already-
//!    rotated pages), and re-encrypts under the primary if the
//!    page came from the secondary.
//! 5. Once the runner completes, the operator calls
//!    [`EncryptedPageStream::clear_secondary`] to drop the old
//!    key out of memory.
//!
//! Read traffic during the window pays one extra failed AEAD probe
//! per page that has not yet been rotated; the cost is bounded by
//! the runner's progress.
//!
//! # Checkpointing
//!
//! The runner reports progress through the
//! [`RotationCheckpoint`] type and accepts a resume cursor. Crash
//! recovery is the operator's responsibility: after a server
//! restart, the operator persists the last-seen checkpoint, then
//! calls `RotationRunner::resume_from(checkpoint)` to pick up
//! where the previous run left off.
//!
//! # Throttling
//!
//! [`RotationRunnerConfig::byte_budget_per_second`] caps the
//! re-encryption rate so the runner never starves the live read /
//! write path. The budget is enforced via a simple sleep loop —
//! tighter (token-bucket) shaping is a follow-up if it ever
//! becomes the bottleneck.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::aes_gcm::PageCipher;
use super::encrypted_file::{
    EncryptedPageStream, FileId, KeySource, PAGE_HEADER_LEN, PageBuffer, PageStreamError,
};

/// Identity of a single page in the rotation walk. The runner sweeps
/// pages in `(file_id, page_offset)` ascending order so the
/// checkpoint is a single tuple rather than per-file cursors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageRef {
    pub file_id: FileId,
    pub page_offset: u64,
}

/// Errors the page store / rotation runner can surface.
#[derive(Debug, Error)]
pub enum RotationError {
    /// AEAD failure on both primary and secondary — the page is
    /// either tampered with or encrypted under a third key not
    /// installed in the stream.
    #[error(transparent)]
    PageStream(#[from] PageStreamError),
    /// Page-store IO failure.
    #[error("ERR_PAGE_STORE: {0}")]
    Store(String),
    /// Caller asked the runner to start before [`EncryptedPageStream::install_secondary`].
    #[error("ERR_NO_SECONDARY: rotation requires a secondary key installed")]
    NoSecondary,
}

/// Where the runner is in the sweep. A `None` cursor means "walk
/// from the beginning"; a `Some(page)` cursor means "skip every
/// page < `page`, then resume". Identical pages are re-rotated
/// idempotently so a checkpoint that includes the last-rotated
/// page is safe.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationCheckpoint {
    /// Last successfully rotated page. `None` at the start of a
    /// fresh run.
    pub last_rotated: Option<PageRef>,
    /// Rolling counters reported back to the operator.
    pub stats: RotationStats,
}

/// Cumulative counters surfaced through the checkpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationStats {
    /// Pages the runner observed (including no-ops on already-
    /// primary pages).
    pub pages_total: u64,
    /// Pages actually re-encrypted (decrypted under secondary,
    /// encrypted under primary).
    pub pages_rotated: u64,
    /// Pages skipped because they were already primary.
    pub pages_already_primary: u64,
    /// Bytes re-encrypted (post-encryption page size, including
    /// header + AEAD tag).
    pub bytes_rotated: u64,
}

/// Knobs for [`RotationRunner::run`].
#[derive(Debug, Clone)]
pub struct RotationRunnerConfig {
    /// Maximum bytes / second the runner will re-encrypt. `0`
    /// means unbounded. Default 64 MiB/s — safe for production
    /// SSDs without starving read traffic.
    pub byte_budget_per_second: u64,
    /// Frequency at which the runner reports a fresh checkpoint
    /// to the caller. Default every 1024 pages.
    pub checkpoint_every: u64,
}

impl Default for RotationRunnerConfig {
    fn default() -> Self {
        Self {
            byte_budget_per_second: 64 * 1024 * 1024,
            checkpoint_every: 1024,
        }
    }
}

/// Storage-layer hook the runner walks. Production wires this to
/// the LMDB catalog + record stores + WAL + indexes (one
/// implementation per file family) under the storage-hooks
/// follow-up; tests use [`InMemoryPageStore`].
pub trait PageStore: Send + Sync {
    /// Iterate every page in `(file_id, page_offset)` ascending
    /// order. Production implementations stream from disk; the
    /// in-memory version returns a `Vec`.
    fn list_pages(&self) -> Vec<PageRef>;

    /// Read the on-disk bytes of `page` (header || ciphertext ||
    /// tag).
    fn read_page(&self, page: PageRef) -> Result<Vec<u8>, RotationError>;

    /// Atomically replace the on-disk bytes of `page` with `bytes`.
    /// Storage hooks layer this on top of fsync / WAL semantics
    /// per their own invariants; the runner does not assume a
    /// particular persistence model.
    fn write_page(&self, page: PageRef, bytes: &[u8]) -> Result<(), RotationError>;
}

// ---------------------------------------------------------------------------
// In-memory page store (tests + the storage-hooks scaffolding seam)
// ---------------------------------------------------------------------------

/// In-memory [`PageStore`] for unit tests and the rotation
/// scaffolding. Backed by a [`BTreeMap`] so iteration order is
/// stable — matches the runner's required `(file_id, page_offset)`
/// ascending sweep.
#[derive(Debug, Default)]
pub struct InMemoryPageStore {
    pages: Mutex<BTreeMap<PageRef, Vec<u8>>>,
}

impl InMemoryPageStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or overwrite a page. Used by the rotation tests to
    /// populate the store before kicking the runner.
    pub fn insert(&self, page: PageRef, bytes: Vec<u8>) {
        self.pages.lock().insert(page, bytes);
    }

    /// Number of pages currently stored.
    pub fn len(&self) -> usize {
        self.pages.lock().len()
    }

    /// Snapshot of the pages map. Test-only.
    pub fn snapshot(&self) -> BTreeMap<PageRef, Vec<u8>> {
        self.pages.lock().clone()
    }
}

impl PageStore for InMemoryPageStore {
    fn list_pages(&self) -> Vec<PageRef> {
        self.pages.lock().keys().copied().collect()
    }

    fn read_page(&self, page: PageRef) -> Result<Vec<u8>, RotationError> {
        self.pages
            .lock()
            .get(&page)
            .cloned()
            .ok_or_else(|| RotationError::Store(format!("page not found: {page:?}")))
    }

    fn write_page(&self, page: PageRef, bytes: &[u8]) -> Result<(), RotationError> {
        self.pages.lock().insert(page, bytes.to_vec());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RotationRunner
// ---------------------------------------------------------------------------

/// Drives one rotation sweep over a [`PageStore`].
///
/// Stateless: the operator threads a fresh runner through every
/// rotation. Per-run state lives in the [`RotationCheckpoint`].
pub struct RotationRunner<'a, S: PageStore> {
    pub stream: &'a EncryptedPageStream,
    pub store: &'a S,
    pub config: RotationRunnerConfig,
    /// Optional cancellation flag — the operator flips this to
    /// `true` when the rotation should pause cleanly. The runner
    /// returns the checkpoint reached so far so the next
    /// invocation resumes.
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl<'a, S: PageStore> RotationRunner<'a, S> {
    /// Build a fresh runner with default config and an unset
    /// cancel flag.
    pub fn new(stream: &'a EncryptedPageStream, store: &'a S) -> Self {
        Self {
            stream,
            store,
            config: RotationRunnerConfig::default(),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Override the config.
    #[must_use]
    pub fn with_config(mut self, config: RotationRunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Execute the sweep. Returns the final checkpoint (with rolling
    /// stats) once every page is primary-encrypted, or the
    /// checkpoint reached when the cancel flag fired.
    ///
    /// Idempotent: re-running with the previous checkpoint resumes
    /// from the next page.
    pub fn run(
        &self,
        resume_from: RotationCheckpoint,
    ) -> Result<RotationCheckpoint, RotationError> {
        if !self.stream.has_secondary() {
            return Err(RotationError::NoSecondary);
        }

        let mut checkpoint = resume_from;
        let mut pages = self.store.list_pages();
        pages.sort();

        // Skip everything <= `last_rotated` — the runner already
        // touched those.
        let start_idx = match checkpoint.last_rotated {
            Some(cursor) => pages
                .iter()
                .position(|p| p > &cursor)
                .unwrap_or(pages.len()),
            None => 0,
        };

        let throttler = Throttler::new(self.config.byte_budget_per_second);

        for page_ref in pages.iter().skip(start_idx) {
            if self.cancel.load(Ordering::Relaxed) {
                tracing::info!(?checkpoint, "rotation cancelled");
                return Ok(checkpoint);
            }
            let bytes = self.store.read_page(*page_ref)?;
            let (plaintext, source) = self
                .stream
                .decrypt_with_source(page_ref.page_offset, &bytes)?;

            checkpoint.stats.pages_total = checkpoint.stats.pages_total.saturating_add(1);

            match source {
                KeySource::Primary => {
                    // Already current — nothing to do.
                    checkpoint.stats.pages_already_primary =
                        checkpoint.stats.pages_already_primary.saturating_add(1);
                }
                KeySource::Secondary => {
                    let new_page =
                        self.stream
                            .encrypt(page_ref.file_id, page_ref.page_offset, &plaintext)?;
                    self.store.write_page(*page_ref, new_page.as_slice())?;
                    checkpoint.stats.pages_rotated =
                        checkpoint.stats.pages_rotated.saturating_add(1);
                    checkpoint.stats.bytes_rotated = checkpoint
                        .stats
                        .bytes_rotated
                        .saturating_add(new_page.as_slice().len() as u64);
                    throttler.consume(new_page.as_slice().len() as u64);
                }
            }

            checkpoint.last_rotated = Some(*page_ref);

            // Mid-sweep checkpoint emission is the operator's
            // responsibility — they read `stats` from the
            // returned struct after the run completes (or after a
            // graceful cancel). Production wiring publishes a
            // Prometheus counter every `checkpoint_every` pages
            // via the tracing log line below.
            if checkpoint.stats.pages_total % self.config.checkpoint_every == 0 {
                tracing::info!(
                    pages_total = checkpoint.stats.pages_total,
                    pages_rotated = checkpoint.stats.pages_rotated,
                    bytes_rotated = checkpoint.stats.bytes_rotated,
                    "rotation progress"
                );
            }
        }

        Ok(checkpoint)
    }
}

/// Simple bytes-per-second throttler. Sleeps when the budget
/// for the current second is exhausted; resets at the next
/// 1-second boundary.
#[derive(Debug)]
struct Throttler {
    budget: u64,
    used: AtomicU64,
    window_start: Mutex<Instant>,
}

impl Throttler {
    fn new(budget: u64) -> Self {
        Self {
            budget,
            used: AtomicU64::new(0),
            window_start: Mutex::new(Instant::now()),
        }
    }

    fn consume(&self, bytes: u64) {
        if self.budget == 0 {
            return;
        }
        let used_now = self.used.fetch_add(bytes, Ordering::Relaxed) + bytes;
        if used_now < self.budget {
            return;
        }
        // Compute remaining time in this 1-second window. If we
        // overshot the window already, reset; otherwise sleep the
        // remainder.
        let mut window = self.window_start.lock();
        let elapsed = window.elapsed();
        if elapsed >= Duration::from_secs(1) {
            *window = Instant::now();
            self.used.store(0, Ordering::Relaxed);
        } else {
            let remaining = Duration::from_secs(1) - elapsed;
            drop(window);
            std::thread::sleep(remaining);
            let mut window = self.window_start.lock();
            *window = Instant::now();
            self.used.store(0, Ordering::Relaxed);
        }
    }
}

// Phantom use of `PageBuffer` so a future refactor that drops the
// type immediately surfaces here.
const _: fn() = || {
    let _ = std::mem::size_of::<PageBuffer>();
    let _ = PAGE_HEADER_LEN;
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::crypto::aes_gcm::PageCipher;
    use crate::storage::crypto::kdf::{MasterKey, derive_database_key};

    fn cipher_for(seed: u8, db: &str, epoch: u32) -> PageCipher {
        let m = MasterKey::new([seed; 32]);
        let k = derive_database_key(&m, db, epoch).unwrap();
        PageCipher::new(&k)
    }

    fn primed_store_under(
        seed: u8,
        db: &str,
        epoch: u32,
        pages: &[(FileId, u64, &[u8])],
    ) -> InMemoryPageStore {
        let stream_old = EncryptedPageStream::new(cipher_for(seed, db, epoch));
        let store = InMemoryPageStore::new();
        for &(file_id, offset, pt) in pages {
            let page = stream_old.encrypt(file_id, offset, pt).expect("seed-enc");
            store.insert(
                PageRef {
                    file_id,
                    page_offset: offset,
                },
                page.0,
            );
        }
        store
    }

    #[test]
    fn read_path_falls_back_to_secondary() {
        // Pages encrypted under epoch 0; stream's primary is now
        // epoch 1 and secondary is epoch 0.
        let store = primed_store_under(1, "default", 0, &[(FileId::NodeStore, 0, b"old-data")]);

        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        stream.install_secondary(cipher_for(1, "default", 0));

        let page = store
            .read_page(PageRef {
                file_id: FileId::NodeStore,
                page_offset: 0,
            })
            .unwrap();
        let pt = stream.decrypt(0, &page).expect("read fallback");
        assert_eq!(pt, b"old-data");
    }

    #[test]
    fn read_path_without_secondary_fails_loudly() {
        let store = primed_store_under(1, "default", 0, &[(FileId::NodeStore, 0, b"old-data")]);
        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        // No secondary installed — read must fail with the AEAD
        // error rather than silently return garbage.
        let page = store
            .read_page(PageRef {
                file_id: FileId::NodeStore,
                page_offset: 0,
            })
            .unwrap();
        assert!(stream.decrypt(0, &page).is_err());
    }

    #[test]
    fn runner_rejects_run_without_secondary() {
        let store = InMemoryPageStore::new();
        let stream = EncryptedPageStream::new(cipher_for(1, "default", 0));
        let runner = RotationRunner::new(&stream, &store);
        let err = runner.run(RotationCheckpoint::default()).unwrap_err();
        assert!(matches!(err, RotationError::NoSecondary));
    }

    #[test]
    fn runner_rotates_every_page_to_primary() {
        let pages: Vec<(FileId, u64, &[u8])> = vec![
            (FileId::NodeStore, 0, b"alpha"),
            (FileId::NodeStore, 8192, b"beta"),
            (FileId::RelStore, 0, b"gamma"),
        ];
        let store = primed_store_under(1, "default", 0, &pages);

        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        stream.install_secondary(cipher_for(1, "default", 0));
        let runner = RotationRunner::new(&stream, &store).with_config(RotationRunnerConfig {
            byte_budget_per_second: 0,
            checkpoint_every: 1024,
        });

        let cp = runner.run(RotationCheckpoint::default()).expect("run");
        assert_eq!(cp.stats.pages_total, 3);
        assert_eq!(cp.stats.pages_rotated, 3);
        assert_eq!(cp.stats.pages_already_primary, 0);
        assert!(cp.stats.bytes_rotated > 0);

        // After rotation, every page must decrypt under the
        // primary alone — drop the secondary and re-read.
        stream.clear_secondary();
        for (file_id, offset, expected) in pages {
            let page = store
                .read_page(PageRef {
                    file_id,
                    page_offset: offset,
                })
                .unwrap();
            let pt = stream.decrypt(offset, &page).expect("primary-only");
            assert_eq!(pt, expected);
        }
    }

    #[test]
    fn runner_skips_pages_already_primary() {
        // Mix of pages: half encrypted under epoch 0, half under
        // epoch 1. The runner must touch only the epoch-0 ones.
        let mut pages_old: Vec<(FileId, u64, &[u8])> = vec![(FileId::NodeStore, 0, b"a")];
        let pages_new: Vec<(FileId, u64, &[u8])> = vec![(FileId::RelStore, 0, b"b")];

        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        let store = InMemoryPageStore::new();

        // Seed the new pages under the primary directly.
        for &(file_id, offset, pt) in &pages_new {
            let page = stream.encrypt(file_id, offset, pt).unwrap();
            store.insert(
                PageRef {
                    file_id,
                    page_offset: offset,
                },
                page.0,
            );
        }
        // Seed the old pages under epoch 0.
        let stream_old = EncryptedPageStream::new(cipher_for(1, "default", 0));
        for &(file_id, offset, pt) in &pages_old {
            let page = stream_old.encrypt(file_id, offset, pt).unwrap();
            store.insert(
                PageRef {
                    file_id,
                    page_offset: offset,
                },
                page.0,
            );
        }
        pages_old.extend(pages_new);

        stream.install_secondary(cipher_for(1, "default", 0));
        let runner = RotationRunner::new(&stream, &store);
        let cp = runner.run(RotationCheckpoint::default()).expect("run");
        assert_eq!(cp.stats.pages_total, 2);
        assert_eq!(cp.stats.pages_rotated, 1);
        assert_eq!(cp.stats.pages_already_primary, 1);
    }

    #[test]
    fn runner_resumes_from_checkpoint() {
        let pages: Vec<(FileId, u64, &[u8])> = vec![
            (FileId::NodeStore, 0, b"page-0"),
            (FileId::NodeStore, 8192, b"page-1"),
            (FileId::NodeStore, 16384, b"page-2"),
        ];
        let store = primed_store_under(1, "default", 0, &pages);
        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        stream.install_secondary(cipher_for(1, "default", 0));

        let runner = RotationRunner::new(&stream, &store);

        // Phase 1 — cancel after the first page rotates.
        let cancel = runner.cancel.clone();
        let cp_phase1 = {
            // Drive only one iteration manually by patching the
            // store to expose a single page first; simpler, we
            // pre-rotate page 0 by calling the runner with a
            // crafted checkpoint that stops at page 0.
            let mut cp = RotationCheckpoint::default();
            // Pretend the runner already rotated page 0.
            cp.last_rotated = Some(PageRef {
                file_id: FileId::NodeStore,
                page_offset: 0,
            });
            cp
        };
        let _ = cancel; // silence unused warning if any

        // Phase 2 — resume; runner should rotate pages 1 and 2 only.
        let cp_phase2 = runner.run(cp_phase1).expect("resume");
        assert_eq!(cp_phase2.stats.pages_rotated, 2);
        assert_eq!(cp_phase2.stats.pages_total, 2);
        assert_eq!(
            cp_phase2.last_rotated,
            Some(PageRef {
                file_id: FileId::NodeStore,
                page_offset: 16384,
            })
        );
    }

    #[test]
    fn runner_honours_cancel_flag() {
        let pages: Vec<(FileId, u64, &[u8])> = (0..10)
            .map(|i| (FileId::NodeStore, (i * 8192) as u64, b"x" as &[u8]))
            .collect();
        let store = primed_store_under(1, "default", 0, &pages);
        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        stream.install_secondary(cipher_for(1, "default", 0));

        let runner = RotationRunner::new(&stream, &store);
        runner.cancel.store(true, Ordering::Relaxed);
        let cp = runner.run(RotationCheckpoint::default()).expect("cancel");
        assert_eq!(cp.stats.pages_total, 0, "cancel before first iteration");
    }

    #[test]
    fn write_during_rotation_uses_primary() {
        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        stream.install_secondary(cipher_for(1, "default", 0));

        // A write issued while the secondary is installed must
        // produce a page that decrypts under the primary alone.
        let page = stream.encrypt(FileId::NodeStore, 0, b"new-write").unwrap();
        // Drop the secondary and verify the page still reads.
        stream.clear_secondary();
        let pt = stream.decrypt(0, page.as_slice()).expect("primary-only");
        assert_eq!(pt, b"new-write");
    }

    #[test]
    fn cleared_secondary_can_be_reinstalled() {
        let stream = EncryptedPageStream::new(cipher_for(1, "default", 1));
        assert!(!stream.has_secondary());
        stream.install_secondary(cipher_for(1, "default", 0));
        assert!(stream.has_secondary());
        stream.clear_secondary();
        assert!(!stream.has_secondary());
        // Reinstall to support a chained rotation epoch_1 -> epoch_2.
        stream.install_secondary(cipher_for(1, "default", 1));
        assert!(stream.has_secondary());
    }
}
