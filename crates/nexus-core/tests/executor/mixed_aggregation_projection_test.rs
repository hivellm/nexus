//! A projection that mixes grouping keys with aggregates keeps every column.
//!
//! `RETURN b, avg(x)` answered with only `b`, and a `RETURN` after an
//! aggregating `WITH` was dropped entirely so the `WITH`'s own columns leaked
//! out as the result shape. Both are silent — the query succeeds, with the
//! wrong columns.

use nexus_core::testing::setup_isolated_test_engine;

fn engine() -> nexus_core::Engine {
    let (mut engine, ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a:MixAgg {name: 'a', v: 1})-[:R]->(b:MixAgg {name: 'b', v: 2}) \
             CREATE (c:MixAgg {name: 'c', v: 3})",
        )
        .unwrap();
    std::mem::forget(ctx);
    engine
}

#[track_caller]
fn columns(query: &str) -> Vec<String> {
    let mut engine = engine();
    engine
        .execute_cypher(query)
        .unwrap_or_else(|e| panic!("`{query}` failed: {e}"))
        .columns
}

#[test]
fn a_return_mixing_a_variable_and_an_aggregate_keeps_both_columns() {
    assert_eq!(
        columns("MATCH (n:MixAgg) RETURN n.name, count(*)"),
        vec!["n.name".to_string(), "count(*)".to_string()]
    );
}

#[test]
fn a_return_mixing_a_node_and_an_aggregate_keeps_both_columns() {
    assert_eq!(
        columns("MATCH (n:MixAgg) RETURN n, count(*)"),
        vec!["n".to_string(), "count(*)".to_string()]
    );
}

#[test]
fn a_return_with_two_keys_and_an_aggregate_keeps_all_three() {
    assert_eq!(
        columns("MATCH (n:MixAgg) RETURN n.name, n.v, sum(n.v)"),
        vec![
            "n.name".to_string(),
            "n.v".to_string(),
            "sum(n.v)".to_string()
        ]
    );
}

#[test]
fn a_return_after_an_aggregating_with_replaces_its_columns() {
    assert_eq!(
        columns("MATCH (n:MixAgg) WITH n.name AS name, count(*) AS c RETURN name"),
        vec!["name".to_string()],
        "the RETURN's shape wins; the WITH's extra column must not leak out"
    );
}

#[test]
fn a_return_after_an_aggregating_with_replaces_its_columns_without_a_match() {
    // Same rule, but with no pattern anywhere in the query — a different
    // planning path, and the one the TCK's quantifier scenarios take.
    assert_eq!(
        columns("WITH 1 AS x WITH x AS result, count(*) AS cnt RETURN result"),
        vec!["result".to_string()]
    );
}

#[test]
fn a_return_after_an_aggregating_with_over_unwind_replaces_its_columns() {
    assert_eq!(
        columns(
            "WITH [1, 2, 3] AS l UNWIND l AS x \
             WITH single(y IN l WHERE false) AS result, count(*) AS cnt RETURN result"
        ),
        vec!["result".to_string()]
    );
}

#[test]
fn a_return_after_an_aggregating_with_may_order_by_a_dropped_aggregate() {
    assert_eq!(
        columns("MATCH (n:MixAgg) WITH n.name AS name, count(*) AS c RETURN name ORDER BY c"),
        vec!["name".to_string()],
        "sorting by `c` must not add it back to the result"
    );
}
