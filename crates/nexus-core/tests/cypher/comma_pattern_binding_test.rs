//! Comma-separated `MATCH` patterns must bind every one of their variables into
//! the driving rows, even when a LATER clause uses one of them as a relationship
//! target.
//!
//! The planner used to collect "nodes an `Expand` will populate" across every
//! pattern of the query and suppress a driving scan for all of them. A variable
//! written in one clause and used as a relationship target in another therefore
//! got no scan at all, so the later `OPTIONAL MATCH` treated it as its own output
//! slot: first observed rebinding it to whatever node the edge led to, then —
//! after the padding helper learned to preserve a bound target — NULL-padding it.
//! Both are wrong; openCypher keeps the cartesian binding and nulls only the
//! relationship.

use nexus_core::testing::setup_isolated_test_engine;

/// `(a:A {n:'a'})`, `(z:Z {n:'z'})`, `(b:B)` with `(a)-[:T]->(b)` — so `a` has a
/// `:T` edge, just not to `z`.
fn create_disjoint_edge_fixture(engine: &mut nexus_core::Engine) {
    engine
        .execute_cypher("CREATE (a:A {n: 'a'}), (z:Z {n: 'z'}), (b:B)")
        .expect("fixture nodes should be created");
    engine
        .execute_cypher("MATCH (a:A), (b:B) CREATE (a)-[:T]->(b)")
        .expect("fixture relationship should be created");
}

#[test]
fn comma_pattern_variable_survives_an_optional_match_that_cannot_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_disjoint_edge_fixture(&mut engine);

    let result = engine
        .execute_cypher(
            "MATCH (a:A), (z:Z)
             OPTIONAL MATCH (a)-[r:T]->(z)
             RETURN a.n, z.n, r IS NULL",
        )
        .expect("query should execute");

    assert_eq!(result.rows.len(), 1, "expected the single cartesian row");
    let row = &result.rows[0].values;
    assert_eq!(row[0].as_str(), Some("a"));
    assert_eq!(
        row[1].as_str(),
        Some("z"),
        "z must keep its comma-pattern binding, not be rebound or nulled"
    );
    assert_eq!(
        row[2].as_bool(),
        Some(true),
        "r must be NULL — no a->z edge"
    );
}

#[test]
fn comma_pattern_optional_match_still_binds_a_relationship_that_exists() {
    // Control: when the edge DOES join the two comma-pattern variables, the
    // optional hop binds it rather than padding.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A {n: 'a'}), (z:Z {n: 'z'})")
        .expect("fixture nodes should be created");
    engine
        .execute_cypher("MATCH (a:A), (z:Z) CREATE (a)-[:T]->(z)")
        .expect("fixture relationship should be created");

    let result = engine
        .execute_cypher(
            "MATCH (a:A), (z:Z)
             OPTIONAL MATCH (a)-[r:T]->(z)
             RETURN a.n, z.n, r IS NULL",
        )
        .expect("query should execute");

    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0].values;
    assert_eq!(row[0].as_str(), Some("a"));
    assert_eq!(row[1].as_str(), Some("z"));
    assert_eq!(row[2].as_bool(), Some(false), "r must be bound");
}

#[test]
fn comma_pattern_cartesian_without_an_optional_clause_is_unchanged() {
    // Control for the scan the fix restored: the plain cartesian was already
    // right, and must stay right.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_disjoint_edge_fixture(&mut engine);

    let result = engine
        .execute_cypher("MATCH (a:A), (z:Z) RETURN a.n, z.n")
        .expect("query should execute");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_str(), Some("a"));
    assert_eq!(result.rows[0].values[1].as_str(), Some("z"));
}

/// openCypher TCK `clauses/with-where` `WithWhere1` [3] "Filter for an unbound
/// relationship variable".
#[test]
fn with_where_filters_for_an_unbound_relationship_variable() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_two_b_fixture(&mut engine);

    let result = engine
        .execute_cypher(
            "MATCH (a:A), (other:B)
             OPTIONAL MATCH (a)-[r]->(other)
             WITH other WHERE r IS NULL
             RETURN other.id",
        )
        .expect("query should execute");

    let ids: Vec<i64> = result
        .rows
        .iter()
        .filter_map(|r| r.values[0].as_i64())
        .collect();
    assert_eq!(
        ids,
        vec![2],
        "only the :B with no incoming edge from :A survives, got {:?}",
        result.rows
    );
}

/// openCypher TCK `clauses/with-where` `WithWhere1` [4] "Filter for an unbound
/// node variable" — here the optional pattern's SOURCE is the unbound one.
#[test]
fn with_where_filters_for_an_unbound_node_variable() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_two_b_fixture(&mut engine);

    let result = engine
        .execute_cypher(
            "MATCH (other:B)
             OPTIONAL MATCH (a)-[r]->(other)
             WITH other WHERE a IS NULL
             RETURN other.id",
        )
        .expect("query should execute");

    let ids: Vec<i64> = result
        .rows
        .iter()
        .filter_map(|r| r.values[0].as_i64())
        .collect();
    assert_eq!(
        ids,
        vec![2],
        "only the :B with no incoming edge survives, got {:?}",
        result.rows
    );
}

/// `(a:A)`, `(b:B {id:1})`, `(:B {id:2})` with `(a)-[:T]->(b)` — the
/// `WithWhere1` [3]/[4] fixture.
fn create_two_b_fixture(engine: &mut nexus_core::Engine) {
    engine
        .execute_cypher("CREATE (a:A), (b:B {id: 1}), (:B {id: 2})")
        .expect("fixture nodes should be created");
    engine
        .execute_cypher("MATCH (a:A), (b:B {id: 1}) CREATE (a)-[:T]->(b)")
        .expect("fixture relationship should be created");
}

#[test]
fn label_predicate_on_a_variable_an_earlier_pattern_bound_is_enforced() {
    // Restoring the per-pattern scan set exposed this: a later clause naming an
    // already-bound variable WITH a label used to be skipped wholesale, dropping
    // the predicate. It is now a Filter — the binding survives AND `:B` is
    // enforced. The negative case is the one that was silently wrong.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:Other)")
        .expect("fixture should be created");

    let result = engine
        .execute_cypher("MATCH (a:A)-[:T]->(b) MATCH (b:B) RETURN count(*)")
        .expect("query should execute");
    assert_eq!(
        result.rows[0].values[0].as_i64(),
        Some(0),
        "b is :Other, so the second clause's :B must reject it"
    );
}

#[test]
fn label_predicate_on_an_already_bound_variable_still_matches_when_it_holds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    let result = engine
        .execute_cypher("MATCH (a:A)-[:T]->(b) MATCH (b:B) RETURN count(*)")
        .expect("query should execute");
    assert_eq!(
        result.rows[0].values[0].as_i64(),
        Some(1),
        "b IS :B, so the row must survive with its binding intact"
    );
}
