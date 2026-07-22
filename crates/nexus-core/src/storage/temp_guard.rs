//! RAII guard that best-effort removes a directory when it is dropped.
//!
//! Used to tie a temp directory's lifetime to a [`super::RecordStore`]'s
//! memory-mapped files via `Arc` reference counting instead of a
//! single-owner `TempDir` field: see [`super::record_store::RecordStore::new_temporary`].

use std::path::PathBuf;

/// Removes its directory (recursively) when the last `Arc` reference
/// wrapping it is dropped.
///
/// Intended to be wrapped in `Arc` and cloned alongside the resource
/// whose lifetime it should track (a memory-mapped record store, in
/// this crate) so directory removal happens exactly when the last live
/// clone of that resource goes away — never on a timer, never guessed
/// at from the outside.
pub(crate) struct TempDirGuard {
    dir: PathBuf,
}

impl TempDirGuard {
    /// Wrap `dir` so it is removed once every clone of the `Arc`
    /// wrapping this guard has been dropped.
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        // Best-effort: `Drop::drop` cannot return a `Result`, and a
        // cleanup failure must never panic during unwind. A missing
        // directory is expected (another cleanup path may have already
        // removed it) and silently ignored; anything else — e.g. a
        // file still locked by another process on Windows — is logged
        // so a persistent failure stays visible without aborting.
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    "TempDirGuard: failed to remove temp directory {}: {}",
                    self.dir.display(),
                    e
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_directory_on_drop() {
        let dir = std::env::temp_dir().join(format!(
            "nexus-temp-guard-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(dir.exists());

        drop(TempDirGuard::new(dir.clone()));

        assert!(!dir.exists());
    }

    #[test]
    fn tolerates_an_already_missing_directory() {
        let dir = std::env::temp_dir().join(format!(
            "nexus-temp-guard-test-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        assert!(!dir.exists());

        // Must not panic even though the directory was never created.
        drop(TempDirGuard::new(dir));
    }
}
