//! Regression: the executor CREATE operator's MATCH/UNWIND-context path
//! must batch node-label counts into `catalog.node_counts`, mirroring the
//! relationship-count batching it already performs.
//!
//! Root cause: `execute_create_with_context` writes nodes directly to the
//! record store via `RecordStore::create_node_with_label_bits[_and_external_id]`
//! (through `create_pattern_node_with_context`), bypassing
//! `Engine::create_node`. Unlike its sibling `execute_create_pattern_internal`
//! (which has always carried a `label_count_updates` accumulator flushed via
//! `Catalog::batch_increment_node_counts`), `execute_create_with_context` had
//! no such accumulator, so `catalog.get_statistics().node_counts` never grew
//! past whatever the very first row happened to leave behind — a lower
//! bound, not the true per-label total, for every node created through an
//! upstream MATCH/UNWIND driving a CREATE.
//!
//! Uses `setup_isolated_test_engine()` (a dedicated LMDB environment per
//! test) rather than the plain `Engine::new()` / `Catalog::new()`
//! constructors: in `cargo test` builds, `Catalog::new` deliberately shares
//! ONE LMDB environment per test-binary *process* (see
//! `catalog::store::Catalog::with_map_size`'s `TEST_CATALOG_DIR`) to avoid
//! exhausting LMDB reader slots across thousands of parallel tests. The
//! record store is still per-`Engine` isolated, but `catalog.node_counts` is
//! not — asserting an exact total against the shared catalog would be
//! order-dependent on whatever else ran earlier in the same process.
//! `setup_isolated_test_engine()` opens its own catalog path and sidesteps
//! that sharing entirely, which is what an exact-count assertion needs.

use nexus_core::Engine;
use nexus_core::testing::setup_isolated_test_engine;

/// Sum every per-label count in the catalog's node-count histogram — the
/// "catalog node total" the task description refers to.
fn total_node_count(engine: &Engine) -> u64 {
    let stats = engine
        .catalog
        .get_statistics()
        .expect("catalog statistics must be readable");
    stats.node_counts.values().sum()
}

/// Drives `execute_create_with_context` (create.rs's MATCH/UNWIND-context
/// CREATE path): `UNWIND range(1, K)` produces K scalar rows with no node
/// variables, so the CREATE operator materialises them as distinct per-row
/// iterations, creating one fresh labelled node per row.
#[test]
fn unwind_driven_create_batches_node_counts_with_context() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("isolated engine");

    engine
        .execute_cypher("UNWIND range(1, 7) AS i CREATE (n:UnwindCreateCountProbe)")
        .expect("UNWIND-driven CREATE must succeed");

    // Ground-truth cross-check against the record store itself (not just
    // the catalog), so a passing assertion below can't be explained away
    // by the catalog and storage disagreeing in the same direction.
    let r = engine
        .execute_cypher("MATCH (n:UnwindCreateCountProbe) RETURN count(n) AS c")
        .expect("count query");
    assert_eq!(
        r.rows[0].values[0].as_i64(),
        Some(7),
        "sanity check: storage itself must contain exactly 7 nodes"
    );

    assert_eq!(
        total_node_count(&engine),
        7,
        "catalog node_counts must equal the 7 nodes written by the \
         UNWIND-driven CREATE path (execute_create_with_context), not a \
         lower-bound count"
    );
}

/// Multi-label nodes must increment every label's counter, not just one —
/// exercising the per-label loop inside the accumulator, not merely its
/// per-node presence.
#[test]
fn unwind_driven_create_batches_node_counts_per_label() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("isolated engine");

    engine
        .execute_cypher(
            "UNWIND range(1, 4) AS i \
             CREATE (n:UnwindMultiLabelProbeA:UnwindMultiLabelProbeB)",
        )
        .expect("UNWIND-driven multi-label CREATE must succeed");

    let stats = engine
        .catalog
        .get_statistics()
        .expect("catalog statistics must be readable");

    let label_a_id = engine
        .catalog
        .get_label_id("UnwindMultiLabelProbeA")
        .expect("label must exist after CREATE");
    let label_b_id = engine
        .catalog
        .get_label_id("UnwindMultiLabelProbeB")
        .expect("label must exist after CREATE");

    assert_eq!(
        stats.node_counts.get(&label_a_id).copied().unwrap_or(0),
        4,
        "first label's catalog count must equal the 4 nodes created, not a \
         lower-bound count"
    );
    assert_eq!(
        stats.node_counts.get(&label_b_id).copied().unwrap_or(0),
        4,
        "second label's catalog count must equal the 4 nodes created, not a \
         lower-bound count"
    );
}
