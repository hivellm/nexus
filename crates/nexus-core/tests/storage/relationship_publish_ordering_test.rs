//! Regression coverage for a relationship-publish-ordering race in
//! `RecordStore::create_relationship`
//! (`crates/nexus-core/src/storage/record_store_ops.rs`).
//!
//! `create_relationship` writes the source node's new `first_rel_ptr`
//! (via `write_node`, publishing the pointer to the newest relationship
//! slot) BEFORE it writes the relationship record itself (via
//! `write_rel`). `RecordStore`'s node and relationship mmaps are
//! independent `Arc<RwLock<MmapMut>>`s, so a lock-free reader that shares
//! those mmaps (e.g. a clone of `RecordStore`, exactly what the server's
//! read path uses) can observe the just-published pointer and then
//! `read_rel` a slot that has not been written yet. That slot is still
//! all-zero: `RelationshipRecord::is_deleted()` reads `flags == 0` as
//! "not deleted", so the reader treats it as a live record with
//! `dst_id == 0` (a phantom edge to node 0) whose `next_src_ptr == 0`
//! looks exactly like the end-of-chain sentinel (silently truncating the
//! adjacency walk).
//!
//! Two tests:
//!
//! 1. `zeroed_relationship_slot_reads_as_live_edge_to_node_zero` pins the
//!    underlying record-format hazard in isolation (no threads): an
//!    allocated-but-unwritten relationship slot reads back as a
//!    convincing "live" end-of-chain edge to node 0. This is a property
//!    of the record format, not of the ordering bug, and is expected to
//!    PASS unconditionally.
//! 2. `concurrent_reads_never_observe_truncated_or_phantom_adjacency_during_create`
//!    is the actual regression guard: it drives concurrent lock-free
//!    readers against a single writer hammering
//!    `Engine::create_relationship` and asserts that no reader ever
//!    observes a phantom `dst_id == 0` edge or an adjacency chain shorter
//!    than the floor established before the race began.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use nexus_core::Engine;
use nexus_core::storage::RecordStore;
use serde_json::json;

// ---------------------------------------------------------------------
// 1. Deterministic hazard: an unwritten slot is indistinguishable from a
//    live end-of-chain edge to node 0.
// ---------------------------------------------------------------------

#[test]
fn zeroed_relationship_slot_reads_as_live_edge_to_node_zero() {
    let mut store = RecordStore::new_temporary().expect("failed to create temporary record store");

    // Reserve a fresh relationship slot WITHOUT writing a record into it --
    // exactly the state a reader can observe between `write_node`
    // (publishing the pointer) and `write_rel` (writing the payload) in
    // `RecordStore::create_relationship`.
    let rel_id = store.allocate_rel_id();

    let rel = store
        .read_rel(rel_id)
        .expect("reading a freshly allocated (but unwritten) relationship slot must not error");

    // `RelationshipRecord` is `#[repr(C, packed)]`, so its fields must be
    // copied to locals before use -- a direct reference to a packed field
    // is unaligned and rejected by the compiler (E0793).
    let src_id = rel.src_id;
    let dst_id = rel.dst_id;
    let next_src_ptr = rel.next_src_ptr;

    assert!(
        !rel.is_deleted(),
        "an unwritten slot's flags are all-zero, so is_deleted() must read false -- \
         this is exactly why the slot looks 'live' to a reader"
    );
    assert_eq!(
        src_id, 0,
        "an unwritten slot's src_id reads as 0, indistinguishable from a real self-loop-ish value"
    );
    assert_eq!(
        dst_id, 0,
        "an unwritten slot's dst_id reads as 0 -- a phantom edge to node 0"
    );
    assert_eq!(
        next_src_ptr, 0,
        "an unwritten slot's next_src_ptr reads as 0 -- indistinguishable from the \
         end-of-chain sentinel, silently truncating an adjacency walk"
    );
}

// ---------------------------------------------------------------------
// 2. Concurrent race: lock-free readers against a live writer.
// ---------------------------------------------------------------------

/// Number of outgoing edges from `a` seeded before the race begins. The
/// concurrent writer only ever appends further edges from `a`, so a
/// correctly-synchronized reader must NEVER observe an adjacency chain
/// shorter than this floor.
const SEEDED_EDGE_COUNT: u64 = 100;

/// Additional edges the writer creates from `a` while readers are racing
/// against it.
const WRITER_ITERATIONS: u64 = 4000;

const READER_THREAD_COUNT: usize = 6;

