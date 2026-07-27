//! A `WHERE` clause containing a `CASE` expression (or a list/pattern
//! comprehension) must evaluate correctly.
//!
//! WHERE predicates were lowered by serializing the parsed `Expression` to a
//! string and re-parsing it at evaluation time. `expression_to_string` had no
//! arm for `Expression::Case` (nor comprehensions), so they fell through its
//! `_ => "?"` catch-all: a `WHERE CASE ... END` became the predicate string
//! `"?"`, which does not re-parse to the intended predicate and silently
//! returned the wrong rows. The fix carries the AST through to the filter
//! operator and evaluates it directly (the same `&Expression`->bool evaluator
//! projections already use — which is why projected CASE always worked).

use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use std::collections::HashMap;

fn values(executor: &nexus_core::executor::Executor, cypher: &str) -> Vec<i64> {
    let result = executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        })
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
    result
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().unwrap())
        .collect()
}

fn seed(executor: &mut nexus_core::executor::Executor) {
    executor
        .execute(&Query {
            cypher: "CREATE (:M {x: 5}), (:M {x: 3}), (:M {x: 10}), (:M {x: 1})".to_string(),
            params: HashMap::new(),
        })
        .unwrap();
}

/// `WHERE CASE WHEN n.x > 4 THEN true ELSE false END` must keep only the
/// positive-x nodes. Pre-fix the predicate degraded to `"?"` and the result
/// was wrong.
#[test]
fn where_case_predicate_filters_correctly() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:M) WHERE CASE WHEN n.x > 4 THEN true ELSE false END \
         RETURN n.x AS x ORDER BY x",
    );
    assert_eq!(v, vec![5, 10], "only x>4 nodes must pass the CASE WHERE");
}

/// A searched CASE with an explicit comparison result.
#[test]
fn where_case_searched_predicate_filters_correctly() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:M) WHERE (CASE WHEN n.x >= 10 THEN 1 ELSE 0 END) = 1 \
         RETURN n.x AS x ORDER BY x",
    );
    assert_eq!(v, vec![10], "only x >= 10 must pass");
}

/// Control: the SAME CASE in a projection already worked (projections evaluate
/// the AST directly) — this pins that the defect was the WHERE round-trip.
#[test]
fn projected_case_control_already_works() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    // Project x alongside c and order by x so the pairing is deterministic.
    let result = executor
        .execute(&Query {
            cypher: "MATCH (n:M) RETURN n.x AS x, CASE WHEN n.x > 4 THEN 1 ELSE 0 END AS c \
                     ORDER BY x"
                .to_string(),
            params: HashMap::new(),
        })
        .unwrap();
    let pairs: Vec<(i64, i64)> = result
        .rows
        .iter()
        .map(|r| (r.values[0].as_i64().unwrap(), r.values[1].as_i64().unwrap()))
        .collect();
    // x sorted: 1, 3, 5, 10 -> c: 0,0,1,1
    assert_eq!(pairs, vec![(1, 0), (3, 0), (5, 1), (10, 1)]);
}

/// `OPTIONAL MATCH ... WHERE CASE` must also evaluate the AST (OptionalFilter).
#[test]
fn optional_match_where_case_predicate_filters_correctly() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    // OPTIONAL MATCH binds n; the CASE WHERE keeps only positive-x rows.
    let v = values(
        &executor,
        "OPTIONAL MATCH (n:M) WHERE CASE WHEN n.x > 4 THEN true ELSE false END \
         RETURN n.x AS x ORDER BY x",
    );
    assert_eq!(v, vec![5, 10]);
}
