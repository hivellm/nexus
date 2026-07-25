//! Regression: the executor CREATE operator must batch relationship-type
//! counts into `catalog.rel_counts`, mirroring the node-count batching it
//! already performs.
//!
//! Root cause: both CREATE code paths in
//! `executor::operators::create` write relationships directly to the
//! record store via `RecordStore::create_relationship`, bypassing
//! `Engine::create_relationship` (the only place that used to call
//! `Catalog::increment_rel_count`). Node counts were already batched via
//! `label_count_updates` + `Catalog::batch_increment_node_counts`; the
//! relationship-count twin (`rel_count_updates` +
//! `Catalog::batch_increment_rel_counts`) was missing, so
//! `catalog.get_statistics().rel_counts` stayed at (or converged to) zero
//! for every edge created through Cypher CREATE, regardless of how many
//! edges were actually written to storage.
//!
//! Two independent CREATE code paths exist in create.rs, each with its own
//! accumulator, so both are exercised here:
//! - `execute_create_pattern_internal` — the standalone-CREATE fast path
//!   (no preceding MATCH/UNWIND; `operators.first()` is the Create itself).
//! - `execute_create_with_context` — driven by upstream rows (MATCH,
//!   UNWIND, WITH, ...); each row runs one CREATE iteration.
//!
//! These tests use `setup_isolated_test_engine()` (a dedicated LMDB
//! environment per test) rather than the plain `Engine::new()` /
//! `Catalog::new()` constructors: in `cargo test` builds, `Catalog::new`
//! deliberately shares ONE LMDB environment per test-binary *process* (see
//! `catalog::store::Catalog::with_map_size`'s `TEST_CATALOG_DIR`) to avoid
//! exhausting LMDB reader slots across thousands of parallel tests. The
//! record store is still per-`Engine` isolated, but `catalog.rel_counts`
//! is not — asserting an exact total against the shared catalog would be
//! order-dependent on whatever else ran earlier in the same process.
//! `setup_isolated_test_engine()` opens its own catalog path and sidesteps
//! that sharing entirely, which is what an exact-count assertion needs.

use nexus_core::Engine;
use nexus_core::testing::setup_isolated_test_engine;

/// Sum every per-type count in the catalog's relationship-count histogram —
/// the "catalog relationship total" the task description refers to.
fn total_rel_count(engine: &Engine) -> u64 {
    let stats = engine
        .catalog
        .get_statistics()
        .expect("catalog statistics must be readable");
    stats.rel_counts.values().sum()
}

/// Drives `execute_create_pattern_internal` (create.rs's standalone-CREATE
/// path, edge-create site at the `store_mut().create_relationship(...)` call
/// inside the single-pattern loop): a single `CREATE` clause chaining five
/// relationships into one pattern, with no MATCH/UNWIND ahead of it, so the
/// executor dispatches it via the op_idx == 0 standalone fast path.
#[test]
fn standalone_chained_create_batches_relationship_counts() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("isolated engine");

    // One CREATE clause, one pattern, five chained :KNOWS edges (six
    // anonymous nodes) — all created within a single transaction and a
    // single `rel_count_updates` accumulator flush.
    engine
        .execute_cypher(
            "CREATE ()-[:KNOWS]->()-[:KNOWS]->()-[:KNOWS]->()-[:KNOWS]->()-[:KNOWS]->()",
        )
        .expect("standalone chained CREATE must succeed");

    assert_eq!(
        total_rel_count(&engine),
        5,
        "catalog rel_counts must equal the 5 edges written by the \
         standalone CREATE path (execute_create_pattern_internal), not a \
         lower-bound/zero count"
    );
}

/// Drives `execute_create_with_context` (create.rs's MATCH/UNWIND-context
/// CREATE path, the other edge-create site): `UNWIND` produces five scalar
/// rows with no node variables, so the CREATE operator materialises them as
/// distinct per-row iterations and calls `execute_create_with_context`,
/// which creates one fresh `(:X)-[:KNOWS]->(:Y)` edge per row.
#[test]
fn unwind_driven_create_batches_relationship_counts_with_context() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("isolated engine");

    engine
        .execute_cypher("UNWIND range(1, 5) AS i CREATE (:X)-[:KNOWS]->(:Y)")
        .expect("UNWIND-driven CREATE must succeed");

    // Ground-truth cross-check against the record store itself (not just
    // the catalog), so a passing assertion below can't be explained away
    // by the catalog and storage disagreeing in the same direction.
    let r = engine
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN count(r) AS c")
        .expect("count query");
    assert_eq!(
        r.rows[0].values[0].as_i64(),
        Some(5),
        "sanity check: storage itself must contain exactly 5 edges"
    );

    assert_eq!(
        total_rel_count(&engine),
        5,
        "catalog rel_counts must equal the 5 edges written by the \
         UNWIND-driven CREATE path (execute_create_with_context), not a \
         lower-bound/zero count"
    );
}

/// Both accumulators are per-call, not global — creating relationships
/// through both code paths in sequence on the same engine must add up
/// rather than clobber each other's flush. The standalone pattern chains
/// three `:KNOWS` edges (four nodes); the UNWIND pass adds three more.
#[test]
fn both_create_paths_accumulate_into_the_same_catalog_total() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("isolated engine");

    engine
        .execute_cypher("CREATE ()-[:KNOWS]->()-[:KNOWS]->()-[:KNOWS]->()")
        .expect("standalone CREATE must succeed"); // +3 edges

    engine
        .execute_cypher("UNWIND range(1, 3) AS i CREATE (:X)-[:KNOWS]->(:Y)")
        .expect("UNWIND-driven CREATE must succeed"); // +3 edges

    assert_eq!(
        total_rel_count(&engine),
        6,
        "relationship counts from the standalone and UNWIND-context CREATE \
         paths must both land in the catalog and sum correctly"
    );
}
