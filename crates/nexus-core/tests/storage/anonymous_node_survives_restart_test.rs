//! Regression coverage for `phase0_fix-anonymous-node-lost-on-restart`.
//!
//! A node with no labels, no properties, and no relationships persists as a
//! byte-for-byte all-zero 32-byte `NodeRecord`. Before the fix, the restart
//! recovery scan in `RecordStore::new` reconstructed `next_node_id` by
//! advancing past any slot with *any* non-zero byte, so an all-zero live
//! node was indistinguishable from an unallocated slot: it was silently
//! dropped on the next clean restart and its id was reused.
//!
//! These tests open a `RecordStore` directly (mirroring the existing
//! `test_persistence` pattern in `storage::record_store::tests`), write
//! records, drop the store, and reopen it at the same path to exercise the
//! real recovery scan.

use nexus_core::storage::{NodeRecord, RecordStore, RelationshipRecord};
use nexus_core::testing::TestContext;
use nexus_core::transaction::TransactionManager;

/// §1.1 — a labelled node plus a trailing anonymous node (no labels, no
/// properties, no relationships) must both survive a clean drop + reopen.
#[test]
fn anonymous_trailing_node_survives_restart() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    {
        let mut store = RecordStore::new(&path).expect("open store");
        let mut tx_mgr = TransactionManager::new().expect("tx manager");
        let mut tx = tx_mgr.begin_write().expect("begin write");

        let foo_id = store
            .create_node(&mut tx, vec!["Foo".to_string()], serde_json::json!({}))
            .expect("create (:Foo)");
        assert_eq!(foo_id, 0);

        // Anonymous node: no labels, no properties, no relationships — this
        // persists as an all-zero 32-byte record.
        let anon_id = store
            .create_node(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
            )
            .expect("create anonymous node");
        assert_eq!(anon_id, 1);

        store.flush().expect("flush before drop");
    } // store dropped — all handles closed.

    let store2 = RecordStore::new(&path).expect("reopen store");

    assert_eq!(
        store2.node_count(),
        2,
        "both the labelled node and the anonymous node must survive restart"
    );

    let foo = store2.read_node(0).expect("read (:Foo)");
    assert!(foo.has_label(0), "labelled node must keep its label");

    let anon = store2.read_node(1).expect("read anonymous node");
    assert!(
        !anon.is_deleted(),
        "surviving anonymous node must not be reported deleted"
    );
    assert_eq!(
        anon.label_bits, 0,
        "anonymous node must still have no labels"
    );
}

/// §1.2 — the extreme case: a store whose ONLY node is anonymous must
/// survive reopen. Before the fix, `next_node_id` reset to 0 and the node
/// vanished entirely.
#[test]
fn store_with_only_an_anonymous_node_survives_restart() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    {
        let mut store = RecordStore::new(&path).expect("open store");
        let mut tx_mgr = TransactionManager::new().expect("tx manager");
        let mut tx = tx_mgr.begin_write().expect("begin write");

        let anon_id = store
            .create_node(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
            )
            .expect("create anonymous node");
        assert_eq!(anon_id, 0);

        store.flush().expect("flush before drop");
    }

    let store2 = RecordStore::new(&path).expect("reopen store");

    assert_eq!(
        store2.node_count(),
        1,
        "an all-anonymous store must not reset next_node_id to 0 on restart"
    );
    let anon = store2
        .read_node(0)
        .expect("read the surviving anonymous node");
    assert!(!anon.is_deleted());
}

/// The surviving anonymous node's id must never be handed out again after
/// restart — otherwise a fresh node would silently overwrite it.
#[test]
fn anonymous_node_id_is_not_reused_after_restart() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    let anon_id;
    {
        let mut store = RecordStore::new(&path).expect("open store");
        let mut tx_mgr = TransactionManager::new().expect("tx manager");
        let mut tx = tx_mgr.begin_write().expect("begin write");

        anon_id = store
            .create_node(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
            )
            .expect("create anonymous node");

        store.flush().expect("flush before drop");
    }

    let mut store2 = RecordStore::new(&path).expect("reopen store");
    let mut tx_mgr2 = TransactionManager::new().expect("tx manager");
    let mut tx2 = tx_mgr2.begin_write().expect("begin write");

    let new_id = store2
        .create_node(&mut tx2, vec!["Bar".to_string()], serde_json::json!({}))
        .expect("create (:Bar) after restart");

    assert_ne!(
        new_id, anon_id,
        "a freshly allocated id must never collide with the surviving anonymous node's id"
    );

    // Both nodes must remain independently readable and correct.
    let anon = store2
        .read_node(anon_id)
        .expect("anonymous node still readable");
    assert_eq!(anon.label_bits, 0);
    let bar = store2.read_node(new_id).expect("new node readable");
    assert!(bar.has_label(0));
}

