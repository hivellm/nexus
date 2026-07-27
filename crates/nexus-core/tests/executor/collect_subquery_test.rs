//! `COLLECT { … }` subquery expression tests
//! (phase6_opencypher-subquery-transactions slice-5).
//!
//! Covers the evaluator wired in
//! `crates/nexus-core/src/executor/eval/projection.rs`:
//!
//! - single-column inner → `LIST<T>` of scalars,
//! - multi-column inner → `LIST<MAP>` keyed by the inner column names,
//! - aggregating inner → single-element list,
//! - empty inner stream → empty list (NOT NULL),
//! - parser-level guards (`ERR_COLLECT_SUBQUERY_NO_RETURN`,
//!   `ERR_COLLECT_SUBQUERY_EMPTY`).

use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use std::collections::HashMap;

fn run(
    executor: &mut nexus_core::executor::Executor,
    cypher: &str,
) -> nexus_core::executor::ResultSet {
    executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        })
        .unwrap_or_else(|e| panic!("query failed: {cypher}\n  err: {e}"))
}

fn try_run(
    executor: &mut nexus_core::executor::Executor,
    cypher: &str,
) -> Result<nexus_core::executor::ResultSet, nexus_core::Error> {
    executor.execute(&Query {
        cypher: cypher.to_string(),
        params: HashMap::new(),
    })
}

fn seed(executor: &mut nexus_core::executor::Executor) {
    run(
        executor,
        "CREATE (:Person {name: 'Alice', age: 30}), \
                (:Person {name: 'Bob',   age: 25})",
    );
}

#[test]
fn collect_subquery_single_column_returns_list_of_scalars() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "RETURN COLLECT { MATCH (p:Person) RETURN p.name } AS names",
    );

    assert_eq!(rs.columns, vec!["names"]);
    assert_eq!(rs.rows.len(), 1);
    let names = match &rs.rows[0].values[0] {
        serde_json::Value::Array(a) => a.clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(names.len(), 2);
    let mut names_str: Vec<String> = names
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    names_str.sort();
    assert_eq!(names_str, vec!["Alice".to_string(), "Bob".to_string()]);
}

#[test]
fn collect_subquery_aggregating_inner_returns_single_element_list() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "RETURN COLLECT { MATCH (p:Person) RETURN count(p) AS c } AS counts",
    );
    let arr = match &rs.rows[0].values[0] {
        serde_json::Value::Array(a) => a.clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0], serde_json::json!(2));
}

#[test]
fn collect_subquery_empty_inner_returns_empty_list_not_null() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    // No nodes seeded — inner MATCH (p:Person) emits zero rows.
    let rs = run(
        &mut executor,
        "RETURN COLLECT { MATCH (p:Person) RETURN p.name } AS names",
    );
    assert_eq!(rs.rows[0].values[0], serde_json::json!([]));
}

#[test]
fn collect_subquery_multi_column_returns_list_of_maps() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "RETURN COLLECT { MATCH (p:Person) RETURN p.name AS n, p.age AS a } AS rows",
    );
    let arr = match &rs.rows[0].values[0] {
        serde_json::Value::Array(a) => a.clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(arr.len(), 2);
    for elem in &arr {
        let m = elem.as_object().expect("each element must be a MAP");
        assert!(m.contains_key("n"));
        assert!(m.contains_key("a"));
    }
}

#[test]
fn collect_subquery_missing_return_is_rejected_at_parse_time() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(
        &mut executor,
        "RETURN COLLECT { MATCH (p:Person) } AS names",
    );
    let err = outcome.expect_err("missing RETURN must fail to parse");
    assert!(
        err.to_string().contains("ERR_COLLECT_SUBQUERY_NO_RETURN"),
        "expected NO_RETURN sentinel, got: {err}"
    );
}

#[test]
fn collect_subquery_empty_body_is_rejected_at_parse_time() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(&mut executor, "RETURN COLLECT { } AS names");
    let err = outcome.expect_err("empty body must fail to parse");
    assert!(
        err.to_string().contains("ERR_COLLECT_SUBQUERY_EMPTY"),
        "expected EMPTY sentinel, got: {err}"
    );
}

#[test]
fn collect_subquery_uppercase_identifier_still_routes_to_function_call() {
    // Sanity check: the disambiguation must NOT swallow `collect(expr)`
    // as a subquery. The lowercased form is the legacy aggregation
    // function and still parses through the regular FunctionCall path.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);
    let rs = run(
        &mut executor,
        "MATCH (p:Person) RETURN collect(p.name) AS names",
    );
    assert_eq!(rs.columns, vec!["names"]);
    assert_eq!(rs.rows.len(), 1);
    if let serde_json::Value::Array(arr) = &rs.rows[0].values[0] {
        assert_eq!(arr.len(), 2);
    } else {
        panic!("collect() function should produce a list");
    }
}
