//! Regression coverage for `phase0_fix-property-store-shrink-corruption`.
//!
//! `PropertyStore::update_properties`'s in-place branch used to handle a
//! SHRINK (`new_data_size <= existing_data_size`) by overwriting the
//! header's `data_size` and leading bytes without zeroing the freed tail of
//! the old, longer payload. On reopen, `rebuild_index`/
//! `ensure_index_populated` strode by the (now smaller) stored `data_size`,
//! landed inside that stale tail instead of at the next entity's true
//! header, read a garbage `EntityType` byte there, and broke early —
//! dropping every later entity from the in-memory index and leaving
//! `next_offset` pointed mid-file, which the next property-store write then
//! silently overwrote.
//!
//! The fix (§2.1, option b — grow-only) makes `update_properties` allocate
//! fresh space at `next_offset` for anything that isn't an identical-size
//! rewrite, so `data_size` on disk always equals an entry's true physical
//! footprint. The two rebuild scanners are additionally hardened to RESYNC
//! (scan forward for the next parseable header) instead of breaking, so an
//! already-damaged OLD-format store degrades gracefully on reopen too.
//!
//! §1.1/§1.2 assert `RecordStore::property_count()` (a direct proxy for the
//! property store's in-memory `index`/`reverse_index`, `crates/nexus-core/
//! src/storage/record_store_ops.rs:1374`) because `load_node_properties`
//! reads via the node record's own `prop_ptr` first — a raw-offset read
//! that stays correct across a reopen even when the *index* is corrupted,
//! independent of this bug. `property_count()` is the assertion that
//! actually distinguishes the pre-fix rebuild-scan corruption from a
//! correct one. §1.3 demonstrates the real, unmasked data-loss symptom: a
//! write that follows a corrupted reopen and allocates from the wrong
//! `next_offset` really does clobber a still-live neighbor's on-disk bytes.

use nexus_core::error::Result;
use nexus_core::storage::RecordStore;
use nexus_core::storage::property_store::{EntityType, PropertyStore};
use nexus_core::testing::TestContext;
use nexus_core::transaction::TransactionManager;
use std::io::{Seek, SeekFrom, Write};

fn create_test_store() -> (RecordStore, TestContext, std::path::PathBuf) {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();
    let store = RecordStore::new(&path).unwrap();
    (store, ctx, path)
}

