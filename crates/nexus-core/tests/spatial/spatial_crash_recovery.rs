//! Crash-recovery harness for the R-tree registry
//! (phase6_spatial-index-autopopulate §6).
//!
//! The hot-path contract the engine relies on is:
//!
//! 1. The engine writes an `RTreeInsert` / `RTreeDelete` WAL entry
//!    **and** fsyncs the WAL.
//! 2. The R-tree registry's in-memory tree is updated by the same
//!    code path that emitted the WAL entry.
//! 3. On a clean shutdown the registry's per-index trees may or may
//!    not have been flushed to disk — the contract says only the WAL
//!    is durable.
//!
//! Under a kill-9 between steps 1 and 3 the in-memory tree is gone
//! but the WAL is durable. The engine's recovery path therefore has
//! to re-apply every R-tree entry from the WAL against a freshly-
//! reopened registry — otherwise a committed `CREATE (:Place {...})`
//! would vanish from `spatial.nearest`.
//!
//! This suite mirrors `fulltext_crash_recovery.rs` line-for-line:
//!
//! ```ignore
//! let entries = wal.recover()?;
//! for e in entries { registry.apply_wal_entry(&e)?; }
//! ```
//!
//! Together §6.2 (mid-ingest kill) and §6.3 (entries that never hit
//! the WAL stay absent) close the crash-during-bulk-ingest scenario.

use nexus_core::index::rtree::registry::RTreeRegistry;
use nexus_core::index::rtree::search::Metric;
use nexus_core::wal::{Wal, WalEntry};
use tempfile::TempDir;

const INDEX_NAME: &str = "Place.loc";

/// Emit the WAL entries the engine would emit for a CREATE INDEX
/// (implicit — replay registers via the first `RTreeInsert`) followed
/// by `points.len()` bulk auto-populate inserts.
fn emit_inserts(wal: &mut Wal, points: &[(u64, f64, f64)]) {
    for (node_id, x, y) in points {
        wal.append(&WalEntry::RTreeInsert {
            index_name: INDEX_NAME.to_string(),
            node_id: *node_id,
            x: *x,
            y: *y,
        })
        .expect("append RTreeInsert");
    }
    wal.flush().expect("fsync WAL");
}

fn collect_visible_ids(reg: &RTreeRegistry, k: usize) -> Vec<u64> {
    // Centred on the origin, large enough k to over-fetch the
    // committed set, no visibility filtering — every node is
    // considered visible for the purposes of the recovery test.
    let hits = reg
        .nearest_with_filter(INDEX_NAME, 0.0, 0.0, k, Metric::Cartesian, |_| true)
        .expect("nearest_with_filter");
    let mut ids: Vec<u64> = hits.into_iter().map(|h| h.node_id).collect();
    ids.sort();
    ids
}

/// §6.2 — "mid-ingest kill" scenario. The WAL has 20 committed
/// `RTreeInsert` entries; the in-process registry is dropped before
/// any flush. After WAL replay every one of those 20 points must be
/// reachable through `nearest_with_filter`.
#[test]
fn wal_replay_restores_every_committed_point_after_registry_drop() {
    let tmp = TempDir::new().expect("tempdir");
    let wal_path = tmp.path().join("wal.log");

    // --- Pre-crash session --------------------------------------
    // Write the WAL entries that represent a durably-committed
    // CREATE + bulk auto-populate, then drop the registry without
    // flushing. Simulates kill-9 between WAL sync and any later
    // checkpoint.
    let points: Vec<(u64, f64, f64)> = (0..20)
        .map(|i| (i as u64, i as f64, (i * 2) as f64))
        .collect();
    {
        let mut wal = Wal::new(&wal_path).expect("open WAL");
        emit_inserts(&mut wal, &points);
    }

    // --- Recovery session ---------------------------------------
    let reg = RTreeRegistry::new();
    assert!(
        reg.is_empty(),
        "fresh registry must be empty before WAL replay"
    );

    let mut wal = Wal::new(&wal_path).expect("reopen WAL");
    let entries = wal.recover().expect("WAL recover");
    assert_eq!(
        entries.len(),
        points.len(),
        "expected {} R-tree inserts, got {}",
        points.len(),
        entries.len()
    );

    for e in &entries {
        reg.apply_wal_entry(e).expect("apply_wal_entry");
    }

    // Every committed point must surface through nearest_with_filter.
    let recovered = collect_visible_ids(&reg, points.len());
    let expected: Vec<u64> = points.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        recovered, expected,
        "expected every WAL-committed point to survive replay"
    );
}

