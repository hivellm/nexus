//! Crash-recovery integration test for the R-tree
//! (phase6_rtree-index-core §6.5).
//!
//! The §6 design promises that R-tree mutations land in the WAL
//! and the in-memory tree converges back to the durable shape on
//! recovery. This test exercises the full path:
//!
//! 1. Build an `RTreeRegistry` and journal `N = 5_000`
//!    `WalEntry::RTreeInsert` records into a `FilePageStore`-style
//!    serialised log (we use a plain `Vec<WalEntry>` because the
//!    full WAL framing is exercised separately under
//!    `crates/nexus-core/src/wal/tests`; what this test cares
//!    about is that the registry's `apply_wal_entry` can rebuild
//!    the tree from the log alone).
//! 2. Append a partial bulk-load: a sequence of additional
//!    inserts that were *not* followed by an
//!    `RTreeBulkLoadDone` marker — simulating a crash mid bulk-
//!    rebuild. Recovery must still see every WAL-committed insert,
//!    and the partial-bulk inserts must also surface (they're
//!    individual journal entries; the missing marker only signals
//!    that whatever bulk-load was in progress did not complete).
//! 3. Drop the registry, reconstruct a brand-new one, and feed it
//!    the saved log. Assert every committed point is reachable
//!    through `nearest` and `within_distance`.

use nexus_core::index::rtree::{Metric, RTreeRegistry};
use nexus_core::wal::WalEntry;

fn point_for(i: u64) -> (f64, f64) {
    // Deterministic spread so neighbours don't collapse onto the
    // same coordinate.
    let x = ((i.wrapping_mul(2_654_435_761) >> 16) & 0xff) as f64;
    let y = ((i.wrapping_mul(40_503) >> 8) & 0xff) as f64;
    (x, y)
}

#[test]
fn rtree_recovery_replays_5k_inserts_plus_partial_bulk_load() {
    let index = "Place.loc".to_string();

    // ---- Phase 1: pre-crash, build an in-memory log -----------------
    let mut log: Vec<WalEntry> = Vec::with_capacity(5_500);

    // 5 000 committed inserts ahead of the bulk-load attempt.
    for i in 0..5_000u64 {
        let (x, y) = point_for(i);
        log.push(WalEntry::RTreeInsert {
            index_name: index.clone(),
            node_id: i,
            x,
            y,
        });
    }

    // 500-row partial bulk-load: every insert lands but the
    // closing `RTreeBulkLoadDone` marker is intentionally
    // omitted, mimicking a crash mid-rebuild.
    for i in 5_000..5_500u64 {
        let (x, y) = point_for(i);
        log.push(WalEntry::RTreeInsert {
            index_name: index.clone(),
            node_id: i,
            x,
            y,
        });
    }
    // *** No `RTreeBulkLoadDone` here. ***

    // ---- Phase 2: simulate crash + reopen ---------------------------
    // A fresh registry consumes the saved log. In production the WAL
    // recovery loop drives this; here we feed the entries directly
    // through `apply_wal_entry`.
    let registry = RTreeRegistry::new();
    for entry in &log {
        registry
            .apply_wal_entry(entry)
            .expect("replay must not fail");
    }

    // ---- Phase 3: assert convergence --------------------------------
    let tree = registry.snapshot(&index).expect("index must be registered");
    assert_eq!(
        tree.len(),
        5_500,
        "every WAL-committed insert (including partial-bulk) must be visible"
    );

    // Spot-check the first, middle, and last entries with a
    // tight `within_distance` query against their known points.
    for &i in &[0u64, 2_500, 4_999, 5_499] {
        let (x, y) = point_for(i);
        let ids = tree
            .within_distance(x, y, 0.5, Metric::Cartesian)
            .expect("cartesian search succeeds");
        assert!(
            ids.contains(&i),
            "node {i} expected within 0.5 of its own coords",
        );
    }

    // A wide bbox sweep must surface every committed id.
    let visible = tree.query_bbox(-10.0, -10.0, 1_000.0, 1_000.0);
    assert_eq!(visible.len(), 5_500);
    let mut sorted = visible;
    sorted.sort_unstable();
    assert_eq!(sorted.first().copied(), Some(0));
    assert_eq!(sorted.last().copied(), Some(5_499));
}

#[test]
fn rtree_recovery_handles_bulk_load_done_marker() {
    // A clean shutdown emits `RTreeBulkLoadDone` after the last
    // insert in a bulk-rebuild. Replay should be a no-op for
    // the marker — every preceding insert remains visible, and
    // the marker doesn't blow up the recovery loop.
    let index = "Place.loc".to_string();
    let registry = RTreeRegistry::new();

    for i in 0..32u64 {
        registry
            .apply_wal_entry(&WalEntry::RTreeInsert {
                index_name: index.clone(),
                node_id: i,
                x: f64::from(i as u32),
                y: 0.0,
            })
            .unwrap();
    }
    registry
        .apply_wal_entry(&WalEntry::RTreeBulkLoadDone {
            index_name: index.clone(),
            root_page_id: 1,
        })
        .unwrap();

    let tree = registry.snapshot(&index).unwrap();
    assert_eq!(tree.len(), 32);
    let hits = tree.nearest(0.0, 0.0, 5, Metric::Cartesian).unwrap();
    assert_eq!(hits[0].node_id, 0);
}

#[test]
fn rtree_recovery_processes_interleaved_insert_and_delete() {
    // Realistic WAL traffic interleaves inserts and deletes.
    // Recovery applies them in log order; the final shape
    // matches what the live tree saw before the crash.
    let registry = RTreeRegistry::new();
    let index = "I".to_string();

    let log = vec![
        WalEntry::RTreeInsert {
            index_name: index.clone(),
            node_id: 1,
            x: 0.0,
            y: 0.0,
        },
        WalEntry::RTreeInsert {
            index_name: index.clone(),
            node_id: 2,
            x: 5.0,
            y: 5.0,
        },
        WalEntry::RTreeInsert {
            index_name: index.clone(),
            node_id: 3,
            x: 10.0,
            y: 10.0,
        },
        WalEntry::RTreeDelete {
            index_name: index.clone(),
            node_id: 2,
        },
        WalEntry::RTreeInsert {
            index_name: index.clone(),
            node_id: 4,
            x: 7.0,
            y: 7.0,
        },
    ];
    for entry in &log {
        registry.apply_wal_entry(entry).unwrap();
    }
    let tree = registry.snapshot(&index).unwrap();
    let mut visible = tree.query_bbox(-100.0, -100.0, 100.0, 100.0);
    visible.sort_unstable();
    assert_eq!(visible, vec![1, 3, 4]);
}
