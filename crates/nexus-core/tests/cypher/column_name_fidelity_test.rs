//! Column-name fidelity: an unaliased RETURN item's column header must
//! render the verbatim-style source expression (openCypher TCK / Neo4j),
//! not the bare aggregate function name. Regression coverage for the
//! aggregate default-alias fix in the planner_core and strategy planning
//! paths plus the float-literal `.0` rendering fix.

use nexus_core::Engine;
use nexus_core::testing::setup_isolated_test_engine;

fn columns(engine: &mut Engine, query: &str) -> Vec<String> {
    engine
        .execute_cypher(query)
        .expect("query should succeed")
        .columns
}

#[test]
fn count_star_renders_verbatim() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Standalone RETURN (planner_core path).
    assert_eq!(columns(&mut engine, "RETURN count(*)"), vec!["count(*)"]);
}

#[test]
fn aggregates_over_match_render_verbatim() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .create_node(vec!["N".to_string()], serde_json::json!({ "age": 5 }))
        .unwrap();
    // MATCH path (strategy planning path).
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN count(n)"),
        vec!["count(n)"]
    );
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN sum(n.age)"),
        vec!["sum(n.age)"]
    );
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN avg(n.age)"),
        vec!["avg(n.age)"]
    );
}

#[test]
fn distinct_aggregate_renders_distinct_keyword() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .create_node(vec!["N".to_string()], serde_json::json!({ "age": 5 }))
        .unwrap();
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN count(DISTINCT n)"),
        vec!["count(DISTINCT n)"]
    );
}

#[test]
fn grouped_aggregate_keeps_both_columns_verbatim() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .create_node(vec!["N".to_string()], serde_json::json!({ "age": 5 }))
        .unwrap();
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN n.age, count(*)"),
        vec!["n.age", "count(*)"]
    );
}

#[test]
fn explicit_alias_still_wins() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .create_node(vec!["N".to_string()], serde_json::json!({ "age": 5 }))
        .unwrap();
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN count(*) AS total"),
        vec!["total"]
    );
}

#[test]
fn non_aggregate_expressions_render_verbatim() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .create_node(vec!["N".to_string()], serde_json::json!({ "age": 5 }))
        .unwrap();
    assert_eq!(
        columns(&mut engine, "MATCH (n:N) RETURN n.age + 1"),
        vec!["n.age + 1"]
    );
}

#[test]
fn integral_float_literal_keeps_decimal() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Rust's f64::to_string drops the fractional part of an integral float;
    // the column header must preserve `1.0` rather than collapse to `1`.
    assert_eq!(columns(&mut engine, "RETURN 1.0"), vec!["1.0"]);
}
