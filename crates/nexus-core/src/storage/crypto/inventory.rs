//! On-disk encryption-state inventory.
//!
//! # Why
//!
//! When `encryption.enabled = true`, the server must refuse to boot
//! against a half-encrypted database — a "mixed-mode" state, where
//! some on-disk pages carry the EaR magic and others do not, would
//! silently leak plaintext on every read of an unconverted file.
//! The detection is also useful with `enabled = false`: if the
//! operator booted an encrypted database under a flag-flipped
//! plaintext config, every read would surface ciphertext as record
//! bytes and corrupt the executor.
//!
//! # What this module does
//!
//! Walk an explicit list of file paths (or a data directory) and
//! classify each file by its first page header:
//!
//! * `Empty` — file is shorter than [`PAGE_HEADER_LEN`]. Treated as
//!   "no opinion" — fresh databases routinely contain zero-byte
//!   bootstrap files.
//! * `Plaintext` — the first 16 bytes do not match the EaR magic.
//!   The file's contents have not been written through the
//!   `EncryptedPageStream`.
//! * `Encrypted` — the first 16 bytes parse as a valid
//!   [`PageHeader`]. Carries the recovered `(file_id, generation)`
//!   pair so the operator log can confirm the catalog is what they
//!   expect.
//!
//! # What this module does NOT do
//!
//! This is a **boot-time invariant check**, not a wire-up. It does
//! not encrypt anything, decrypt anything, or replace LMDB's page
//! IO. Wiring is tracked under the storage-layer refactor track —
//! the catalog, mmap-backed record stores, and the page-cache
//! buffer pool each need their own seam before `EncryptedPageStream`
//! can plug in. This inventory is the floor those wirings will
//! report against.
//!
//! See `docs/security/ENCRYPTION_AT_REST.md` § "Mixed-mode
//! detection" for the operator-facing recipe.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::encrypted_file::{FileId, PAGE_HEADER_LEN, PageHeader};

/// Per-file classification result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEncryptionState {
    /// File is shorter than [`PAGE_HEADER_LEN`]. The contract is
    /// "no opinion": fresh databases legitimately keep empty
    /// bootstrap files, and a partially-written first page that
    /// crash-truncates short of the header is operationally
    /// indistinguishable from "not yet written".
    Empty,
    /// First page is plaintext (no EaR magic). Indicates the file
    /// has not been written through the encrypted page stream.
    Plaintext,
    /// First page parses as a valid encrypted page header. The
    /// `file_id` and `generation` fields are recovered from disk.
    Encrypted {
        /// File identifier recovered from the page header.
        file_id: FileId,
        /// Page generation recovered from the page header.
        generation: u32,
    },
}

/// Aggregate per-database inventory.
///
/// Each vector keeps the file path so the operator log can name
/// the offending files when a mixed-mode error is raised.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryReport {
    /// Files that classified as [`FileEncryptionState::Empty`].
    pub empty: Vec<PathBuf>,
    /// Files that classified as [`FileEncryptionState::Plaintext`].
    pub plaintext: Vec<PathBuf>,
    /// Files that classified as [`FileEncryptionState::Encrypted`].
    pub encrypted: Vec<(PathBuf, FileId, u32)>,
}

impl InventoryReport {
    /// `true` iff at least one plaintext file AND at least one
    /// encrypted file were found. Empty files do not count toward
    /// either side — they are "no opinion".
    #[must_use]
    pub fn is_mixed(&self) -> bool {
        !self.plaintext.is_empty() && !self.encrypted.is_empty()
    }

    /// Total number of files visited.
    #[must_use]
    pub fn total(&self) -> usize {
        self.empty.len() + self.plaintext.len() + self.encrypted.len()
    }

    /// Insert a classification result, dispatching to the
    /// appropriate vector.
    fn record(&mut self, path: PathBuf, state: FileEncryptionState) {
        match state {
            FileEncryptionState::Empty => self.empty.push(path),
            FileEncryptionState::Plaintext => self.plaintext.push(path),
            FileEncryptionState::Encrypted {
                file_id,
                generation,
            } => self.encrypted.push((path, file_id, generation)),
        }
    }
}

