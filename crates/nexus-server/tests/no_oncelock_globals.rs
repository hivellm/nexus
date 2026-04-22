//! Anti-regression guard for the phase2 migration.
//!
//! The phase2a–phase2e migration moved every per-subsystem API state
//! off a process-wide `OnceLock<Arc<_>>` and onto the `NexusServer`
//! struct so two servers in the same process no longer silently share
//! state. That migration is valuable only as long as nobody reintroduces
//! the anti-pattern. This test walks `nexus-server/src/api/` and fails
//! if it encounters a `static .* OnceLock<...>` declaration.
//!
//! The check is intentionally textual — the crate already runs clippy
//! under `-D warnings`, so any false positive will be visible at review
//! time and easy to silence with the `ALLOW_ONCELOCK` escape hatch
//! below. If a future subsystem *legitimately* needs a singleton (e.g.
//! a process-wide atomic counter shared across listener threads that
//! cannot be parameterised on `NexusServer`), add the full file path to
//! `ALLOW_ONCELOCK` with a comment explaining why.

use std::fs;
use std::path::{Path, PathBuf};

/// Relative paths under `nexus-server/src/api/` that are ALLOWED to
/// declare `static <NAME>: OnceLock<...>`. Each entry must document
/// why the singleton is acceptable. The list is empty today.
const ALLOW_ONCELOCK: &[&str] = &[];

/// Walk `dir` recursively collecting every `.rs` file into `out`.
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_oncelock_statics_in_nexus_server_api() {
    // CARGO_MANIFEST_DIR points at `nexus-server/` when this test runs
    // via `cargo test -p nexus-server`. From there `src/api/` is the
    // scope that phase2 migrated off globals.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let api_dir = PathBuf::from(manifest_dir).join("src").join("api");
    assert!(
        api_dir.is_dir(),
        "expected {} to exist for the guard test",
        api_dir.display()
    );

    let mut files = Vec::new();
    collect_rust_files(&api_dir, &mut files);
    assert!(
        !files.is_empty(),
        "no .rs files collected under {} — guard test misconfigured",
        api_dir.display()
    );

    let mut offenders = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&api_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOW_ONCELOCK.iter().any(|allowed| rel == *allowed) {
            continue;
        }
        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // We only care about module-level `static …: OnceLock<…>` or
        // `static …: std::sync::OnceLock<…>` declarations. Locals
        // inside functions cannot participate in cross-request shared
        // state because they live for the duration of one call.
        for (lineno, line) in contents.lines().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("static ") && !trimmed.starts_with("pub static ") {
                continue;
            }
            if !trimmed.contains("OnceLock") {
                continue;
            }
            offenders.push(format!("{}:{}: {}", rel, lineno + 1, trimmed));
        }
    }

    assert!(
        offenders.is_empty(),
        "phase2 anti-regression: OnceLock<_> statics reintroduced in \
         nexus-server/src/api/:\n  {}\n\n\
         These modules were migrated onto NexusServer in phase2a–phase2e. \
         If you genuinely need a process-wide singleton (cross-listener \
         atomic counters, for example), add the relative path to the \
         ALLOW_ONCELOCK list in this test with a comment.",
        offenders.join("\n  ")
    );
}
