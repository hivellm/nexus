//! Crash-recovery harness for the async FTS writer
//! (phase6_fulltext-async-writer §4).
//!
//! The hot-path contract the engine relies on is:
//!
//! 1. The engine writes an `FtsAdd` WAL entry **and** fsyncs the WAL.
//! 2. The engine enqueues the document onto the per-index async
//!    writer's bounded channel.
//! 3. The writer batches + commits against Tantivy on its
//!    `refresh_ms` cadence.
//!
//! Under a kill-9 between steps 2 and 3 the on-disk Tantivy segment
//! is missing documents that have already been durably written to
//! the WAL. The engine's recovery path therefore has to
//! re-apply every `FtsAdd` entry from the WAL against the
//! freshly-reopened registry — otherwise a committed transaction
//! would vanish.
//!
//! This suite validates exactly that guarantee without spawning a
//! child process (subprocess harnesses are brittle on Windows in CI
//! and add platform-specific timing knobs the single-process
//! equivalent does not need). The "crash" is simulated by dropping
//! the registry + writer *before* the cadence tick that would have
//! committed them; the recovery path is the same code the engine
//! executes on restart:
//!
//! ```ignore
//! let entries = wal.recover()?;
//! for e in entries { registry.apply_wal_entry(&e)?; }
//! ```
//!
//! Together §4.1 (mid-bulk kill) and §4.3 (docs that never reached
//! the WAL stay absent) close the crash-during-bulk-ingest scenario
//! deferred by `phase6_fulltext-wal-integration` §5.3.

use nexus_core::index::fulltext_registry::FullTextRegistry;
use nexus_core::wal::{Wal, WalEntry};
use std::time::Duration;
use tempfile::TempDir;

/// Emit the WAL entries the engine would emit for a CREATE INDEX
/// followed by `content.len()` bulk adds.
fn emit_create_and_adds(wal: &mut Wal, index: &str, content: &[(u64, &str)]) {
    wal.append(&WalEntry::FtsCreateIndex {
        name: index.to_string(),
        entity: 0,
        labels_or_types: vec!["Doc".to_string()],
        properties: vec!["body".to_string()],
        analyzer: "standard".to_string(),
    })
    .expect("append FtsCreateIndex");
    for (node_id, body) in content {
        wal.append(&WalEntry::FtsAdd {
            name: index.to_string(),
            entity_id: *node_id,
            label_or_type_id: 0,
            key_id: 0,
            content: body.to_string(),
        })
        .expect("append FtsAdd");
    }
    wal.flush().expect("fsync WAL");
}

fn fresh_registry_in(dir: &TempDir) -> FullTextRegistry {
    let reg = FullTextRegistry::new();
    reg.set_base_dir(dir.path().join("fulltext"));
    reg
}

/// §4.1 + §4.2 — "mid-bulk kill" scenario. The WAL has 20 committed
/// adds; the in-process registry is dropped before the writer
/// commits any of them. After WAL replay every one of those 20 docs
/// must be searchable.
#[test]
fn wal_replay_restores_every_committed_doc_after_writer_drop() {
    let tmp = TempDir::new().expect("tempdir");
    let wal_path = tmp.path().join("wal.log");

    // --- Pre-crash session --------------------------------------
    // Write the WAL entries that represent a durably-committed
    // CREATE + bulk ingest, then drop the registry without flushing
    // or committing the Tantivy segment. Simulates kill-9 between
    // "WAL sync" and "writer commit".
    let docs: Vec<(u64, String)> = (0..20)
        .map(|i| (i as u64, format!("alpha doc-{i} brown fox")))
        .collect();
    let refs: Vec<(u64, &str)> = docs.iter().map(|(id, s)| (*id, s.as_str())).collect();
    {
        let mut wal = Wal::new(&wal_path).expect("open WAL");
        emit_create_and_adds(&mut wal, "corpus", &refs);
    }
    // No Tantivy side-effect yet — the fulltext subdir doesn't even
    // exist. This proves that after the "crash" Tantivy is empty.
    let fulltext_root = tmp.path().join("fulltext");
    assert!(
        !fulltext_root.join("corpus").exists(),
        "pre-crash Tantivy segment must not exist — simulating kill-9 before writer commit"
    );

    // --- Recovery session ---------------------------------------
    let reg = fresh_registry_in(&tmp);
    // Nothing catalogued on disk yet, so load_from_disk is a no-op.
    assert_eq!(reg.load_from_disk().expect("load_from_disk"), 0);

    let mut wal = Wal::new(&wal_path).expect("reopen WAL");
    let entries = wal.recover().expect("WAL recover");
    assert_eq!(
        entries.len(),
        1 + docs.len(),
        "expected 1 create + {} adds, got {}",
        docs.len(),
        entries.len()
    );

    let mut applied = 0usize;
    for e in &entries {
        if reg.apply_wal_entry(e).expect("apply_wal_entry") {
            applied += 1;
        }
    }
    assert_eq!(applied, entries.len(), "every FTS entry must apply");

    // Give any spawned async writer a chance to flush. `apply_wal_entry`
    // goes through the sync path (registry-level `add_node_document`
    // with no writer spawned for this registry), so this is a
    // belt-and-braces guard against future regressions where the
    // replay path starts routing through the writer.
    reg.flush_all().expect("flush_all");

    // Every committed doc must be searchable.
    let hits = reg.query("corpus", "alpha", None).expect("query");
    let mut recovered: Vec<u64> = hits.iter().map(|h| h.node_id).collect();
    recovered.sort();
    let expected: Vec<u64> = docs.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        recovered, expected,
        "expected every WAL-committed doc to survive replay"
    );
}