/// §3.4 migration/back-compat — a store written by the OLD format (a live,
/// NON-anonymous node with `flags == 0`, i.e. no allocated bit) must reopen
/// with all nodes intact. Simulated by writing a raw all-legacy record
/// directly into the `nodes.store` file, bypassing the (fixed) write path
/// entirely — the only way to reproduce genuine pre-fix on-disk bytes.
#[test]
fn old_format_store_with_labelled_node_migrates_losslessly() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    // Initialize the store's on-disk layout (creates nodes.store/rels.store
    // pre-sized and zeroed), then drop it before injecting the legacy bytes.
    drop(RecordStore::new(&path).expect("initialize store layout"));

    let mut legacy_record = NodeRecord::new();
    legacy_record.add_label(3);
    legacy_record.prop_ptr = 0; // no properties, but labelled so non-zero
    assert_eq!(
        legacy_record.flags, 0,
        "sanity: legacy record has flags == 0"
    );

    {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(path.join("nodes.store"))
            .expect("open nodes.store for legacy-record injection");
        file.seek(SeekFrom::Start(0)).expect("seek to node 0");
        file.write_all(bytemuck::bytes_of(&legacy_record))
            .expect("write legacy record bytes");
        file.sync_all().expect("sync injected legacy record");
    }

    let store2 = RecordStore::new(&path).expect("reopen store (runs migration)");

    assert_eq!(
        store2.node_count(),
        1,
        "old-format live node must be recovered by the back-compat scan"
    );
    let recovered = store2.read_node(0).expect("legacy node still readable");
    assert!(!recovered.is_deleted());
    assert!(recovered.has_label(3), "legacy label must be preserved");
}

/// §2.3 — the relationship-store equivalent: a degenerate all-zero self-loop
/// relationship (id 0, type 0, no properties) must survive restart the same
/// way an anonymous node does.
#[test]
fn degenerate_self_loop_relationship_survives_restart() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    {
        let mut store = RecordStore::new(&path).expect("open store");

        // A relationship record with src_id == dst_id == 0, type_id == 0,
        // and no next/prop pointers (all zero) is the relationship analogue
        // of an anonymous node.
        let rel_id = store.allocate_rel_id();
        assert_eq!(rel_id, 0);
        let degenerate = RelationshipRecord {
            src_id: 0,
            dst_id: 0,
            type_id: 0,
            next_src_ptr: 0,
            next_dst_ptr: 0,
            prop_ptr: 0,
            flags: 0,
            reserved: 0,
        };
        store.write_rel(rel_id, &degenerate).expect("write rel");
        store.flush().expect("flush before drop");
    }

    let store2 = RecordStore::new(&path).expect("reopen store");
    assert_eq!(
        store2.relationship_count(),
        1,
        "degenerate all-zero self-loop relationship must survive restart"
    );
    let rel = store2
        .read_rel(0)
        .expect("degenerate relationship readable");
    assert!(!rel.is_deleted());
}

/// Regression: a legacy (pre-fix, `flags == 0`) record that was
/// SOFT-DELETED by the old binary still has non-zero bytes (residual
/// `label_bits`/pointers) even though it is not "live" in the query sense.
/// The id-RESERVATION predicate in the recovery scan must treat it as
/// occupying its slot regardless of the deleted bit — `is_deleted()` is
/// only the query-visibility gate (`get_node`/`get_relationship`), never
/// the reservation gate. A scan whose legacy arm requires `!is_deleted()`
/// would treat this slot as free, letting the next `allocate_node_id()`
/// reuse id 0 and overwrite it — reintroducing the exact id-reuse/data-loss
/// bug this task closes, for a case the ORIGINAL "any non-zero byte" scan
/// already handled correctly.
#[test]
fn legacy_soft_deleted_node_slot_is_not_reused_after_restart() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    // Initialize the store's on-disk layout, then drop it before injecting
    // the legacy bytes.
    drop(RecordStore::new(&path).expect("initialize store layout"));

    // A legacy record: labelled (non-zero label_bits), soft-deleted via
    // FLAG_DELETED (bit 0), but WITHOUT FLAG_ALLOCATED (bit 1) — exactly
    // what the pre-fix `delete_node` would have produced by ORing `1` into
    // a legacy live record's flags.
    let mut legacy_deleted = NodeRecord::new();
    legacy_deleted.add_label(5);
    legacy_deleted.mark_deleted();
    assert_eq!(
        legacy_deleted.flags, 1,
        "sanity: legacy deleted record has only the deleted bit set"
    );
    assert!(!legacy_deleted.is_allocated());

    {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(path.join("nodes.store"))
            .expect("open nodes.store for legacy-record injection");
        file.seek(SeekFrom::Start(0)).expect("seek to node 0");
        file.write_all(bytemuck::bytes_of(&legacy_deleted))
            .expect("write legacy deleted record bytes");
        file.sync_all().expect("sync injected legacy record");
    }

    let mut store2 = RecordStore::new(&path).expect("reopen store (runs migration)");

    // (a) The reserved slot must still be reported as deleted after reopen.
    let reopened = store2.read_node(0).expect("legacy deleted node readable");
    assert!(
        reopened.is_deleted(),
        "migration must preserve the deleted bit"
    );

    // (b) id 0 must remain reserved: a freshly allocated node must NOT
    // reuse it and overwrite the (still-referenced-by-id) deleted slot.
    let mut tx_mgr = TransactionManager::new().expect("tx manager");
    let mut tx = tx_mgr.begin_write().expect("begin write");
    let new_id = store2
        .create_node(&mut tx, vec!["Fresh".to_string()], serde_json::json!({}))
        .expect("create node after restart");
    assert_ne!(
        new_id, 0,
        "id 0 (a legacy soft-deleted slot) must not be reused after restart"
    );

    // The deleted slot must remain unmodified by the new allocation.
    let still_deleted = store2.read_node(0).expect("legacy deleted node readable");
    assert!(still_deleted.is_deleted());
    assert!(still_deleted.has_label(5));
}
