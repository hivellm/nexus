//! Tests for the list-predicate quantifiers any/all/none/single.
//!
//! phase21_tck-quantifier-in-where-parser: these parse as
//! `any/all/none/single(x IN list WHERE predicate)` (WHERE required),
//! mirroring the filter() special form, and evaluate in a WHERE clause
//! (the task's target — the openCypher `expressions/quantifier` scenarios
//! and pattern predicates use them predominantly in WHERE).
//!
//! KNOWN FOLLOW-UP: a quantifier used as a bare RETURN *projection*
//! (`RETURN any(x IN [...] WHERE ...)`) currently drops the row — the
//! planner's projection column-collection hoists the quantifier's inner
//! bound variable (`x`) as an external column (filter() dodges this by
//! lowering to a ListComprehension node the planner treats specially).
//! That is a planner variable-scoping fix, tracked separately (belongs
//! with phase21_tck-semantic-analysis-pass / a projection-scoping task),
//! not the parser task — see the `#[ignore]`d test below.

use nexus_core::testing::setup_isolated_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

#[test]
fn quantifier_without_where_is_a_parse_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // WHERE is required for these four quantifiers (unlike filter()).
    for q in [
        "RETURN any(x IN [1, 2, 3]) AS r",
        "RETURN all(x IN [1, 2, 3]) AS r",
        "RETURN none(x IN [1, 2, 3]) AS r",
        "RETURN single(x IN [1, 2, 3]) AS r",
    ] {
        assert!(
            engine.execute_cypher(q).is_err(),
            "a quantifier without WHERE must be a parse error: {q}"
        );
    }
}

/// The task's target: quantifiers in a WHERE clause filter rows correctly.
#[test]
fn quantifiers_in_where_filter_rows() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Inline array literals in CREATE are unsupported; seed via the data API.
    engine
        .create_node(
            vec!["N".to_string()],
            serde_json::json!({ "id": 1, "vals": [1, 2, 3] }),
        )
        .unwrap();
    engine
        .create_node(
            vec!["N".to_string()],
            serde_json::json!({ "id": 2, "vals": [1, 1, 1] }),
        )
        .unwrap();

    // any: node 1 has an element > 2, node 2 does not.
    let any = execute_query(
        &mut engine,
        "MATCH (n:N) WHERE any(x IN n.vals WHERE x > 2) RETURN n.id AS id",
    );
    assert_eq!(
        any.rows.len(),
        1,
        "any(): expected 1 node, got {:?}",
        any.rows
    );

    // all: only node 2 has every element < 2.
    let all = execute_query(
        &mut engine,
        "MATCH (n:N) WHERE all(x IN n.vals WHERE x < 2) RETURN n.id AS id",
    );
    assert_eq!(
        all.rows.len(),
        1,
        "all(): expected 1 node, got {:?}",
        all.rows
    );

    // none: only node 2 has no element > 1.
    let none = execute_query(
        &mut engine,
        "MATCH (n:N) WHERE none(x IN n.vals WHERE x > 1) RETURN n.id AS id",
    );
    assert_eq!(
        none.rows.len(),
        1,
        "none(): expected 1 node, got {:?}",
        none.rows
    );

    // single: only node 1 has exactly one element equal to 2.
    let single = execute_query(
        &mut engine,
        "MATCH (n:N) WHERE single(x IN n.vals WHERE x = 2) RETURN n.id AS id",
    );
    assert_eq!(
        single.rows.len(),
        1,
        "single(): expected 1 node, got {:?}",
        single.rows
    );
}

/// KNOWN FOLLOW-UP (see module doc): a quantifier as a bare RETURN
/// projection drops the row due to a planner projection-scoping bug that
/// hoists the inner bound variable. Ignored until that fix lands.
#[test]
#[ignore = "planner hoists quantifier bound var in RETURN projection — follow-up in semantic-analysis-pass"]
fn quantifier_in_return_projection_is_a_known_gap() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let r = execute_query(&mut engine, "RETURN any(x IN [1, 2, 3] WHERE x > 2) AS r");
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_bool(), Some(true));
}
