//! `EXISTS { MATCH ... [WHERE ...] [RETURN ...] }` full-subquery form
//! tests (openCypher TCK `existentialSubqueries`).
//!
//! Covers the parser (`parse_exists_subquery_body` /
//! `try_desugar_exists_pattern` in
//! `crates/nexus-core/src/executor/parser/expressions/structured.rs`) and
//! the evaluator (`evaluate_exists_subquery` in
//! `crates/nexus-core/src/executor/eval/projection/core.rs`):
//!
//! - full-form existence (true/false),
//! - write clauses rejected at parse time (`InvalidClauseComposition`),
//! - `WITH ... WHERE` aggregation shape (`ExistentialSubquery2[2]`),
//! - nested `EXISTS` (pattern-in-full and full-in-full,
//!   `ExistentialSubquery3`),
//! - correlation with the outer row and no-leak of inner bindings.

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

/// Mirrors the graph shared by `ExistentialSubquery1`/`2`/`3`:
/// `a` has three outgoing edges (to `b`, `c`, `d`); only `a`-`b` has
/// matching `prop` values, and `b` also points at `d`.
fn seed(executor: &mut nexus_core::executor::Executor) {
    run(
        executor,
        "CREATE (a:A {prop: 1})-[:R]->(b:B {prop: 1}), \
                (a)-[:R]->(:C {prop: 2}), \
                (a)-[:R]->(d:D {prop: 3}), \
                (b)-[:R]->(d)",
    );
}

#[test]
fn exists_subquery_full_form_true_when_match_exists() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n) WHERE exists { MATCH (n)-->() RETURN true } \
         RETURN n.prop AS prop ORDER BY prop",
    );

    let props: Vec<i64> = rs
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("prop is an integer"))
        .collect();
    // `a` (prop 1) and `b` (prop 1) both have an outgoing edge; `d` and the
    // orphan `c` do not.
    assert_eq!(props, vec![1, 1]);
}

#[test]
fn exists_subquery_full_form_false_when_no_match() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(&mut executor, "CREATE (:Lonely {name: 'solo'})");

    let rs = run(
        &mut executor,
        "MATCH (n:Lonely) WHERE exists { MATCH (n)-->() RETURN true } \
         RETURN n.name AS name",
    );
    assert_eq!(rs.rows.len(), 0);
}

#[test]
fn exists_subquery_write_clauses_are_rejected_at_parse_time() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    for (write_clause, token) in [
        ("SET m.prop = 'fail'", "SET"),
        ("CREATE (m)-[:X]->()", "CREATE"),
        ("DELETE m", "DELETE"),
        ("MERGE (m)-[:X]->()", "MERGE"),
        ("REMOVE m.prop", "REMOVE"),
        ("FOREACH (x IN [1] | SET m.prop = x)", "FOREACH"),
    ] {
        let cypher =
            format!("MATCH (n) WHERE exists {{ MATCH (n)-->(m) {write_clause} }} RETURN n");
        let outcome = try_run(&mut executor, &cypher);
        let err = outcome.expect_err(&format!(
            "write clause `{write_clause}` must be rejected at parse time"
        ));
        assert!(
            err.to_string().contains("InvalidClauseComposition"),
            "expected InvalidClauseComposition for `{write_clause}`, got: {err}"
        );
        assert!(
            err.to_string().contains(token),
            "expected the `{token}` clause token in the error for `{write_clause}`, got: {err}"
        );
    }
}

#[test]
fn exists_subquery_write_clause_with_no_leading_match_is_rejected() {
    // A write clause is illegal inside `EXISTS { … }` whether or not it
    // follows a `MATCH` — the subquery-form trigger routes on any clause
    // keyword (`is_clause_boundary()`), not just `MATCH`, so a bare
    // `EXISTS { CREATE (x) }` must still reach the same
    // `InvalidClauseComposition` check.
    let (mut executor, _ctx) = create_isolated_test_executor();

    let outcome = try_run(
        &mut executor,
        "MATCH (n) WHERE exists { CREATE (x) } RETURN n",
    );
    let err = outcome.expect_err("EXISTS { CREATE (x) } (no leading MATCH) must be rejected");
    assert!(
        err.to_string().contains("InvalidClauseComposition"),
        "expected InvalidClauseComposition, got: {err}"
    );
    assert!(
        err.to_string().contains("CREATE"),
        "expected the CREATE clause token in the error, got: {err}"
    );
}

#[test]
fn exists_subquery_with_aggregation_filters_via_with_where() {
    // openCypher TCK `ExistentialSubquery2[2]`: only `a` has exactly 3
    // outgoing connections (`b`, `c`, `d`); `b` has 1 (`d`).
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n) WHERE exists { \
             MATCH (n)-->(m) \
             WITH n, count(*) AS numConnections \
             WHERE numConnections = 3 \
             RETURN true \
         } \
         RETURN n.prop AS prop",
    );

    let props: Vec<i64> = rs
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("prop is an integer"))
        .collect();
    assert_eq!(props, vec![1]);
}

