//! Coverage for `ResultSet::side_effects` — the mutation counters the
//! openCypher TCK asserts on via `And the side effects should be:` /
//! `And no side effects` (see `crates/nexus-core/tests/tck/opencypher/`).
//!
//! Every test uses an isolated per-test catalog via
//! `Engine::with_isolated_catalog` + `testing::TestContext`, matching the
//! pattern in `cypher_external_id_write_paths.rs`.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

#[test]
fn create_node_reports_one_node_created_and_nothing_else() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let result = engine
        .execute_cypher("CREATE (n:X)")
        .expect("CREATE must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.nodes_created, 1, "exactly one node was created");
    assert_eq!(effects.nodes_deleted, 0);
    assert_eq!(effects.relationships_created, 0);
    assert_eq!(effects.relationships_deleted, 0);
    assert_eq!(effects.properties_set, 0);
    assert_eq!(effects.properties_removed, 0);
    assert_eq!(effects.labels_added, 0);
    assert_eq!(effects.labels_removed, 0);
}

#[test]
fn read_only_match_reports_no_side_effects() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:X)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n) RETURN n")
        .expect("MATCH must succeed");

    assert_eq!(
        result.side_effects,
        nexus_core::executor::types::SideEffects::default(),
        "a read-only query must report an all-zero SideEffects, got {:?}",
        result.side_effects
    );
}

#[test]
fn merge_onto_existing_node_reports_no_creation() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let first = engine
        .execute_cypher("MERGE (n:W {k: 1})")
        .expect("first MERGE must succeed");
    assert_eq!(first.side_effects.nodes_created, 1, "first MERGE creates");

    // Second MERGE matches the existing node -- nothing is created.
    let second = engine
        .execute_cypher("MERGE (n:W {k: 1})")
        .expect("second MERGE must succeed");
    assert_eq!(
        second.side_effects.nodes_created, 0,
        "MERGE onto an existing node must not report a creation"
    );

    let count = engine
        .execute_cypher("MATCH (n:W) RETURN count(n)")
        .expect("count must succeed");
    assert_eq!(
        count.rows[0].values[0].as_i64(),
        Some(1),
        "only one node should exist"
    );
}

#[test]
fn external_id_conflict_policy_match_reports_no_creation() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let first = engine
        .execute_cypher("CREATE (n:E {_id: 'str:dup'})")
        .expect("first create must succeed");
    assert_eq!(first.side_effects.nodes_created, 1);

    // ON CONFLICT MATCH resolves to the existing node: no record is written,
    // so this must not be counted as a creation.
    let second = engine
        .execute_cypher("CREATE (n:E {_id: 'str:dup'}) ON CONFLICT MATCH")
        .expect("ON CONFLICT MATCH must succeed, not error");
    assert_eq!(
        second.side_effects.nodes_created, 0,
        "ON CONFLICT MATCH resolved to an existing node; nothing was created"
    );
}
