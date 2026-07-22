//! Regression coverage for `Engine::process_merge_relationship`
//! (`crates/nexus-core/src/engine/write_exec.rs`): the src/dst endpoints
//! were always resolved in pattern/array order (`elements[0]` ->
//! `elements[2]`) regardless of the parsed `RelationshipDirection`, so
//! `MERGE (a)<-[:T]-(b)` silently wrote (and matched) the edge as a->b
//! instead of b->a. Part of the
//! phase0_fix-create-ignores-arrow-direction audit — see proposal.md
//! "Audit `process_merge_relationship`".

use nexus_core::Engine;
use nexus_core::testing::TestContext;

/// Count relationships of `rel_type` directly connecting the node named
/// `src_name` to the node named `dst_name`, in that direction.
fn count_rel_named(engine: &mut Engine, src_name: &str, rel_type: &str, dst_name: &str) -> u64 {
    let q = format!(
        "MATCH (a {{name: '{src_name}'}})-[r:{rel_type}]->(b {{name: '{dst_name}'}}) \
         RETURN count(r) AS c"
    );
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

/// `MERGE (a)<-[:T]-(b)` must create the edge b->a, never a->b.
#[test]
fn merge_reversed_arrow_creates_edge_in_arrow_direction() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (a:MadPerson {name: 'Alice'})<-[:MadKnows]-(b:MadPerson {name: 'Bob'})",
        )
        .expect("reversed-arrow MERGE must succeed");

    assert_eq!(
        count_rel_named(&mut engine, "Bob", "MadKnows", "Alice"),
        1,
        "edge must be written Bob->Alice per the `<-` arrow"
    );
    assert_eq!(
        count_rel_named(&mut engine, "Alice", "MadKnows", "Bob"),
        0,
        "edge must NOT be written Alice->Bob — that reverses the parsed arrow"
    );
}

/// A reversed-arrow MERGE must find an already-existing, correctly-oriented
/// edge instead of creating a duplicate with the wrong orientation — this
/// exercises the `existing_rel` lookup, not just the create fallback.
#[test]
fn merge_reversed_arrow_matches_existing_reversed_edge_without_duplicating() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // Seed the edge directly in the arrow-correct orientation: Bob -> Alice.
    engine
        .execute_cypher(
            "CREATE (a:MaePerson {name: 'Alice'}), (b:MaePerson {name: 'Bob'}), \
             (b)-[:MaeKnows]->(a)",
        )
        .expect("seed edge");

    // Re-running the same reversed-arrow MERGE must match the existing
    // Bob->Alice edge, not create a second (wrongly-oriented) one.
    engine
        .execute_cypher(
            "MATCH (a:MaePerson {name: 'Alice'}), (b:MaePerson {name: 'Bob'}) \
             MERGE (a)<-[:MaeKnows]-(b)",
        )
        .expect("reversed-arrow MERGE must succeed");

    assert_eq!(
        count_rel_named(&mut engine, "Bob", "MaeKnows", "Alice"),
        1,
        "MERGE must match the existing Bob->Alice edge, not duplicate it"
    );
    assert_eq!(
        count_rel_named(&mut engine, "Alice", "MaeKnows", "Bob"),
        0,
        "MERGE must NOT create a second, wrongly-oriented Alice->Bob edge \
         when the correctly-oriented one already exists"
    );
}
