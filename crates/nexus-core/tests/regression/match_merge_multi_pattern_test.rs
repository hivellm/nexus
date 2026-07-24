//! A comma-joined `MATCH (a:A), (b:B) MERGE (a)-[:T]->(b)` must MERGE once per
//! driving row, not just the first.
//!
//! `process_merge_relationship` (`engine/write_exec.rs`) resolved each endpoint
//! by collapsing its bound id list to `ids[0]`, so when a preceding MATCH bound
//! an endpoint to several nodes every row after the first was silently dropped
//! — `MATCH (c:C), (d:D) MERGE (c)-[:S]->(d)` over 2 C's and 1 D created ONE
//! edge, not two. Under-counting is worse than the CREATE over-count it was
//! found next to: it silently loses writes. Fixed by MERGE-ing the cartesian
//! product of the two endpoint id lists. See phase7_opencypher-gap-closure
//! item 4.10.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn count(engine: &mut Engine, cypher: &str) -> i64 {
    engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("`{cypher}` failed: {e}"))
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is an integer")
}

#[test]
fn comma_join_merge_runs_once_per_driving_row() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:C {id: 1}), (:C {id: 2}), (:C {id: 3}), (:D {id: 9})")
        .expect("seed");

    engine
        .execute_cypher("MATCH (c:C), (d:D) MERGE (c)-[:S]->(d)")
        .expect("merge");

    assert_eq!(
        count(&mut engine, "MATCH ()-[r:S]->() RETURN count(r)"),
        3,
        "one edge per (c, d) driving pair, not just the first"
    );
    assert_eq!(
        count(
            &mut engine,
            "MATCH (c:C)-[:S]->(:D) RETURN count(DISTINCT c)"
        ),
        3,
        "every C must have merged its edge"
    );
}

#[test]
fn comma_join_merge_is_idempotent() {
    // MERGE is find-or-create: re-running must not add duplicates.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:C {id: 1}), (:C {id: 2}), (:D {id: 9})")
        .expect("seed");

    for _ in 0..3 {
        engine
            .execute_cypher("MATCH (c:C), (d:D) MERGE (c)-[:S]->(d)")
            .expect("merge");
    }
    assert_eq!(
        count(&mut engine, "MATCH ()-[r:S]->() RETURN count(r)"),
        2,
        "repeated MERGE over the same pairs stays at two edges"
    );
}

#[test]
fn comma_join_merge_full_cartesian() {
    // Both endpoints bound to several nodes → the full product is merged.
    let (mut engine, _ctx) = engine();
    for i in 0..3 {
        engine
            .execute_cypher(&format!("CREATE (:P {{id: {i}}})"))
            .expect("seed p");
    }
    for j in 0..4 {
        engine
            .execute_cypher(&format!("CREATE (:Q {{id: {j}}})"))
            .expect("seed q");
    }

    engine
        .execute_cypher("MATCH (p:P), (q:Q) MERGE (p)-[:R]->(q)")
        .expect("merge");
    assert_eq!(
        count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"),
        12,
        "3 P × 4 Q merges 12 distinct edges"
    );
}

#[test]
fn on_create_fires_for_every_merged_row() {
    // ON CREATE SET must run for each newly created edge, not only the first.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:C {id: 1}), (:C {id: 2}), (:C {id: 3}), (:D {id: 9})")
        .expect("seed");

    engine
        .execute_cypher("MATCH (c:C), (d:D) MERGE (c)-[r:S]->(d) ON CREATE SET r.tag = 'new'")
        .expect("merge on create");

    assert_eq!(
        count(
            &mut engine,
            "MATCH ()-[r:S]->() WHERE r.tag = 'new' RETURN count(r)"
        ),
        3,
        "ON CREATE SET must apply to every merged edge"
    );
}

#[test]
fn incoming_direction_merges_the_reversed_edge_per_row() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:C {id: 1}), (:C {id: 2}), (:D {id: 9})")
        .expect("seed");

    // `(c)<-[:S]-(d)` writes d -> c for every driving pair.
    engine
        .execute_cypher("MATCH (c:C), (d:D) MERGE (c)<-[:S]-(d)")
        .expect("merge incoming");

    assert_eq!(
        count(&mut engine, "MATCH (:D)-[r:S]->(:C) RETURN count(r)"),
        2,
        "the reversed edge is written once per pair"
    );
    assert_eq!(
        count(&mut engine, "MATCH (:C)-[r:S]->(:D) RETURN count(r)"),
        0,
        "no forward edge is created"
    );
}

#[test]
fn single_row_and_unwind_merge_paths_stay_correct() {
    // The paths that were already correct must not regress.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:C {id: 1}), (:C {id: 2}), (:D {id: 9})")
        .expect("seed");

    // Both endpoints pinned to one node → exactly one edge, idempotent.
    engine
        .execute_cypher("MATCH (c:C {id: 1}), (d:D {id: 9}) MERGE (c)-[:ONE]->(d)")
        .expect("single");
    engine
        .execute_cypher("MATCH (c:C {id: 1}), (d:D {id: 9}) MERGE (c)-[:ONE]->(d)")
        .expect("single again");
    assert_eq!(
        count(&mut engine, "MATCH ()-[r:ONE]->() RETURN count(r)"),
        1
    );

    // UNWIND resolving both endpoints by id → one edge per element.
    engine
        .execute_cypher(
            "UNWIND [1, 2] AS i MATCH (c:C {id: i}), (d:D {id: 9}) MERGE (c)-[:BY_ID]->(d)",
        )
        .expect("unwind merge");
    assert_eq!(
        count(&mut engine, "MATCH ()-[r:BY_ID]->() RETURN count(r)"),
        2
    );
}
