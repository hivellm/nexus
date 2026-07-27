//! Regression tests: a `RecordStore` clone's bound-check must track the
//! shared mmap it indexes, not a per-clone cached `nodes_file_size` /
//! `rels_file_size` snapshot.
//!
//! `RecordStore` shares its mmaps across clones via `Arc<RwLock<MmapMut>>`
//! (a fresh clone is taken on every `refresh_executor`) but historically
//! cached the file sizes it bound-checks against as plain per-instance
//! `usize` fields copied by value on `Clone`. The two states could diverge:
//!
//! - **grow** replaces the shared mmap but updates only the growing clone's
//!   own size, so a stale clone under-reported capacity and returned a
//!   spurious `NotFound` for a node that physically exists in the grown mmap;
//! - **clear_all** shrinks the shared mmap but likewise updates only its own
//!   size, so a stale clone over-reported capacity, passed the bound check,
//!   and sliced past the now-smaller mmap — an out-of-bounds panic.
//!
//! The fix bound-checks reads against the live mapping length (`guard.len()`)
//! under the same lock used to copy the record.

use nexus_core::storage::{NodeRecord, RecordStore};
use nexus_core::testing::TestContext;
use std::thread;

/// grow divergence: a clone taken before a grow must still read a node the
/// original wrote into the grown region of the shared mmap.
#[test]
fn stale_clone_reads_node_in_grown_region() {
    let ctx = TestContext::new();
    let mut original = RecordStore::new(ctx.path()).unwrap();

    // Clone BEFORE the grow — captures the stale (initial) file-size snapshot.
    let clone = original.clone();

    // Grow the shared mmap via the original, far past the clone's cached size
    // (node 100_000 -> offset 3.2 MB, well beyond the 1 MB initial file).
    let id = 100_000u64;
    let mut rec = NodeRecord::new();
    rec.label_bits = 0x1234;
    original.write_node(id, &rec).unwrap();

    // The clone shares the now-grown mmap but holds the stale-small size.
    let read = clone.read_node(id);
    assert!(
        read.is_ok(),
        "a stale clone must read a node that exists in the grown shared mmap, \
         not spuriously return NotFound: {read:?}"
    );
    assert_eq!(read.unwrap().label_bits, 0x1234);
}

/// clear_all divergence: a clone taken before `clear_all` shrinks the shared
/// mmap holds a stale-LARGE cached size. Reading a high offset must give the
/// same answer as the freshly-sized original — both bound-check the same
/// shared mmap — and must never slice past it (OOB panic). Asserting agreement
/// (rather than a specific panic) is robust to `clear_all`'s exact resulting
/// file length, which depends on the OS file cursor: the divergence is that
/// pre-fix the clone trusts its stale 3.2 MB size while the original uses the
/// reset 1 MB size, so the two disagree; the fix makes both consult the live
/// mapping length so they agree.
#[test]
fn stale_clone_agrees_with_original_after_clear_all() {
    let ctx = TestContext::new();
    let mut original = RecordStore::new(ctx.path()).unwrap();

    // Grow the shared mmap past its initial size.
    original.write_node(100_000, &NodeRecord::new()).unwrap();

    // Clone now — captures the stale-LARGE size (~3.2 MB).
    let clone = original.clone();

    // Shrink the shared mmap back to the initial size via the original.
    original.clear_all().unwrap();

    // Offset 50_000 * 32 = 1.6 MB: within the clone's stale-large cached size
    // but past the original's reset size. Pre-fix the clone's stale size lets
    // the bound check pass (Ok/garbage or an OOB panic) while the original
    // correctly rejects it — they disagree. Post-fix both consult the live
    // shared-mmap length and agree. A pre-fix OOB panic here also fails the
    // test (a panic is a test failure), so either divergence outcome is red.
    let clone_read = clone.read_node(50_000);
    let original_read = original.read_node(50_000);
    assert_eq!(
        clone_read.is_ok(),
        original_read.is_ok(),
        "a stale clone and the original must agree on readability after \
         clear_all (both bound-check the same shared mmap): \
         clone={clone_read:?}, original={original_read:?}"
    );
}

/// Concurrency shape (§4.2): readers on a stale clone hammering `read_node`
/// while the original keeps growing the shared mmap must never panic, and a
/// node written before the clone must stay readable throughout.
#[test]
fn concurrent_reads_during_grow_never_panic() {
    let ctx = TestContext::new();
    let mut original = RecordStore::new(ctx.path()).unwrap();

    let target = 80_000u64;
    let mut rec = NodeRecord::new();
    rec.label_bits = 0xBEEF;
    original.write_node(target, &rec).unwrap();

    let clone = original.clone();
    let reader = thread::spawn(move || {
        for _ in 0..5_000 {
            let r = clone
                .read_node(target)
                .expect("target node must remain readable during concurrent grow");
            assert_eq!(r.label_bits, 0xBEEF);
        }
    });

    // Keep growing the shared mmap under the reader.
    for i in 1..40u64 {
        original
            .write_node(target + i * 4_000, &NodeRecord::new())
            .unwrap();
    }

    reader
        .join()
        .expect("reader thread must not panic during concurrent grow");
}

/// Concurrency shape (§4.2): readers on a stale clone must not panic when the
/// original shrinks the shared mmap via `clear_all` underneath them.
#[test]
fn concurrent_reads_during_clear_all_never_panic() {
    let ctx = TestContext::new();
    let mut original = RecordStore::new(ctx.path()).unwrap();
    original.write_node(100_000, &NodeRecord::new()).unwrap();

    let clone = original.clone();
    let reader = thread::spawn(move || {
        for _ in 0..5_000 {
            // Whatever the current mmap state, this must never panic; Ok or
            // NotFound both acceptable depending on timing.
            let _ = clone.read_node(50_000);
        }
    });

    original.clear_all().unwrap();

    reader
        .join()
        .expect("reader thread must not panic during concurrent clear_all");
}
