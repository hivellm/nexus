//! Regression coverage for CREATE's relationship arrow direction
//! (`crates/nexus-core/src/executor/operators/create.rs`): both CREATE
//! paths — standalone `execute_create_pattern_internal` and MATCH...CREATE's
//! `execute_create_with_context` — wrote every relationship edge in
//! pattern/array order (the node to the LEFT of the relationship element ->
//! the node to the RIGHT) regardless of the parsed `RelationshipDirection`,
//! so `CREATE (x)<-[:T]-(y)` silently stored the edge as x->y instead of
//! y->x. See
//! `.rulebook/tasks/phase0_fix-create-ignores-arrow-direction/proposal.md`.

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

/// Standalone `CREATE (a)<-[:T]-(b)` must write the edge b->a (the arrow
/// points INTO `a`), never a->b.
#[test]
fn standalone_create_reversed_arrow_stores_edge_in_arrow_direction() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (a:CadPerson {name: 'Alice'})<-[:CadKnows]-(b:CadPerson {name: 'Bob'})",
        )
        .expect("reversed-arrow CREATE must succeed");

    assert_eq!(
        count_rel_named(&mut engine, "Bob", "CadKnows", "Alice"),
        1,
        "edge must be written Bob->Alice per the `<-` arrow"
    );
    assert_eq!(
        count_rel_named(&mut engine, "Alice", "CadKnows", "Bob"),
        0,
        "edge must NOT be written Alice->Bob — that reverses the parsed arrow"
    );
}

/// `CREATE (a:A)-[:T]->(b:B)` and `CREATE (b:B)<-[:T]-(a:A)` must produce
/// identically-oriented edges: source is always the node the arrow leaves
/// FROM, target the node it points TO — regardless of which variable is
/// written first in the pattern text.
#[test]
fn standalone_create_forward_and_reversed_arrows_produce_identical_edges() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (a:CbfPerson {name: 'Alice'})-[:CbfKnows]->(b:CbfPerson {name: 'Bob'})",
        )
        .expect("forward-arrow CREATE must succeed");

    engine
        .execute_cypher(
            "CREATE (b:CbfPerson {name: 'Dan'})<-[:CbfKnows]-(a:CbfPerson {name: 'Carl'})",
        )
        .expect("reversed-arrow CREATE must succeed");

    // Forward form: `(a)-[:T]->(b)` => src=Alice, dst=Bob.
    assert_eq!(
        count_rel_named(&mut engine, "Alice", "CbfKnows", "Bob"),
        1,
        "forward arrow must write Alice->Bob"
    );
    // Reversed form: `(b)<-[:T]-(a)` => the arrow leaves FROM `a` (here
    // named Carl) and points INTO `b` (here named Dan), so the edge must be
    // Carl->Dan — the same "arrow source -> arrow target" orientation the
    // forward form used, independent of pattern-text order.
    assert_eq!(
        count_rel_named(&mut engine, "Carl", "CbfKnows", "Dan"),
        1,
        "reversed arrow must write Carl->Dan (arrow source -> arrow target)"
    );
    assert_eq!(
        count_rel_named(&mut engine, "Dan", "CbfKnows", "Carl"),
        0,
        "reversed arrow must NOT write Dan->Carl"
    );
}

/// The MATCH...CREATE path (`execute_create_with_context`) shares the same
/// array-order bug as the standalone path — must be fixed identically.
#[test]
fn match_create_reversed_arrow_stores_edge_in_arrow_direction() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:CmcPerson {name: 'Alice'}), (b:CmcPerson {name: 'Bob'})")
        .expect("seed nodes");

    engine
        .execute_cypher(
            "MATCH (a:CmcPerson {name: 'Alice'}), (b:CmcPerson {name: 'Bob'}) \
             CREATE (a)<-[:CmcKnows]-(b)",
        )
        .expect("MATCH...CREATE reversed-arrow must succeed");

    assert_eq!(
        count_rel_named(&mut engine, "Bob", "CmcKnows", "Alice"),
        1,
        "edge must be written Bob->Alice per the `<-` arrow"
    );
    assert_eq!(
        count_rel_named(&mut engine, "Alice", "CmcKnows", "Bob"),
        0,
        "edge must NOT be written Alice->Bob — that reverses the parsed arrow"
    );
}

/// Mixed-direction multi-hop: `(a)-[:R1]->(b)<-[:R2]-(c)`. R1 is a plain
/// outgoing hop (a->b, unaffected by the bug since pattern order already
/// matches arrow direction there). R2's arrow points INTO `b`, so it must
/// be written c->b, never b->c — this is the case the array-order bug
/// silently reversed.
#[test]
fn standalone_create_mixed_direction_multi_hop() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE (a:CmhPerson {name: 'A'})-[:CmhR1]->(b:CmhPerson {name: 'B'})\
             <-[:CmhR2]-(c:CmhPerson {name: 'C'})",
        )
        .expect("mixed-direction multi-hop CREATE must succeed");

    assert_eq!(
        count_rel_named(&mut engine, "A", "CmhR1", "B"),
        1,
        "R1 must be written A->B"
    );
    assert_eq!(
        count_rel_named(&mut engine, "C", "CmhR2", "B"),
        1,
        "R2 must be written C->B (arrow points into B)"
    );
    assert_eq!(
        count_rel_named(&mut engine, "B", "CmhR2", "C"),
        0,
        "R2 must NOT be written B->C"
    );
}

/// Neo4j rejects an undirected relationship pattern (`-[:TYPE]-`, no `->`
/// or `<-`) in CREATE outright rather than silently defaulting to a
/// direction. Standalone CREATE path.
#[test]
fn standalone_create_undirected_relationship_is_rejected() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let res = engine.execute_cypher(
        "CREATE (a:CudPerson {name: 'Alice'})-[:CudKnows]-(b:CudPerson {name: 'Bob'})",
    );
    assert!(
        res.is_err(),
        "CREATE with an undirected relationship pattern (`-[:TYPE]-`) must be \
         rejected — Neo4j requires an explicit `->` or `<-`"
    );
}

/// Same undirected-form rejection, MATCH...CREATE path.
#[test]
fn match_create_undirected_relationship_is_rejected() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:CumPerson {name: 'Alice'}), (b:CumPerson {name: 'Bob'})")
        .expect("seed nodes");

    let res = engine.execute_cypher(
        "MATCH (a:CumPerson {name: 'Alice'}), (b:CumPerson {name: 'Bob'}) \
         CREATE (a)-[:CumKnows]-(b)",
    );
    assert!(
        res.is_err(),
        "MATCH...CREATE with an undirected relationship pattern (`-[:TYPE]-`) \
         must be rejected — Neo4j requires an explicit `->` or `<-`"
    );
}