/// §6.3 — entries that never reached the WAL stay absent after
/// replay. The WAL contains 5 committed inserts and the engine's
/// in-memory tree (had the process survived) would have added 5
/// more. After a simulated crash only the 5 committed points can
/// survive — the "lost" 5 must not show up, otherwise we'd be
/// claiming durability we did not deliver.
#[test]
fn unflushed_entries_stay_absent_after_crash() {
    let tmp = TempDir::new().expect("tempdir");
    let wal_path = tmp.path().join("wal.log");

    let committed: Vec<(u64, f64, f64)> = (0..5).map(|i| (i as u64, i as f64, i as f64)).collect();
    let phantom: Vec<(u64, f64, f64)> =
        (100..105).map(|i| (i as u64, i as f64, i as f64)).collect();
    {
        let mut wal = Wal::new(&wal_path).expect("open WAL");
        emit_inserts(&mut wal, &committed);
        // The `phantom` entries deliberately never reach the WAL —
        // they represent points that were buffered in memory at the
        // moment of the kill. We still sync the committed-5 so they
        // are durable; the uncommitted-5 exist only in this test's
        // expectations.
    }

    // Recovery.
    let reg = RTreeRegistry::new();
    let mut wal = Wal::new(&wal_path).expect("reopen WAL");
    for e in wal.recover().expect("WAL recover") {
        reg.apply_wal_entry(&e).expect("apply_wal_entry");
    }

    let recovered = collect_visible_ids(&reg, 32);
    let expected: Vec<u64> = committed.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        recovered, expected,
        "only the 5 WAL-committed points may survive"
    );

    // Probe for the phantom ids the pre-crash buffer would have
    // held. None of their node ids may surface.
    for (phantom_id, _, _) in &phantom {
        assert!(
            !recovered.contains(phantom_id),
            "phantom id {phantom_id} must not reappear after replay: {recovered:?}"
        );
    }
}

/// §6.2 + §6.3 mixed: WAL contains an insert/delete pair for the
/// same node id. Replay must converge to the post-delete state —
/// the node must NOT surface after recovery.
#[test]
fn wal_replay_honours_insert_then_delete_ordering() {
    let tmp = TempDir::new().expect("tempdir");
    let wal_path = tmp.path().join("wal.log");

    {
        let mut wal = Wal::new(&wal_path).expect("open WAL");
        wal.append(&WalEntry::RTreeInsert {
            index_name: INDEX_NAME.to_string(),
            node_id: 42,
            x: 1.0,
            y: 2.0,
        })
        .expect("append insert");
        wal.append(&WalEntry::RTreeInsert {
            index_name: INDEX_NAME.to_string(),
            node_id: 99,
            x: 3.0,
            y: 4.0,
        })
        .expect("append insert");
        wal.append(&WalEntry::RTreeDelete {
            index_name: INDEX_NAME.to_string(),
            node_id: 42,
        })
        .expect("append delete");
        wal.flush().expect("fsync WAL");
    }

    let reg = RTreeRegistry::new();
    let mut wal = Wal::new(&wal_path).expect("reopen WAL");
    for e in wal.recover().expect("WAL recover") {
        reg.apply_wal_entry(&e).expect("apply_wal_entry");
    }

    let recovered = collect_visible_ids(&reg, 8);
    assert_eq!(
        recovered,
        vec![99],
        "insert-then-delete pair must converge to post-delete state"
    );
}
