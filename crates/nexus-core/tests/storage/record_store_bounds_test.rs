//! Bounds-hardening regression tests for the record store.
//!
//! (#2) `read_node` / `write_node` / `read_rel` / `write_rel` computed the
//! byte offset as `id as usize * SIZE` and the bound as `offset + SIZE` with
//! unchecked arithmetic. In a release build those wrap: a crafted id makes
//! `offset + SIZE` wrap to `0`, passing the `offset + SIZE > file_size`
//! guard, and the subsequent `mmap[start..end]` slice runs past the mapping
//! and panics. In a debug/test build the overflow-check panics on the
//! multiply instead. Either way an ordinary read of a corrupt/self-referential
//! id (e.g. a bad `dst_id` copied during expand) aborts the thread. Fixed with
//! checked `u64` arithmetic that returns a storage error instead.
//!
//! (#4) `grow_nodes_file` / `grow_rels_file` sized the new file from the
//! CURRENT size only (`max(1.5x, +2MB)`), never the write's target offset, so
//! a single sparse write more than ~2 MB past EOF grew once, was still too
//! small, and then sliced past the freshly-remapped mmap and panicked. Fixed
//! by sizing the grow to at least the target offset.

use nexus_core::storage::{
    NODE_RECORD_SIZE, NodeRecord, REL_RECORD_SIZE, RecordStore, RelationshipRecord,
};
use nexus_core::testing::TestContext;

// ---- #2: overflow-safe offsets -------------------------------------------

#[test]
fn read_node_overflow_id_is_error_not_panic() {
    let ctx = TestContext::new();
    let store = RecordStore::new(ctx.path()).unwrap();

    // `u64::MAX * NODE_RECORD_SIZE` overflows the multiply.
    assert!(
        store.read_node(u64::MAX).is_err(),
        "an overflow-inducing node id must be a storage error, not a panic"
    );

    // The exact audit case: offset = id * 32 = 2^64 - 32 (fits), then
    // offset + 32 wraps to 0 and passes the old `offset + SIZE > file_size`
    // guard. `u64::MAX / 32 == 2^59 - 1`.
    let wrap_id = u64::MAX / NODE_RECORD_SIZE as u64;
    assert!(
        store.read_node(wrap_id).is_err(),
        "the offset+SIZE wrap-to-zero id must be a storage error, not a panic"
    );
}

#[test]
fn read_rel_overflow_id_is_error_not_panic() {
    let ctx = TestContext::new();
    let store = RecordStore::new(ctx.path()).unwrap();

    assert!(store.read_rel(u64::MAX).is_err());
    let wrap_id = u64::MAX / REL_RECORD_SIZE as u64;
    assert!(store.read_rel(wrap_id).is_err());
}

#[test]
fn write_node_overflow_id_is_error_not_panic() {
    let ctx = TestContext::new();
    let mut store = RecordStore::new(ctx.path()).unwrap();
    let rec = NodeRecord::new();
    assert!(
        store.write_node(u64::MAX, &rec).is_err(),
        "an overflow-inducing node id must be a storage error, not a panic"
    );
}

#[test]
fn write_rel_overflow_id_is_error_not_panic() {
    let ctx = TestContext::new();
    let mut store = RecordStore::new(ctx.path()).unwrap();
    let rec = RelationshipRecord::new(1, 2, 3);
    assert!(store.write_rel(u64::MAX, &rec).is_err());
}

// ---- #4: file grow sized to the target offset ----------------------------

#[test]
fn write_node_sparse_offset_beyond_one_grow_succeeds() {
    let ctx = TestContext::new();
    let mut store = RecordStore::new(ctx.path()).unwrap();

    // Initial nodes file is 1 MB; one grow adds max(0.5 MB, 2 MB) -> 3 MB.
    // node id 200_000 -> offset 6.4 MB, well past a single grow, so pre-fix
    // the write sliced past the 3 MB mmap and panicked.
    let id = 200_000u64;
    let mut rec = NodeRecord::new();
    rec.label_bits = 0xABCD;
    store
        .write_node(id, &rec)
        .expect("a sparse node write must grow the file enough and succeed");

    let read = store
        .read_node(id)
        .expect("the sparse node must be readable back");
    assert_eq!(read.label_bits, 0xABCD, "sparse node data must round-trip");
}

#[test]
fn write_rel_sparse_offset_beyond_one_grow_succeeds() {
    let ctx = TestContext::new();
    let mut store = RecordStore::new(ctx.path()).unwrap();

    // rel id 200_000 -> offset 200_000 * 52 = 10.4 MB, past a single grow.
    let id = 200_000u64;
    let rec = RelationshipRecord::new(11, 22, 3);
    store
        .write_rel(id, &rec)
        .expect("a sparse relationship write must grow the file enough and succeed");

    let read = store
        .read_rel(id)
        .expect("the sparse relationship must be readable back");
    // RelationshipRecord is #[repr(C, packed)] — copy fields before asserting.
    let src_id = read.src_id;
    let dst_id = read.dst_id;
    assert_eq!(src_id, 11, "sparse relationship src must round-trip");
    assert_eq!(dst_id, 22, "sparse relationship dst must round-trip");
}