/// Errors the inventory can surface.
#[derive(Debug, Error)]
pub enum InventoryError {
    /// Mixed-mode database: at least one plaintext file alongside at
    /// least one encrypted file. The variant carries both lists so
    /// the operator log can name the offenders. Reported regardless
    /// of the configured encryption flag — a mixed database is
    /// always wrong.
    #[error(
        "ERR_ENCRYPTION_MIXED_MODE: data directory contains {plain_count} plaintext file(s) and {enc_count} encrypted file(s); refuse to boot",
        plain_count = .plaintext.len(),
        enc_count = .encrypted.len(),
    )]
    MixedMode {
        /// Plaintext-classified file paths.
        plaintext: Vec<PathBuf>,
        /// Encrypted-classified file paths.
        encrypted: Vec<PathBuf>,
    },
    /// Encryption is disabled in the boot config but the data
    /// directory contains encrypted files. Booting through them
    /// would feed ciphertext to the executor.
    #[error(
        "ERR_ENCRYPTION_UNEXPECTED_ENCRYPTED: encryption is disabled but {} file(s) carry the EaR magic",
        .files.len()
    )]
    UnexpectedEncrypted {
        /// Paths of the unexpectedly-encrypted files.
        files: Vec<PathBuf>,
    },
    /// Encryption is enabled in the boot config but the data
    /// directory contains plaintext files. Operators see this when
    /// they flipped the flag without running the (yet-to-ship) data
    /// migration verb. Distinct from `MixedMode` because there are
    /// zero encrypted files — every byte is plaintext.
    #[error(
        "ERR_ENCRYPTION_NOT_INITIALIZED: encryption is enabled but {} file(s) are plaintext; run `nexus admin encrypt-database` once it ships",
        .files.len()
    )]
    UnexpectedPlaintext {
        /// Paths of the unexpectedly-plaintext files.
        files: Vec<PathBuf>,
    },
    /// IO failure reading a file during the scan. The scan does not
    /// retry — a half-readable directory is an operator problem
    /// (filesystem corruption, permission denial); booting forward
    /// would only mask the issue.
    #[error("ERR_INVENTORY_IO: failed to read {path}: {source}")]
    Io {
        /// Path the scan failed on.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: io::Error,
    },
}

/// Classify a single file by its first 16 bytes.
pub fn classify_file(path: &Path) -> Result<FileEncryptionState, InventoryError> {
    let mut f = fs::File::open(path).map_err(|e| InventoryError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut buf = [0u8; PAGE_HEADER_LEN];
    let read = read_exact_or_short(&mut f, &mut buf).map_err(|e| InventoryError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    if read < PAGE_HEADER_LEN {
        return Ok(FileEncryptionState::Empty);
    }
    match PageHeader::from_bytes(&buf) {
        Some(h) => Ok(FileEncryptionState::Encrypted {
            file_id: h.file_id,
            generation: h.generation,
        }),
        None => Ok(FileEncryptionState::Plaintext),
    }
}

/// Read up to `buf.len()` bytes, returning the actual count read
/// (so a short file does not surface as an error). Distinct from
/// `Read::read_exact` because EOF before `buf.len()` is a normal
/// outcome here, not an error.
fn read_exact_or_short<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match r.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

/// Classify every path in `paths` and return the aggregate report.
///
/// The caller is responsible for filtering directory entries down
/// to the storage-relevant set (catalog files, record stores, WAL
/// segments, index files) — this function does not assume a
/// specific data-directory layout.
pub fn scan_paths<I, P>(paths: I) -> Result<InventoryReport, InventoryError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut report = InventoryReport::default();
    for path in paths {
        let path = path.as_ref().to_path_buf();
        let state = classify_file(&path)?;
        report.record(path, state);
    }
    Ok(report)
}

/// Walk `data_dir` recursively and classify every regular file,
/// skipping the well-known non-storage artifacts that LMDB / heed /
/// the WAL writer routinely leave around (`.lock`, `.tmp`,
/// `LOG`, `LOST.DIR`, etc.).
///
/// The skip list is conservative on purpose: false positives —
/// classifying a non-storage file — are loud (they always look
/// plaintext) but never silent. False negatives — skipping an
/// actual storage file — would defeat the mixed-mode check, so the
/// skip list errs on the side of "include unless we know it's
/// junk".
pub fn scan_directory(data_dir: &Path) -> Result<InventoryReport, InventoryError> {
    let mut report = InventoryReport::default();
    walk(data_dir, &mut report)?;
    Ok(report)
}

fn walk(dir: &Path, report: &mut InventoryReport) -> Result<(), InventoryError> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(InventoryError::Io {
                path: dir.to_path_buf(),
                source: e,
            });
        }
    };
    for entry in entries {
        let entry = entry.map_err(|e| InventoryError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| InventoryError::Io {
            path: path.clone(),
            source: e,
        })?;
        if ft.is_dir() {
            walk(&path, report)?;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        if should_skip(&path) {
            continue;
        }
        let state = classify_file(&path)?;
        report.record(path, state);
    }
    Ok(())
}

