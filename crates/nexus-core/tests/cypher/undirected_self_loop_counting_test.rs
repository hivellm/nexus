//! Undirected matching over a self-loop must count it once. A self-loop's two
//! orientations are the same binding — source and target are the same node — so
//! emitting both inflates every undirected count over a looping node.
//!
//! Relationship isomorphism (`MATCH ()-[]-()-[]-()` must not reuse one
//! relationship across two slots) is a separate, larger defect tracked by
//! `phase21_tck-consecutive-relationship-match-clauses`; it is NOT covered here,
//! so the multi-slot counts the openCypher TCK
//! `useCases/countingSubgraphMatches` [10] and [11] scenarios assert are still
//! wrong. See that task for the verified expectations.

use nexus_core::testing::setup_isolated_test_engine;

fn count(engine: &mut nexus_core::Engine, query: &str) -> i64 {
    engine
        .execute_cypher(query)
        .expect("query should execute")
        .rows
        .first()
        .and_then(|r| r.values.first().and_then(|v| v.as_i64()))
        .expect("expected a single count row")
}

#[test]
fn source_less_undirected_scan_counts_a_self_loop_once() {
    // The fully-anonymous form takes the source-less relationship scan, which
    // emitted every relationship once per direction. For a self-loop that is the
    // same binding twice.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (l:Looper)-[:LOOP]->(l)")
        .expect("fixture should be created");

    assert_eq!(count(&mut engine, "MATCH ()-[]-() RETURN count(*)"), 1);
}

#[test]
fn source_less_undirected_scan_still_counts_a_normal_edge_twice() {
    // Control: a non-loop relationship genuinely has two distinct orientations
    // under an undirected pattern, and must keep yielding both.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(count(&mut engine, "MATCH ()-[]-() RETURN count(*)"), 2);
}

#[test]
fn source_less_directed_scan_counts_a_self_loop_once() {
    // Control: the directed form was never affected — one orientation, one row.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (l:Looper)-[:LOOP]->(l)")
        .expect("fixture should be created");

    assert_eq!(count(&mut engine, "MATCH ()-[]->() RETURN count(*)"), 1);
}

#[test]
fn anchored_undirected_match_counts_a_self_loop_once() {
    // Control: the anchored path (a bound source node) already counted a
    // self-loop once and must keep doing so.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (l:Looper)-[:LOOP]->(l)")
        .expect("fixture should be created");

    assert_eq!(
        count(&mut engine, "MATCH (l:Looper)--() RETURN count(*)"),
        1
    );
}

#[test]
fn source_less_undirected_scan_over_a_mixed_graph() {
    // `(:A)-[:T1]->(l:Looper)`, `(l)-[:LOOP]->(l)`, `(l)-[:T2]->(:B)`: two normal
    // edges contribute two orientations each, the loop contributes one — five
    // single-slot bindings, not six.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:A)-[:T1]->(l:Looper), (l)-[:LOOP]->(l), (l)-[:T2]->(:B)")
        .expect("fixture should be created");

    assert_eq!(count(&mut engine, "MATCH ()-[]-() RETURN count(*)"), 5);
}
