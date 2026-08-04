//! An unlabelled node in a second (or comma-separated) `MATCH` pattern must get a
//! driving scan, so the pattern joins with the rows already in flight instead of
//! leaving its variable unbound.
//!
//! Before this, the additional-pattern lowering emitted a scan only for *labelled*
//! nodes, so an unlabelled one was never bound: as an `Expand` source it made every
//! row drop (zero rows for the whole query), and as a bare node it projected as
//! NULL. The first pattern's lowering always emitted `AllNodesScan` here — only the
//! additional-pattern loop was missing it.

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

fn rows(engine: &mut nexus_core::Engine, query: &str) -> Vec<Vec<Value>> {
    engine
        .execute_cypher(query)
        .expect("query should execute")
        .rows
        .into_iter()
        .map(|r| r.values)
        .collect()
}

fn count(engine: &mut nexus_core::Engine, query: &str) -> i64 {
    rows(engine, query)
        .first()
        .and_then(|r| r.first().and_then(Value::as_i64))
        .expect("expected a single count row")
}

fn node_id(value: &Value) -> Option<i64> {
    value.get("_nexus_id").and_then(Value::as_i64)
}

#[test]
fn a_second_clause_with_a_relationship_pattern_yields_rows() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN count(*)"
        ),
        1,
        "the one relationship pairs with itself across the two clauses"
    );
}

#[test]
fn a_second_clause_relationship_pattern_keeps_both_clauses_correlated() {
    // The join must not decorrelate: `x`/`y` come from the first clause's Expand as
    // a tuple and must stay paired with the second clause's `p`/`q`.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    let result = rows(
        &mut engine,
        "MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN x, y, p, q",
    );
    assert_eq!(result.len(), 1);
    let row = &result[0];
    assert_eq!(node_id(&row[0]), node_id(&row[2]), "x and p are both :A");
    assert_eq!(node_id(&row[1]), node_id(&row[3]), "y and q are both :B");
    assert_ne!(node_id(&row[0]), node_id(&row[1]), "x and y differ");
}

#[test]
fn a_second_clause_bare_node_produces_the_cartesian_not_a_null() {
    // Two nodes in the graph, so one driving row times two nodes is two rows — and
    // `p` must be bound in both, not NULL.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    let result = rows(&mut engine, "MATCH (x)-[r1]->(y) MATCH (p) RETURN x, p");
    assert_eq!(result.len(), 2, "got: {result:?}");
    for row in &result {
        assert!(!row[1].is_null(), "p must be bound, got: {row:?}");
        assert_eq!(
            node_id(&row[0]),
            Some(0),
            "x stays the :A node in every row"
        );
    }
    let mut p_ids: Vec<Option<i64>> = result.iter().map(|r| node_id(&r[1])).collect();
    p_ids.sort();
    assert_eq!(p_ids, vec![Some(0), Some(1)], "p ranges over both nodes");
}

#[test]
fn a_variable_bound_by_an_earlier_clause_is_not_rescanned() {
    // Control: `b` is already bound, so the second clause must continue from it
    // rather than re-drive the query from every node. `:B` has no outgoing edge, so
    // the correct answer is zero rows — a rescan would have found the `:A`->`:B`
    // edge again and wrongly returned one.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (a2)-[r]->(b2) MATCH (b2)-[r2]->(c2) RETURN count(*)"
        ),
        0
    );
}

#[test]
fn a_two_hop_chain_across_two_clauses_matches() {
    // The positive half of the control above: with a real two-hop chain, continuing
    // from the bound variable finds it.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)-[:T]->(c:C)")
        .expect("fixture should be created");

    let result = rows(
        &mut engine,
        "MATCH (a2:A)-[r]->(b2) MATCH (b2)-[r2]->(c2) RETURN a2, b2, c2",
    );
    assert_eq!(result.len(), 1, "got: {result:?}");
    let row = &result[0];
    assert_eq!(node_id(&row[0]), Some(0));
    assert_eq!(node_id(&row[1]), Some(1));
    assert_eq!(node_id(&row[2]), Some(2));
}

#[test]
fn a_labelled_second_pattern_still_works() {
    // Control: the labelled path was always lowered and must be unchanged.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (x:A)-[r1]->(y:B) MATCH (p:A)-[r2]->(q:B) RETURN count(*)"
        ),
        1
    );
}
