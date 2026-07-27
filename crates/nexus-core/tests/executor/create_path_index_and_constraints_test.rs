//! Regression suite for `phase0_fix-create-path-index-and-constraints`.
//!
//! Two related CREATE-path defects, both "a CREATE variant skips
//! index/constraint maintenance its sibling variant performs":
//!
//! - **C-5**: a node created via `MATCH…CREATE` was durable and label-indexed
//!   but absent from the engine's typed property B-tree
//!   (`Engine::indexes.property_index`) — the executor CREATE operator maintains
//!   only its cloned label index, and the `MATCH…CREATE` engine branch synced
//!   storage back but never called `index_typed_properties_for_new_nodes`
//!   (unlike the standalone-CREATE branch). A later `MERGE`'s existence check
//!   resolves indexed filters through `property_index.find_exact` before any
//!   fallback scan, so it could not see the node and created a duplicate.
//!
//! - **M-2**: a bare `CREATE` ran only the executor-local `check_constraints`
//!   (catalog UNIQUE / EXISTS), never the engine's extended constraint set
//!   (`NODE KEY` / property-type) nor `index_composite_tuples`. A `NODE KEY`
//!   constraint was therefore silently unenforced and its composite B-tree left
//!   un-backed for every node created through a bare `CREATE`.

use nexus_core::index::PropertyValue;
use nexus_core::testing::setup_isolated_test_engine;

/// C-5 — a node created via `MATCH…CREATE` must land in the engine's typed
/// property B-tree (`Engine::indexes.property_index`, what `find_exact` /
/// `NodeIndexSeek` read), exactly as a standalone `CREATE` does. The executor
/// CREATE operator maintains only its cloned label index; the `MATCH…CREATE`
/// engine branch synced storage back but never called
/// `index_typed_properties_for_new_nodes`, so `find_exact` returned an empty
/// bitmap for the freshly created node.
///
/// Asserted against the raw index (not a user query): ordinary reads have a
/// storage-scan fallback that masks the missing entry, but any consumer that
/// trusts the index as complete — the index-only MERGE existence fast path,
/// `NodeIndexSeek`, occupancy accounting — is silently corrupted by the gap.
#[test]
fn match_create_node_indexed_in_typed_property_btree() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX DDL must succeed");
    engine
        .execute_cypher("CREATE (:Seed)")
        .expect("seed node must be created");
    engine
        .execute_cypher("MATCH (s:Seed) CREATE (n:Person {id: 42})")
        .expect("MATCH…CREATE of the indexed node must succeed");

    let label_id = engine
        .catalog
        .get_or_create_label("Person")
        .expect("resolve the Person label id");
    let key_id = engine
        .catalog
        .get_or_create_key("id")
        .expect("resolve the id key id");
    let hit = engine
        .indexes
        .property_index
        .find_exact(label_id, key_id, PropertyValue::Integer(42))
        .expect("raw typed-index seek");
    assert_eq!(
        hit.len(),
        1,
        "the MATCH…CREATE-created node must be present in the typed property \
         B-tree — a node absent from `property_index` is invisible to \
         `find_exact` / `NodeIndexSeek` and to the index-backed MERGE \
         existence check: {hit:?}"
    );
}

/// C-5 (companion, user-visible) — a `MERGE` after a `MATCH…CREATE` of the same
/// indexed tuple must not create a duplicate. Guards the higher-level contract
/// the index fix underpins.
#[test]
fn match_create_node_is_visible_to_later_merge() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX DDL must succeed");
    engine
        .execute_cypher("CREATE (:Seed)")
        .expect("seed node must be created");
    engine
        .execute_cypher("MATCH (s:Seed) CREATE (n:Person {id: 42})")
        .expect("MATCH…CREATE of the indexed node must succeed");
    engine
        .execute_cypher("MERGE (m:Person {id: 42})")
        .expect("MERGE of the same tuple must succeed");

    let counted = engine
        .execute_cypher("MATCH (p:Person {id: 42}) RETURN count(p)")
        .expect("count query must succeed");
    let count = counted.rows[0].values[0].as_u64().unwrap_or(u64::MAX);
    assert_eq!(
        count, 1,
        "MERGE must find the MATCH…CREATE-created node and not create a duplicate"
    );
}

