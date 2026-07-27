//! Small filesystem-durability helpers shared by the record store and the WAL.

use std::path::Path;

/// Best-effort fsync of the *parent directory* of `path`, so a freshly created
/// file's directory entry is durable — not just the file's own data.
///
/// POSIX does not guarantee a new file's directory entry survives a crash until
/// the containing directory is fsynced: an `fsync` on the file covers its data,
/// not the directory metadata that makes the file discoverable after recovery.
/// A crash between "file created + its own data fsynced" and "parent directory
/// fsynced" can leave the file's data on disk but its directory entry not, so
/// the file may appear missing, zero-length, or stale after the filesystem
/// recovers — even though the create+write+fsync the process performed looked
/// complete. Call this after creating (and fsyncing) a new file.
///
/// This is **best-effort**: a failure to open or fsync the directory is logged
/// but not propagated, so a filesystem/platform where directory fsync is
/// unsupported cannot turn a missing-durability-guarantee gap into a hard
/// startup failure. On non-Unix platforms (e.g. Windows) it is a no-op — std
/// exposes no directory fsync there and directory-metadata durability is the
/// OS/filesystem's responsibility.
#[cfg(unix)]
pub(crate) fn sync_parent_dir(path: &Path) {
    let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return;
    };
    match std::fs::File::open(parent) {
        Ok(dir) => {
            if let Err(e) = dir.sync_all() {
                tracing::warn!(
                    "sync_parent_dir: fsync of directory {} failed: {e}",
                    parent.display()
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                "sync_parent_dir: opening directory {} for fsync failed: {e}",
                parent.display()
            );
        }
    }
}

/// No-op on non-Unix platforms — see the Unix variant's documentation.
#[cfg(not(unix))]
pub(crate) fn sync_parent_dir(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;

    #[test]
    fn sync_parent_dir_of_existing_file_does_not_panic() {
        let ctx = TestContext::new();
        let file = ctx.path().join("f.dat");
        std::fs::write(&file, b"x").unwrap();
        // Must complete cleanly on every platform (a real fsync on Unix, a
        // no-op elsewhere) — the parent directory exists and is openable.
        sync_parent_dir(&file);
    }

    #[test]
    fn sync_parent_dir_with_no_meaningful_parent_does_not_panic() {
        // A bare relative filename has an empty parent; must be a clean no-op.
        sync_parent_dir(Path::new("f.dat"));
    }
}
