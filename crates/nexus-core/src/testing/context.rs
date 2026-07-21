//! TestContext - Manages test lifecycle and resource cleanup
//!
//! TestContext ensures that all resources allocated during a test are properly
//! cleaned up when the test completes, preventing resource leaks and race conditions.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// A single dedicated base directory under the system temp dir for every
/// `TestContext` temp directory. Isolating them under one root lets the
/// process-start sweep find and remove leftovers from earlier (crashed or
/// order-of-drop-defeated) runs without ever touching unrelated temp files.
fn test_tmp_base() -> PathBuf {
    std::env::temp_dir().join("nexus-test-tmp")
}

/// Directories whose immediate removal failed (Windows keeps the catalog's
/// LMDB / mmap files locked until the owning `Engine` is fully dropped, and
/// `let (engine, _ctx) = ...` drops `_ctx` BEFORE `engine`). They are retried
/// on every later `TestContext` drop — by then the previous test's engine has
/// dropped and released its handles (see `catalog::store::EnvCloser`), so the
/// directory becomes removable.
static PENDING_CLEANUP: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Any `TestContext` temp directory older than this cannot belong to a
/// still-running test (no single test holds its context this long), so the
/// process-start sweep can remove it safely even while other test binaries run
/// concurrently against the shared base.
const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

static SWEEP_ONCE: Once = Once::new();

/// Remove stale leftover directories from prior runs. Runs once per process,
/// before the first `TestContext` is created. Only touches entries older than
/// [`STALE_AFTER`], so a concurrently-running test binary's fresh directories
/// are never disturbed. Leftovers are from processes that have exited, so their
/// files are unlocked and remove fully.
fn sweep_stale_leftovers_once(base: &Path) {
    SWEEP_ONCE.call_once(|| {
        let Ok(entries) = std::fs::read_dir(base) else {
            return;
        };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let is_stale = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|mtime| now.duration_since(mtime).unwrap_or_default() > STALE_AFTER)
                .unwrap_or(false);
            if is_stale {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    });
}

/// Best-effort removal of a finished test's directory. On success it is gone; on
/// failure (its owning engine is not dropped yet) it is queued for a later
/// retry. Every call also drains the queue, so directories left behind by
/// earlier tests in this process are reclaimed as soon as their engines drop.
fn reclaim_dir(path: PathBuf) {
    let mut pending = PENDING_CLEANUP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if std::fs::remove_dir_all(&path).is_err() && path.exists() {
        pending.push(path);
    }
    // Retry everything queued so far; keep only the ones still locked.
    pending.retain(|p| std::fs::remove_dir_all(p).is_err() && p.exists());
}

/// Context for managing test resources and lifecycle
///
/// TestContext automatically cleans up all resources when dropped, ensuring
/// that temporary directories are removed and LMDB locks are released.
///
/// # Example
///
/// ```rust,no_run
/// use nexus_core::testing::TestContext;
///
/// #[test]
/// fn my_test() {
///     let ctx = TestContext::new();
///     // Use ctx.path() to get the test directory
///     // All resources are cleaned up when ctx is dropped
/// }
/// ```
pub struct TestContext {
    /// Temporary directory for this test. `Option` so `Drop` can disarm
    /// `tempfile`'s own (Windows-racy) removal and take manual control.
    temp_dir: Option<TempDir>,
    /// Additional resources to clean up
    resources: Vec<Box<dyn Any>>,
}

impl TestContext {
    /// Create a new TestContext with a unique temporary directory
    ///
    /// The directory is guaranteed to exist before this function returns.
    pub fn new() -> Self {
        let base = test_tmp_base();
        std::fs::create_dir_all(&base).expect("Failed to create test temp base directory");
        // Clear leftovers from previous runs before creating this run's dir.
        sweep_stale_leftovers_once(&base);

        let temp_dir = TempDir::new_in(&base).expect("Failed to create temporary directory");

        // CRITICAL: Ensure directory exists before returning
        // This prevents race conditions in high-parallelism environments
        std::fs::create_dir_all(temp_dir.path()).expect("Failed to create test directory");

        Self {
            temp_dir: Some(temp_dir),
            resources: Vec::new(),
        }
    }

    /// Get the path to the temporary directory
    ///
    /// This path is guaranteed to exist and will be cleaned up when the
    /// TestContext is dropped.
    pub fn path(&self) -> &Path {
        self.temp_dir
            .as_ref()
            .expect("TestContext temp dir already taken")
            .path()
    }

    /// Register an additional resource for cleanup
    ///
    /// Resources are cleaned up in reverse order of registration when
    /// the TestContext is dropped.
    pub fn register<T: Any>(&mut self, resource: T) {
        self.resources.push(Box::new(resource));
    }

    /// Get the temporary directory handle
    ///
    /// This is useful if you need to keep the directory alive beyond
    /// the TestContext lifetime. Takes ownership of the `TempDir`, so this
    /// `TestContext`'s `Drop` no longer manages that directory.
    pub fn into_temp_dir(mut self) -> TempDir {
        self.temp_dir
            .take()
            .expect("TestContext temp dir already taken")
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Reclaim this test's directory (and any still-queued from earlier
        // tests). `keep()` disarms tempfile's own removal — on Windows it would
        // run now, while the engine (dropped AFTER this context in the common
        // `let (engine, _ctx) = ...` binding) still holds the catalog's LMDB
        // files open, silently fail, and leak the directory. Our deferred retry
        // reclaims it once that engine drops.
        if let Some(td) = self.temp_dir.take() {
            reclaim_dir(td.keep());
        }
    }
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_context_creates_directory() {
        let ctx = TestContext::new();
        let path = ctx.path();

        assert!(path.exists(), "Test directory should exist");
        assert!(path.is_dir(), "Test path should be a directory");
    }

    #[test]
    fn test_context_path_is_writable() {
        let ctx = TestContext::new();
        let path = ctx.path();
        let test_file = path.join("test.txt");

        fs::write(&test_file, "test").expect("Should be able to write to test directory");
        assert!(test_file.exists(), "Test file should exist");
    }

    #[test]
    fn test_context_cleanup_on_drop() {
        // A context with no engine holding files open must remove its directory
        // immediately on drop (the deferred-retry path is only needed when an
        // engine still holds the catalog locked).
        let path = {
            let ctx = TestContext::new();
            let path = ctx.path().to_path_buf();
            fs::write(path.join("test.txt"), "test").unwrap();
            path
        };
        assert!(
            !path.exists(),
            "directory with no open handles must be removed on TestContext drop"
        );
    }

    #[test]
    fn test_context_under_dedicated_base() {
        let ctx = TestContext::new();
        assert!(
            ctx.path().starts_with(test_tmp_base()),
            "test dirs must live under the dedicated sweepable base"
        );
    }
}