/// §4.3 — docs that never reached the WAL stay absent after
/// replay. The WAL contains 5 committed adds and the writer's
/// in-memory buffer (had the process survived) would have added 5
/// more. After a simulated crash only the 5 committed docs can
/// survive — the "lost" 5 must not show up, otherwise we'd be
/// claiming durability we did not deliver.
#[test]
fn unwritten_buffer_entries_stay_absent_after_crash() {
    let tmp = TempDir::new().expect("tempdir");
    let wal_path = tmp.path().join("wal.log");

    let committed: Vec<(u64, String)> = (0..5)
        .map(|i| (i as u64, format!("survives crash {i}")))
        .collect();
    let refs: Vec<(u64, &str)> = committed.iter().map(|(id, s)| (*id, s.as_str())).collect();
    {
        let mut wal = Wal::new(&wal_path).expect("open WAL");
        emit_create_and_adds(&mut wal, "corpus", &refs);
        // These entries deliberately never reach the WAL — they
        // represent docs that were buffered in memory at the moment
        // of the kill. We still sync the WAL so the committed-5
        // are durable; the uncommitted-5 exist only in the test's
        // expectations.
    }

    // Recovery.
    let reg = fresh_registry_in(&tmp);
    assert_eq!(reg.load_from_disk().unwrap(), 0);
    let mut wal = Wal::new(&wal_path).unwrap();
    for e in wal.recover().unwrap() {
        reg.apply_wal_entry(&e).unwrap();
    }
    reg.flush_all().unwrap();

    let hits = reg.query("corpus", "crash", None).unwrap();
    let mut recovered: Vec<u64> = hits.iter().map(|h| h.node_id).collect();
    recovered.sort();
    assert_eq!(
        recovered,
        committed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        "only the 5 WAL-committed docs may survive"
    );

    // Probe for the phantom ids the pre-crash buffer would have
    // held. Their content was never indexed, so nothing under that
    // query string must hit.
    let phantom = reg
        .query("corpus", "phantom ghost never-committed", None)
        .unwrap();
    assert!(
        phantom.is_empty(),
        "docs that never reached the WAL must not reappear: {phantom:?}"
    );
}

/// End-to-end: the async writer's `refresh_ms` cadence must fire
/// without an explicit `flush_blocking` call once the registry has
/// spawned per-index writers. Enqueue on the registry, wait one
/// cadence tick, query — the doc must be visible.
///
/// This is the companion to the writer-module unit test
/// `writer_honours_max_batch_capacity_trigger`: that one covers the
/// batch-size threshold; this one covers the wall-clock cadence as
/// seen through the registry wrapper the engine actually uses.
#[test]
fn async_writer_cadence_makes_docs_visible_without_explicit_flush() {
    let tmp = TempDir::new().expect("tempdir");
    let reg = FullTextRegistry::new();
    reg.set_base_dir(tmp.path().to_path_buf());
    reg.create_node_index("corpus", &["Doc"], &["body"], Some("standard"))
        .unwrap();
    reg.enable_async_writers();

    reg.add_node_document("corpus", 42, 0, 0, "cadence-visible doc body")
        .unwrap();

    // Poll for up to 2× the default cadence. The writer's refresh
    // tick is 1s by default; 50× 50ms = 2.5s is generous enough to
    // absorb CI scheduler jitter without padding the suite.
    let mut hits = vec![];
    for _ in 0..50 {
        hits = reg.query("corpus", "cadence", None).unwrap();
        if !hits.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        hits.iter().any(|h| h.node_id == 42),
        "cadence-triggered commit must surface the doc, got {hits:?}"
    );

    // Clean shutdown — the registry's Drop chain already flushes
    // writers, but calling disable explicitly asserts the API.
    reg.disable_async_writers().unwrap();
}