/// Hard cap on chain-walk steps per reader iteration, purely to turn a
/// hypothetical infinite loop (e.g. a corrupt cyclic chain) into a test
/// failure instead of a hang.
const WALK_ITERATION_CAP: u64 = 1_000_000;

/// Walks the OUTGOING relationship chain from `node_id` (`first_rel_ptr`
/// -> `next_src_ptr`, decoding each pointer as `ptr - 1` per the
/// `rel_id + 1` chain-pointer encoding -- see
/// `RecordStore::create_relationship`). Returns `Ok(count)` with the
/// number of live relationships walked, or `Err(violation message)` the
/// instant a phantom edge (`dst_id == 0`) is observed.
fn walk_outgoing_chain(store: &RecordStore, node_id: u64) -> Result<u64, String> {
    let node = store
        .read_node(node_id)
        .map_err(|e| format!("read_node({node_id}) failed: {e}"))?;

    let mut ptr = node.first_rel_ptr;
    let mut count: u64 = 0;
    let mut steps: u64 = 0;

    while ptr != 0 {
        steps += 1;
        if steps > WALK_ITERATION_CAP {
            return Err(format!(
                "adjacency walk from node {node_id} exceeded the {WALK_ITERATION_CAP} step cap \
                 without reaching end-of-chain -- possible cyclic/corrupt chain"
            ));
        }

        // Chain pointers are stored as rel_id + 1 so that 0 can serve as
        // the end-of-chain sentinel.
        let rel_id = ptr - 1;
        let rel = store
            .read_rel(rel_id)
            .map_err(|e| format!("read_rel({rel_id}) failed: {e}"))?;

        if rel.is_deleted() {
            break;
        }

        // Copy packed fields to locals before use (E0793).
        let dst_id = rel.dst_id;
        let src_id = rel.src_id;
        let next_src_ptr = rel.next_src_ptr;

        if dst_id == 0 {
            return Err(format!(
                "observed a phantom edge: rel_id={rel_id} has dst_id=0 (src_id={src_id}, \
                 next_src_ptr={next_src_ptr}) after {count} previously-walked live \
                 relationships -- this is an allocated-but-not-yet-written relationship \
                 slot being read as a live edge to node 0"
            ));
        }

        count += 1;
        ptr = next_src_ptr;
    }

    Ok(count)
}

#[test]
fn concurrent_reads_never_observe_truncated_or_phantom_adjacency_during_create() {
    let mut engine = Engine::new().expect("engine init");

    let a = engine
        .create_node(vec!["Src".to_string()], json!({}))
        .expect("create source node");

    // Seed a floor of pre-existing outgoing edges from `a` BEFORE any
    // reader starts. The concurrent writer below only ever appends more
    // edges from `a`, so the chain length can only grow from here on.
    for _ in 0..SEEDED_EDGE_COUNT {
        let x = engine
            .create_node(vec!["Dst".to_string()], json!({}))
            .expect("create seed target node");
        engine
            .create_relationship(a, x, "R".to_string(), json!({}))
            .expect("create seed relationship");
    }

    let stop = Arc::new(AtomicBool::new(false));

    let readers: Vec<thread::JoinHandle<Option<String>>> = (0..READER_THREAD_COUNT)
        .map(|_| {
            let store = engine.storage.clone();
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match walk_outgoing_chain(&store, a) {
                        Ok(count) if count < SEEDED_EDGE_COUNT => {
                            return Some(format!(
                                "truncated adjacency chain: walked only {count} relationship(s), \
                                 expected at least the seeded floor of {SEEDED_EDGE_COUNT}"
                            ));
                        }
                        Ok(_) => {
                            // Full, un-truncated walk this iteration -- keep racing.
                        }
                        Err(violation) => return Some(violation),
                    }
                }
                None
            })
        })
        .collect();

    // Writer: keep appending outgoing edges from `a` while readers race
    // against the publish-before-write ordering.
    for _ in 0..WRITER_ITERATIONS {
        let x = engine
            .create_node(vec!["Dst".to_string()], json!({}))
            .expect("create writer target node");
        engine
            .create_relationship(a, x, "R".to_string(), json!({}))
            .expect("create writer relationship");
    }

    stop.store(true, Ordering::Relaxed);

    let violations: Vec<String> = readers
        .into_iter()
        .filter_map(|h| h.join().expect("reader thread panicked"))
        .collect();

    assert!(
        violations.is_empty(),
        "concurrent readers observed relationship-publish-ordering violations: {violations:#?}"
    );
}
