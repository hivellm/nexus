//! Regression coverage for phase0_fix-fts-async-writer-ordering.
//!
//! `fts_refresh_node` (`engine/crud/index_maintenance.rs:43,70`)
//! implements a SET/REMOVE property update as del-then-add: it calls
//! `FullTextRegistry::remove_entity` and then, if any indexed
//! property still has content, `FullTextRegistry::add_node_document`.
//! When the per-index async writer is enabled
//! (`FullTextRegistry::enable_async_writers`), both calls enqueue
//! [`nexus_core::index::fulltext_writer::WriterCommand`]s onto the
//! same background writer instead of touching Tantivy synchronously.
//!
//! Before the fix, `apply_batch` split a drained batch into "every
//! add" then "every del" and applied the groups in that fixed order
//! regardless of enqueue order — so a Del{id} enqueued *before* an
//! Add{id} in the same batch still ran *after* it, deleting the
//! node's fresh content and leaving the registry's `members`
//! bookkeeping (which is updated synchronously, independent of the
//! writer) claiming the node was still indexed.
//!
//! These tests exercise the registry the way `fts_refresh_node` does
//! — `remove_entity` then `add_node_document` for the same node id,
//! flushed together so both commands land in one async-writer batch
//! — and assert the node's final state matches arrival order.

use nexus_core::index::fulltext_registry::FullTextRegistry;
use tempfile::TempDir;

/// §1/§2 regression: a SET-style refresh (del stale doc, re-add
/// fresh content) must leave the node visible under fulltext search
/// with its *new* content once the async writer flushes, and the
/// registry's `indexes_containing` bookkeeping must agree with the
/// actual Tantivy state.
#[test]
fn set_refresh_preserves_fulltext_visibility_under_async_writer() {
    let tmp = TempDir::new().expect("tempdir");
    let reg = FullTextRegistry::new();
    reg.set_base_dir(tmp.path().to_path_buf());
    reg.create_node_index("docs", &["Doc"], &["body"], Some("standard"))
        .unwrap();
    reg.enable_async_writers();

    // Initial index + commit — the state a node is in before a SET
    // touches its indexed property.
    reg.add_node_document("docs", 1, 0, 0, "original alpha content")
        .unwrap();
    reg.flush_all().expect("flush initial add");
    assert!(
        reg.query("docs", "alpha", None)
            .unwrap()
            .iter()
            .any(|h| h.node_id == 1),
        "initial content must be visible before the refresh"
    );

    // Mirror `fts_refresh_node`'s del-then-add sequence: the stale
    // doc is removed, then the fresh content is re-added. Both
    // commands are enqueued before the explicit flush, so they land
    // in the SAME async-writer batch — the exact scenario
    // `apply_batch` mishandled.
    reg.remove_entity("docs", 1).expect("refresh-remove");
    reg.add_node_document("docs", 1, 0, 0, "updated beta content")
        .expect("refresh-add");
    reg.flush_all().expect("flush refresh batch");

    assert!(
        reg.indexes_containing(1).contains(&"docs".to_string()),
        "registry members must still consider node 1 indexed after the refresh"
    );
    let hits = reg.query("docs", "beta", None).unwrap();
    assert!(
        hits.iter().any(|h| h.node_id == 1),
        "refreshed content must be searchable once the async-writer batch flushes, got {hits:?}"
    );
    let stale = reg.query("docs", "alpha", None).unwrap();
    assert!(
        !stale.iter().any(|h| h.node_id == 1),
        "stale pre-refresh content must not still be indexed, got {stale:?}"
    );

    reg.disable_async_writers().unwrap();
}

/// §2.3: a REMOVE-style refresh (the property that fed the index is
/// cleared, so no follow-up add is enqueued) must still drop the
/// node out of both the registry's bookkeeping and the Tantivy
/// index once the async writer flushes the lone Del.
#[test]
fn remove_refresh_drops_node_from_fulltext_index_under_async_writer() {
    let tmp = TempDir::new().expect("tempdir");
    let reg = FullTextRegistry::new();
    reg.set_base_dir(tmp.path().to_path_buf());
    reg.create_node_index("docs", &["Doc"], &["body"], Some("standard"))
        .unwrap();
    reg.enable_async_writers();

    reg.add_node_document("docs", 2, 0, 0, "gamma searchable text")
        .unwrap();
    reg.flush_all().expect("flush initial add");
    assert!(
        reg.query("docs", "gamma", None)
            .unwrap()
            .iter()
            .any(|h| h.node_id == 2)
    );

    reg.remove_entity("docs", 2).expect("refresh-remove");
    reg.flush_all().expect("flush del-only batch");

    assert!(
        !reg.indexes_containing(2).contains(&"docs".to_string()),
        "registry members must drop node 2 after REMOVE"
    );
    let hits = reg.query("docs", "gamma", None).unwrap();
    assert!(
        !hits.iter().any(|h| h.node_id == 2),
        "removed node must no longer be searchable, got {hits:?}"
    );

    reg.disable_async_writers().unwrap();
}