/// Names that are never storage data — heed/LMDB lock files, the
/// WAL writer's temp staging, log files. Everything else passes
/// through to the classifier.
fn should_skip(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return true;
    };
    matches!(
        name,
        "lock.mdb" | "data.mdb-lock" | "LOCK" | "LOG" | "LOG.old" | ".DS_Store"
    ) || name.ends_with(".tmp")
        || name.ends_with(".lock")
}

/// Boot-time invariant: assert that the on-disk state matches the
/// configured encryption flag. Returns the inventory on success so
/// the caller (the server boot path) can surface counts on
/// `/admin/encryption/status`.
///
/// Decision matrix:
///
/// | enabled | plaintext | encrypted | outcome |
/// |---------|-----------|-----------|---------|
/// | any     | ≥1        | ≥1        | `MixedMode`              |
/// | true    | 0         | any       | OK                       |
/// | true    | ≥1        | 0         | `UnexpectedPlaintext`    |
/// | false   | any       | 0         | OK                       |
/// | false   | 0         | ≥1        | `UnexpectedEncrypted`    |
///
/// Empty files never trigger a failure on their own; a fresh
/// database that is mid-bootstrap may legitimately have zero-byte
/// files with no first page yet written.
pub fn enforce_uniform_state(
    report: InventoryReport,
    encryption_enabled: bool,
) -> Result<InventoryReport, InventoryError> {
    if report.is_mixed() {
        return Err(InventoryError::MixedMode {
            plaintext: report.plaintext.clone(),
            encrypted: report.encrypted.iter().map(|(p, _, _)| p.clone()).collect(),
        });
    }
    if encryption_enabled && !report.plaintext.is_empty() {
        return Err(InventoryError::UnexpectedPlaintext {
            files: report.plaintext.clone(),
        });
    }
    if !encryption_enabled && !report.encrypted.is_empty() {
        return Err(InventoryError::UnexpectedEncrypted {
            files: report.encrypted.iter().map(|(p, _, _)| p.clone()).collect(),
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(path: &Path, bytes: &[u8]) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    fn encrypted_first_page(file_id: FileId, generation: u32) -> Vec<u8> {
        // Only the first PAGE_HEADER_LEN bytes need to parse as a
        // valid header; the rest of the page is irrelevant — the
        // classifier never decrypts the body.
        let header = PageHeader {
            file_id,
            generation,
        };
        let mut out = header.to_bytes().to_vec();
        out.resize(64, 0);
        out
    }

    #[test]
    fn classify_empty_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("zero.bin");
        write_file(&p, b"");
        assert_eq!(classify_file(&p).unwrap(), FileEncryptionState::Empty);
    }

    #[test]
    fn classify_short_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("short.bin");
        write_file(&p, &[0u8; PAGE_HEADER_LEN - 1]);
        assert_eq!(classify_file(&p).unwrap(), FileEncryptionState::Empty);
    }

    #[test]
    fn classify_plaintext_when_no_magic() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("plain.bin");
        // 16 zero bytes — magic mismatch, classifier returns
        // Plaintext.
        write_file(&p, &[0u8; PAGE_HEADER_LEN]);
        assert_eq!(classify_file(&p).unwrap(), FileEncryptionState::Plaintext);
    }

    #[test]
    fn classify_encrypted_recovers_file_id_and_generation() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("enc.bin");
        write_file(&p, &encrypted_first_page(FileId::PropertyStore, 42));
        match classify_file(&p).unwrap() {
            FileEncryptionState::Encrypted {
                file_id,
                generation,
            } => {
                assert_eq!(file_id, FileId::PropertyStore);
                assert_eq!(generation, 42);
            }
            other => panic!("expected Encrypted, got {other:?}"),
        }
    }

    #[test]
    fn scan_paths_aggregates_state() {
        let dir = TempDir::new().unwrap();
        let plain = dir.path().join("plain.bin");
        let enc = dir.path().join("enc.bin");
        let empty = dir.path().join("empty.bin");
        write_file(&plain, &[0u8; PAGE_HEADER_LEN]);
        write_file(&enc, &encrypted_first_page(FileId::NodeStore, 1));
        write_file(&empty, b"");
        let report = scan_paths([&plain, &enc, &empty]).unwrap();
        assert_eq!(report.plaintext, vec![plain]);
        assert_eq!(report.encrypted.len(), 1);
        assert_eq!(report.encrypted[0].1, FileId::NodeStore);
        assert_eq!(report.empty.len(), 1);
        assert_eq!(report.total(), 3);
    }

    #[test]
    fn scan_directory_skips_lock_files_and_recurses() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("sub");
        fs::create_dir_all(&nested).unwrap();
        write_file(
            &nested.join("data.bin"),
            &encrypted_first_page(FileId::Wal, 7),
        );
        write_file(&dir.path().join("lock.mdb"), &[0u8; 32]);
        write_file(&dir.path().join("staging.tmp"), &[0u8; 32]);
        let report = scan_directory(dir.path()).unwrap();
        assert_eq!(report.encrypted.len(), 1, "report = {report:?}");
        assert_eq!(report.encrypted[0].1, FileId::Wal);
        assert!(report.plaintext.is_empty());
    }

    #[test]
    fn scan_directory_missing_returns_empty_report() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist");
        let report = scan_directory(&missing).unwrap();
        assert_eq!(report.total(), 0);
    }

    #[test]
    fn enforce_rejects_mixed_mode() {
        let mut report = InventoryReport::default();
        report.plaintext.push(PathBuf::from("a"));
        report
            .encrypted
            .push((PathBuf::from("b"), FileId::Catalog, 1));
        let err = enforce_uniform_state(report, true).unwrap_err();
        match err {
            InventoryError::MixedMode {
                plaintext,
                encrypted,
            } => {
                assert_eq!(plaintext, vec![PathBuf::from("a")]);
                assert_eq!(encrypted, vec![PathBuf::from("b")]);
            }
            other => panic!("expected MixedMode, got {other:?}"),
        }
    }

    #[test]
    fn enforce_rejects_mixed_mode_regardless_of_flag() {
        // Mixed mode is always wrong, even when encryption is off.
        let mut report = InventoryReport::default();
        report.plaintext.push(PathBuf::from("a"));
        report
            .encrypted
            .push((PathBuf::from("b"), FileId::Catalog, 1));
        let err = enforce_uniform_state(report, false).unwrap_err();
        assert!(matches!(err, InventoryError::MixedMode { .. }));
    }

    #[test]
    fn enforce_rejects_plaintext_when_enabled() {
        let mut report = InventoryReport::default();
        report.plaintext.push(PathBuf::from("a"));
        let err = enforce_uniform_state(report, true).unwrap_err();
        assert!(matches!(err, InventoryError::UnexpectedPlaintext { .. }));
    }

    #[test]
    fn enforce_rejects_encrypted_when_disabled() {
        let mut report = InventoryReport::default();
        report.encrypted.push((PathBuf::from("a"), FileId::Wal, 0));
        let err = enforce_uniform_state(report, false).unwrap_err();
        assert!(matches!(err, InventoryError::UnexpectedEncrypted { .. }));
    }

    #[test]
    fn enforce_accepts_uniform_encrypted_when_enabled() {
        let mut report = InventoryReport::default();
        report
            .encrypted
            .push((PathBuf::from("a"), FileId::Catalog, 1));
        let out = enforce_uniform_state(report, true).expect("enforce");
        assert_eq!(out.encrypted.len(), 1);
    }

    #[test]
    fn enforce_accepts_uniform_plaintext_when_disabled() {
        let mut report = InventoryReport::default();
        report.plaintext.push(PathBuf::from("a"));
        let out = enforce_uniform_state(report, false).expect("enforce");
        assert_eq!(out.plaintext.len(), 1);
    }

    #[test]
    fn enforce_accepts_empty_only_state() {
        // A fresh data dir whose files are all zero-byte (no first
        // page yet written) is a valid state under either flag.
        let mut report = InventoryReport::default();
        report.empty.push(PathBuf::from("bootstrap"));
        let out = enforce_uniform_state(report.clone(), true).expect("enforce-on");
        assert_eq!(out.empty.len(), 1);
        let out = enforce_uniform_state(report, false).expect("enforce-off");
        assert_eq!(out.empty.len(), 1);
    }

    #[test]
    fn classify_io_error_surfaces_path() {
        let err = classify_file(Path::new("/this/path/does/not/exist")).unwrap_err();
        match err {
            InventoryError::Io { path, .. } => {
                assert_eq!(path, PathBuf::from("/this/path/does/not/exist"));
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