/// §1.1 — two nodes back to back (the second physically laid out right
/// after the first's property blob); `SET` the first node's property to a
/// strictly shorter value (the in-place shrink branch); close and reopen;
/// the second node's properties must still be correct and, critically, the
/// property store's rebuilt index must still contain BOTH entries.
#[test]
fn shrink_in_place_preserves_next_entity_on_reopen() -> Result<()> {
    let (mut store, _ctx, path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;

    let mut tx = tx_mgr.begin_write()?;
    // Node A: a long property value, so the later SET below is a strict
    // shrink (triggers `update_properties`'s in-place branch pre-fix).
    let a_id = store.create_node_with_label_bits(
        &mut tx,
        1,
        serde_json::json!({ "bio": "Alice ".repeat(60) }),
    )?;
    // Node B: created immediately after A, so its property blob sits right
    // after A's on disk.
    let b_id = store.create_node_with_label_bits(
        &mut tx,
        1,
        serde_json::json!({ "name": "Bob", "city": "Springfield" }),
    )?;
    tx_mgr.commit(&mut tx)?;

    let b_props_before = store
        .load_node_properties(b_id)?
        .expect("b has properties before the shrink");

    store.update_node_properties(a_id, serde_json::json!({ "bio": "Al" }))?;
    store.flush()?;
    drop(store);

    let store2 = RecordStore::new(&path)?;

    // §2.1's grow-only trade-off: the shrink allocates A's new (short)
    // payload at fresh space instead of reusing its old slot in place, so
    // A's original blob is left behind as harmless, unreclaimed dead space
    // (the proposal's documented, accepted trade-off — a future compaction
    // pass, out of scope here). The rebuild scan re-indexes that dead slot
    // too (it's still a well-formed entry), so the expected count is
    // A-dead + A-fresh + B = 3, not 2. What actually regressed pre-fix is
    // B being dropped entirely — pre-fix this assertion reads back 1 (only
    // A, reused in place with no dead slot, since the corrupted scan
    // breaks immediately after A and never reaches B at all).
    assert_eq!(
        store2.property_count(),
        3,
        "A's dead pre-shrink slot, A's fresh post-shrink slot, and B's entry must all survive \
         the rebuild scan after reopen; pre-fix, A's in-place shrink leaves a stale on-disk \
         tail that strides the scan into garbage and drops B from the index entirely"
    );

    let b_props_after = store2
        .load_node_properties(b_id)?
        .expect("b's properties must still be readable after reopen");
    assert_eq!(
        b_props_after, b_props_before,
        "node B's properties must be unchanged by A's shrink"
    );

    Ok(())
}

/// §1.2 — three or more entities after the shrunk one must ALL survive
/// reopen, not just the immediate neighbor (the pre-fix scan break drops
/// every entity from the failure point onward, not just one).
#[test]
fn shrink_in_place_preserves_multiple_trailing_entities_on_reopen() -> Result<()> {
    let (mut store, _ctx, path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;
    let mut tx = tx_mgr.begin_write()?;

    let a_id = store.create_node_with_label_bits(
        &mut tx,
        1,
        serde_json::json!({ "bio": "Alice ".repeat(80) }),
    )?;
    let b_id =
        store.create_node_with_label_bits(&mut tx, 1, serde_json::json!({ "name": "Bob" }))?;
    let c_id =
        store.create_node_with_label_bits(&mut tx, 1, serde_json::json!({ "name": "Carol" }))?;
    let d_id =
        store.create_node_with_label_bits(&mut tx, 1, serde_json::json!({ "name": "Dave" }))?;
    tx_mgr.commit(&mut tx)?;

    let expected = vec![
        (
            b_id,
            store
                .load_node_properties(b_id)?
                .expect("b props before shrink"),
        ),
        (
            c_id,
            store
                .load_node_properties(c_id)?
                .expect("c props before shrink"),
        ),
        (
            d_id,
            store
                .load_node_properties(d_id)?
                .expect("d props before shrink"),
        ),
    ];

    store.update_node_properties(a_id, serde_json::json!({ "bio": "Al" }))?;
    store.flush()?;
    drop(store);

    let store2 = RecordStore::new(&path)?;

    assert_eq!(
        store2.node_count(),
        4,
        "all four node records must survive restart"
    );
    // Same dead-slot accounting as §1.1: A-dead + A-fresh + B + C + D = 5.
    assert_eq!(
        store2.property_count(),
        5,
        "A's dead pre-shrink slot, A's fresh post-shrink slot, and all three trailing \
         entities must survive the rebuild scan after reopen"
    );

    for (id, expected_props) in expected {
        let actual = store2
            .load_node_properties(id)?
            .unwrap_or_else(|| panic!("node {id} properties missing after reopen"));
        assert_eq!(
            actual, expected_props,
            "node {id} properties diverged after reopen"
        );
    }

    Ok(())
}

/// §1.3 — continuing to write after a corrupted reopen (a `CREATE` that
/// allocates from `next_offset`) must NOT silently overwrite a still-live
/// entity's blob. This is the unmasked corruption symptom: unlike a pure
/// read (§1.1/§1.2), a subsequent property-store write physically clobbers
/// whatever bytes sit at a wrong `next_offset`.
#[test]
fn continued_write_after_reopen_does_not_overwrite_live_entity() -> Result<()> {
    let (mut store, _ctx, path) = create_test_store();
    let mut tx_mgr = TransactionManager::new()?;
    let mut tx = tx_mgr.begin_write()?;

    let a_id = store.create_node_with_label_bits(
        &mut tx,
        1,
        serde_json::json!({ "bio": "Alice ".repeat(60) }),
    )?;
    let b_id = store.create_node_with_label_bits(
        &mut tx,
        1,
        serde_json::json!({ "name": "Bob", "role": "engineer" }),
    )?;
    tx_mgr.commit(&mut tx)?;

    let b_props_before = store
        .load_node_properties(b_id)?
        .expect("b props before shrink");

    store.update_node_properties(a_id, serde_json::json!({ "bio": "Al" }))?;
    store.flush()?;
    drop(store);

    let mut store2 = RecordStore::new(&path)?;
    let mut tx_mgr2 = TransactionManager::new()?;
    let mut tx2 = tx_mgr2.begin_write()?;

    // A write that allocates fresh property-store space from `next_offset`.
    // Pre-fix, the reopen scan's premature break leaves `next_offset`
    // pointed inside A's stale tail — inside or before B's still-live blob
    // — so this allocation would overwrite B's on-disk bytes.
    let c_id = store2.create_node_with_label_bits(
        &mut tx2,
        1,
        serde_json::json!({ "name": "Carol", "role": "designer" }),
    )?;
    tx_mgr2.commit(&mut tx2)?;

    let b_props_after = store2
        .load_node_properties(b_id)?
        .expect("b's properties must still be present after the continued write");
    assert_eq!(
        b_props_after, b_props_before,
        "node B's properties must not be corrupted by a write that happens after a reopen \
         following A's shrink"
    );

    let c_props = store2
        .load_node_properties(c_id)?
        .expect("c's properties must be readable");
    assert_eq!(c_props["name"], "Carol");

    Ok(())
}

/// §3.4 back-compat — a store written by the OLD (pre-fix) code may already
/// contain a shrunk-in-place entry with an unzeroed, stale tail. The
/// hardened scanner must RESYNC past it and recover the later entities,
/// not drop them. Simulated by writing raw bytes directly into
/// `properties.store` — the only way to reproduce genuine pre-fix on-disk
/// bytes, since the (fixed) write path can no longer produce them.
#[test]
fn old_format_stale_tail_entry_recovers_via_resync() -> Result<()> {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    let a_original = serde_json::json!({ "bio": "Alice Alice Alice Alice Alice" });
    let a_shrunk = serde_json::json!({ "bio": "Al" });
    let b_props = serde_json::json!({ "name": "Bob", "city": "Springfield" });
    let c_props = serde_json::json!({ "name": "Carol", "age": 42 });

    let a_offset;
    let a_original_len;
    let a_shrunk_bytes = serde_json::to_vec(&a_shrunk).unwrap();

    {
        let mut store = PropertyStore::new(path.clone())?;
        a_offset = store.store_properties(100, EntityType::Node, a_original.clone())?;
        store.store_properties(200, EntityType::Node, b_props.clone())?;
        store.store_properties(300, EntityType::Node, c_props.clone())?;
        store.flush()?;
        a_original_len = serde_json::to_vec(&a_original).unwrap().len() as u64;
    } // store dropped here — releases the mmap so the file can be rewritten directly.

    assert!(
        (a_shrunk_bytes.len() as u64) < a_original_len,
        "sanity: the injected shrink must be strictly smaller than the original payload"
    );

    // Simulate the OLD (pre-fix) in-place shrink: overwrite the header's
    // `data_size` and A's leading bytes, but leave the freed tail of the
    // original, longer payload untouched — exactly what a pre-fix binary
    // left on disk (`property_store.rs`, old `update_properties`:307-312).
    {
        let property_file = path.join("properties.store");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&property_file)?;

        // Header layout: entity_id (8 bytes) + entity_type (1 byte) +
        // data_size (4 bytes, little-endian) starting at `a_offset + 9`.
        file.seek(SeekFrom::Start(a_offset + 9))?;
        file.write_all(&(a_shrunk_bytes.len() as u32).to_le_bytes())?;

        // Overwrite only the leading bytes of A's payload; the remainder
        // (from `a_offset + 13 + shrunk_len` to `a_offset + 13 +
        // original_len`) is left as stale, unzeroed tail bytes — this is
        // what strides the pre-fix scan into garbage.
        file.seek(SeekFrom::Start(a_offset + 13))?;
        file.write_all(&a_shrunk_bytes)?;
        file.sync_all()?;
    }

    // Reopen — runs the full rebuild scan, which must resync past the
    // stale tail instead of dropping B and C.
    let store2 = PropertyStore::new(path)?;

    assert_eq!(
        store2.property_count(),
        3,
        "A (shrunk), B, and C must all be recovered by the hardened resync scan"
    );

    let b_recovered = store2
        .load_properties(200, EntityType::Node)?
        .expect("B must be recovered via resync");
    assert_eq!(b_recovered, b_props);

    let c_recovered = store2
        .load_properties(300, EntityType::Node)?
        .expect("C must be recovered via resync");
    assert_eq!(c_recovered, c_props);

    Ok(())
}