/// M-2 — a bare `CREATE` that violates a `NODE KEY` constraint must be
/// rejected. Before the fix the second CREATE silently succeeded (the executor
/// never enforced NODE KEY), leaving two nodes with the same key tuple.
#[test]
fn bare_create_enforces_node_key_constraint() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    engine
        .execute_cypher("CREATE (:Person {tenantId: 't1', id: 1})")
        .expect("first bare CREATE of the tuple must succeed");

    let second = engine.execute_cypher("CREATE (:Person {tenantId: 't1', id: 1})");
    assert!(
        second.is_err(),
        "a second bare CREATE of the same NODE KEY tuple must be rejected — \
         the executor CREATE path bypassed the engine's NODE KEY enforcement, \
         so the duplicate silently succeeded: {second:?}"
    );

    let counted = engine
        .execute_cypher("MATCH (p:Person {tenantId: 't1', id: 1}) RETURN count(p)")
        .expect("count query must succeed");
    let count = counted.rows[0].values[0].as_u64().unwrap_or(u64::MAX);
    assert_eq!(
        count, 1,
        "exactly one node must carry the NODE KEY tuple after the rejected \
         duplicate CREATE"
    );
}

// ── Inverse pairings: each CREATE entry point verified against both invariants ──

/// Inverse of C-5 — a bare `CREATE` of an indexed node must be in the typed
/// property index (so a follow-up `MERGE` does not duplicate it). The
/// standalone-CREATE branch already indexed typed properties; this locks that
/// both CREATE forms satisfy the invariant.
#[test]
fn bare_create_indexed_node_visible_to_merge() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX DDL must succeed");
    engine
        .execute_cypher("CREATE (:Person {id: 7})")
        .expect("bare CREATE of the indexed node must succeed");

    let label_id = engine.catalog.get_or_create_label("Person").unwrap();
    let key_id = engine.catalog.get_or_create_key("id").unwrap();
    let hit = engine
        .indexes
        .property_index
        .find_exact(label_id, key_id, PropertyValue::Integer(7))
        .expect("raw typed-index seek");
    assert_eq!(hit.len(), 1, "bare CREATE must index the node: {hit:?}");

    engine
        .execute_cypher("MERGE (m:Person {id: 7})")
        .expect("MERGE must succeed");
    let counted = engine
        .execute_cypher("MATCH (p:Person {id: 7}) RETURN count(p)")
        .expect("count query must succeed");
    assert_eq!(
        counted.rows[0].values[0].as_u64(),
        Some(1),
        "MERGE must find the bare-CREATE-created node, not duplicate it"
    );
}

/// Inverse of M-2 — a `MATCH…CREATE` that violates a `NODE KEY` constraint must
/// be rejected too, not only a bare `CREATE`. Both CREATE entry points must
/// reach the engine's extended-constraint enforcement.
#[test]
fn match_create_enforces_node_key_constraint() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");
    engine
        .execute_cypher("CREATE (:Person {tenantId: 't1', id: 1})")
        .expect("first bare CREATE of the tuple must succeed");
    engine
        .execute_cypher("CREATE (:Seed)")
        .expect("seed node for the MATCH driver must be created");

    let violating =
        engine.execute_cypher("MATCH (s:Seed) CREATE (n:Person {tenantId: 't1', id: 1})");
    assert!(
        violating.is_err(),
        "a MATCH…CREATE of an already-present NODE KEY tuple must be rejected: {violating:?}"
    );

    let counted = engine
        .execute_cypher("MATCH (p:Person {tenantId: 't1', id: 1}) RETURN count(p)")
        .expect("count query must succeed");
    assert_eq!(
        counted.rows[0].values[0].as_u64(),
        Some(1),
        "only the original node may carry the NODE KEY tuple"
    );
}

/// Rollback atomicity — a single CREATE statement that produces two nodes with
/// the same NODE KEY tuple must be rejected AND leave nothing behind (Neo4j
/// rejects the whole write, never a partial one). Exercises same-statement
/// enforcement (the first node is indexed before the second is checked) and the
/// full-statement rollback path.
#[test]
fn create_statement_violating_node_key_is_fully_rolled_back() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    let res = engine.execute_cypher(
        "CREATE (:Person {tenantId: 't1', id: 1}), (:Person {tenantId: 't1', id: 1})",
    );
    assert!(
        res.is_err(),
        "a single CREATE producing a duplicate NODE KEY tuple must be rejected: {res:?}"
    );

    let counted = engine
        .execute_cypher("MATCH (p:Person) RETURN count(p)")
        .expect("count query must succeed");
    assert_eq!(
        counted.rows[0].values[0].as_u64(),
        Some(0),
        "the rejected CREATE statement must be fully rolled back — neither node \
         may survive"
    );
}