#[test]
fn exists_subquery_aggregating_return_is_true_even_with_zero_matches() {
    // Pins the trailing-RETURN-drop's aggregate-free gate: `count(*)`
    // always yields exactly one row, even over zero input rows, so
    // dropping the `RETURN` (as if it were a cardinality-preserving
    // no-op) would wrongly flip this to `false`. Left in place, the real
    // pipeline runs and correctly reports one (non-empty) row.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n:A) WHERE exists { MATCH (m:NoSuchLabel) RETURN count(*) } \
         RETURN n.prop AS prop",
    );

    assert_eq!(
        rs.rows.len(),
        1,
        "count(*) over zero matches still yields one row -> EXISTS true"
    );
    assert_eq!(rs.rows[0].values[0], serde_json::json!(1));
}

#[test]
fn exists_subquery_return_distinct_still_reports_existence_correctly() {
    // `RETURN DISTINCT true` is a valid (if unusual) subquery RETURN —
    // DISTINCT only removes duplicate rows, never manufactures or drops
    // the underlying existence signal.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n) WHERE exists { MATCH (n)-->() RETURN DISTINCT true } \
         RETURN n.prop AS prop ORDER BY prop",
    );

    let props: Vec<i64> = rs
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("prop is an integer"))
        .collect();
    assert_eq!(props, vec![1, 1]);
}

#[test]
fn exists_subquery_limit_zero_after_return_forces_false() {
    // `LIMIT 0` parses as its own trailing clause after `RETURN true`,
    // which must disqualify the trailing-RETURN drop (`RETURN` is no
    // longer the *last* clause) — the full pipeline runs, `LIMIT`
    // truncates every row away, and EXISTS correctly reports `false`
    // despite a real match upstream.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n:A) WHERE exists { MATCH (n)-->() RETURN true LIMIT 0 } \
         RETURN n.prop AS prop",
    );

    assert_eq!(
        rs.rows.len(),
        0,
        "LIMIT 0 must suppress every row despite a real match"
    );
}

#[test]
fn exists_subquery_nested_full_in_full() {
    // openCypher TCK `ExistentialSubquery3[2]`: only `a` has two distinct
    // outgoing edges (needed to bind `l` and `m` without reusing the same
    // relationship — relationship isomorphism within one MATCH pattern).
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n) WHERE exists { \
             MATCH (m) WHERE exists { \
                 MATCH (l)<-[:R]-(n)-[:R]->(m) RETURN true \
             } \
             RETURN true \
         } \
         RETURN n.prop AS prop",
    );

    let props: Vec<i64> = rs
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("prop is an integer"))
        .collect();
    assert_eq!(props, vec![1]);
}

#[test]
fn exists_subquery_correlates_with_an_additional_outer_where_predicate() {
    // `a` and `b` both have `prop: 1`, but only `a` reaches a neighbour
    // with `prop: 2` (`c`); `b`'s only neighbour is `d` (`prop: 3`). The
    // outer `AND` proves the correlated inner subquery and the plain
    // outer predicate both apply to the *same* `n`, not independently.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n) WHERE n.prop = 1 AND exists { \
             MATCH (n)-->(m) WHERE m.prop = 2 RETURN true \
         } \
         RETURN n.prop AS prop",
    );

    let props: Vec<i64> = rs
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("prop is an integer"))
        .collect();
    assert_eq!(props, vec![1]);
}

#[test]
fn exists_subquery_inner_bindings_do_not_leak_to_outer_scope() {
    // `m` is bound only inside the EXISTS subquery's own `MATCH`; the
    // outer `RETURN m` never saw a `MATCH` (or any other binder) for `m`,
    // so it must read as NULL, not the correlated match's neighbour.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n:A) WHERE exists { MATCH (n)-->(m) RETURN true } \
         RETURN m AS leaked",
    );

    assert_eq!(rs.rows.len(), 1, "the outer MATCH (n:A) still finds `a`");
    assert!(
        rs.rows[0].values[0].is_null(),
        "expected NULL (no leak), got: {:?}",
        rs.rows[0].values[0]
    );
}

#[test]
fn exists_subquery_nested_pattern_in_full() {
    // openCypher TCK `ExistentialSubquery3[1]`: the outer subquery is the
    // full form (opens with `MATCH`); the inner nested `EXISTS` is the
    // abbreviated pattern-probe form (no `MATCH` keyword). Only `a`
    // reaches a neighbour whose `prop` matches its own (`a`-`b`, both 1).
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (n) WHERE exists { \
             MATCH (m) WHERE exists { \
                 (n)-[]->(m) WHERE n.prop = m.prop \
             } \
             RETURN true \
         } \
         RETURN n.prop AS prop",
    );

    let props: Vec<i64> = rs
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("prop is an integer"))
        .collect();
    assert_eq!(props, vec![1]);
}
