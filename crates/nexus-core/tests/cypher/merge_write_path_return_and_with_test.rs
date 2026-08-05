//! The engine write path — which intercepts `MERGE`/`SET`/`DELETE` before the
//! executor sees them — models bindings as `variable -> Vec<node_id>`. Two shapes
//! that model could not express were the only two failures on the Neo4j
//! differential suite (cases 15.08 and 15.12).
//!
//! Neither was a MERGE bug: one was a RETURN that walks a single variable's id
//! list, the other a dispatcher with no notion of a scope boundary at all.

use nexus_core::testing::setup_isolated_test_engine;

fn run(engine: &mut nexus_core::Engine, query: &str) -> nexus_core::executor::ResultSet {
    engine.execute_cypher(query).expect("query should execute")
}

/// Differential case 15.08. Two independent `MERGE`s, both variables returned.
/// The write path cannot reconstruct multi-variable rows (its per-variable lists
/// are independent, not row-aligned), so it hands the ids to the executor.
#[test]
fn a_return_spanning_two_merged_variables() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (a:Product {name: 'A'}) MERGE (b:Product {name: 'B'}) \
         RETURN a.name AS a, b.name AS b",
    );

    assert_eq!(rs.columns, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(rs.rows.len(), 1, "expected one row, got {:?}", rs.rows);
    assert_eq!(rs.rows[0].values[0].as_str(), Some("A"));
    assert_eq!(rs.rows[0].values[1].as_str(), Some("B"));
}

#[test]
fn a_return_spanning_three_merged_variables() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (a:P {n: 1}) MERGE (b:P {n: 2}) MERGE (c:P {n: 3}) RETURN a.n, b.n, c.n",
    );
    assert_eq!(rs.rows.len(), 1);
    let vals: Vec<i64> = rs.rows[0]
        .values
        .iter()
        .filter_map(|v| v.as_i64())
        .collect();
    assert_eq!(vals, vec![1, 2, 3]);
}

#[test]
fn a_single_variable_return_is_unchanged() {
    // Control: the fast path that walks one variable's ids must keep working —
    // the delegation only kicks in on a second distinct variable.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (a:Product {name: 'Solo'}) RETURN a.name AS n",
    );
    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values[0].as_str(), Some("Solo"));
}

/// Differential case 15.12. A `WITH` between two writes, and the second `MERGE`
/// must MATCH what the first created rather than duplicate it — which is what the
/// case's name ("verify single node") is really asserting, so the node count is
/// checked here and not only the returned count.
#[test]
fn a_with_between_two_merges_keeps_one_node() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (n:Product {name: 'Unique'}) WITH n \
         MERGE (n2:Product {name: 'Unique'}) RETURN count(DISTINCT n) AS cnt",
    );
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(1), "cnt must be 1");

    engine.refresh_executor().unwrap();
    let count = run(&mut engine, "MATCH (p:Product) RETURN count(p) AS c");
    assert_eq!(
        count.rows[0].values[0].as_i64(),
        Some(1),
        "the second MERGE must have matched, not created a duplicate"
    );
}

#[test]
fn a_with_can_rename_the_carried_variable() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (n:Product {name: 'R'}) WITH n AS m RETURN m.name AS name",
    );
    assert_eq!(rs.rows[0].values[0].as_str(), Some("R"));
}

#[test]
fn a_with_cuts_the_variables_it_does_not_project() {
    // The scope-cut half of the semantics: `b` is bound before the WITH and gone
    // after it, so the RETURN can only see `a`.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (a:P {n: 1}) MERGE (b:P {n: 2}) WITH a RETURN a.n AS n",
    );
    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(1));
}

#[test]
fn a_with_projecting_an_expression_is_rejected_with_a_specific_message() {
    // Only a bare variable carries an id list. An expression has no binding to
    // carry, so it must say so rather than silently dropping the projection —
    // and rather than the old blanket "Unsupported clause in write query".
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("MERGE (n:P {n: 1}) WITH n.n AS v MERGE (m:P {n: 2}) RETURN v")
        .expect_err("a projected expression in a write-path WITH must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("bare variable projections only"),
        "expected a message naming what is supported, got: {msg}"
    );
}

/// Differential case 15.05, and the regression the first cut of this change
/// introduced: a RETURN that names NO variable still needs the write's rows to
/// count. Selecting materialisation variables from the RETURN made that list
/// empty and the query errored — the old arbitrary `keys().next()` had masked it.
#[test]
fn a_return_naming_no_variable_still_counts_the_written_rows() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (n:Product {name: 'NewItem'}) RETURN count(*) AS cnt",
    );
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(1));
}

#[test]
fn count_star_over_two_merged_variables_is_one_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = run(
        &mut engine,
        "MERGE (a:P {n: 1}) MERGE (b:P {n: 2}) RETURN count(*) AS cnt",
    );
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(1));
}
