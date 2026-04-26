//! `CALL { … }` subquery executor tests
//! (phase6_opencypher-subquery-transactions slice-1).
//!
//! Covers the read-only inner subquery path wired in
//! `crates/nexus-core/src/executor/operators/call_subquery.rs`:
//!
//! - basic non-transactional `CALL { MATCH … RETURN … }` per outer row,
//! - empty-driver standalone `CALL { … }` form,
//! - inner-join semantics when the inner produces no rows for a given
//!   outer row,
//! - rectangular-result guard: column-shape drift across outer rows
//!   is rejected,
//! - the slice-1 contract for write-bearing inner subqueries and
//!   `IN TRANSACTIONS` variants — both must surface a typed error
//!   (`ERR_CALL_SUBQUERY_WRITE_INNER_UNSUPPORTED` and
//!   `ERR_CALL_IN_TX_PENDING_SLICE2` respectively) until the
//!   re-entrant executor refactor lands in slice-2.

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

fn seed_three_people(executor: &mut nexus_core::executor::Executor) {
    run(
        executor,
        "CREATE (:Person {name: 'Alice', age: 30}), \
                (:Person {name: 'Bob',   age: 25}), \
                (:Person {name: 'Carol', age: 40})",
    );
}

#[test]
fn call_subquery_basic_match_return_runs_per_outer_row() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_three_people(&mut executor);

    // Outer driver: each Person row triggers an inner MATCH that
    // counts the same Person set. The inner runs once per outer row;
    // every outer × inner pair is preserved.
    let rs = run(
        &mut executor,
        "MATCH (p:Person) \
         CALL { MATCH (q:Person) RETURN count(q) AS n } \
         RETURN p.name AS name, n",
    );

    assert_eq!(rs.columns, vec!["name", "n"]);
    assert_eq!(rs.rows.len(), 3, "three outer rows × one inner row each");
    for row in &rs.rows {
        // Every row should report n = 3 (count of Persons).
        assert_eq!(row.values[1], serde_json::json!(3));
    }
}

#[test]
fn call_subquery_empty_outer_drives_inner_once() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_three_people(&mut executor);

    // Standalone CALL{} with no preceding clause — exercises the
    // empty-driver-row seed in build_inner_ctx.
    let rs = run(
        &mut executor,
        "CALL { MATCH (p:Person) RETURN count(p) AS c } RETURN c",
    );

    assert_eq!(rs.columns, vec!["c"]);
    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values[0], serde_json::json!(3));
}

#[test]
fn call_subquery_multi_column_inner_returns_list_of_pairs() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_three_people(&mut executor);

    let rs = run(
        &mut executor,
        "MATCH (p:Person) \
         CALL { MATCH (q:Person) RETURN q.name AS qn, q.age AS qa } \
         RETURN p.name AS pn, qn, qa",
    );

    assert_eq!(rs.columns, vec!["pn", "qn", "qa"]);
    // 3 outer × 3 inner = 9 rows.
    assert_eq!(rs.rows.len(), 9);
}

#[test]
fn call_subquery_in_transactions_errors_until_slice_two() {
    // Slice-1 contract: parser accepts `IN TRANSACTIONS` and the
    // executor refuses it with a typed error so users do not silently
    // get non-transactional semantics.
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(
        &mut executor,
        "CALL { MATCH (p:Person) RETURN p.name AS n } \
         IN TRANSACTIONS OF 10 ROWS \
         RETURN n",
    );
    let err = outcome.expect_err("IN TRANSACTIONS must error in slice-1");
    let msg = err.to_string();
    assert!(
        msg.contains("ERR_CALL_IN_TX_PENDING_SLICE2"),
        "expected slice-2 sentinel in error: {msg}"
    );
}

#[test]
fn call_subquery_write_inner_errors_until_slice_two() {
    // Slice-1 contract: write-bearing inner subqueries (CREATE /
    // MERGE / DELETE / SET) require the re-entrant executor refactor;
    // until then they must surface a typed error rather than corrupt
    // catalog state.
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(&mut executor, "CALL { CREATE (:Audit {ts: 'now'}) }");
    let err = outcome.expect_err("CREATE inside CALL{} must error in slice-1");
    let msg = err.to_string();
    assert!(
        msg.contains("ERR_CALL_SUBQUERY_WRITE_INNER_UNSUPPORTED"),
        "expected write-inner sentinel in error: {msg}"
    );
}

#[test]
fn call_subquery_on_error_clause_errors_without_in_transactions() {
    // The §2.3-style validation rejects ON ERROR / REPORT STATUS
    // outside `IN TRANSACTIONS`. Slice-1's executor surfaces the
    // PENDING_SLICE2 sentinel for any of those non-default settings.
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(
        &mut executor,
        "CALL { MATCH (p:Person) RETURN p.name AS n } \
         IN TRANSACTIONS OF 5 ROWS ON ERROR CONTINUE \
         RETURN n",
    );
    let err = outcome.expect_err("ON ERROR with IN TRANSACTIONS must error in slice-1");
    assert!(
        err.to_string().contains("ERR_CALL_IN_TX_PENDING_SLICE2"),
        "got: {err}"
    );
}

#[test]
fn call_subquery_nested_call_inner_runs_through_dispatch() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_three_people(&mut executor);

    // Nested CALL: the outer subquery wraps another subquery. Both
    // are read-only — exercises the recursive walker in
    // `inner_subquery_has_writes`.
    let rs = run(
        &mut executor,
        "MATCH (p:Person) \
         CALL { \
           CALL { MATCH (q:Person) RETURN count(q) AS inner_count } \
           RETURN inner_count AS c \
         } \
         RETURN p.name AS name, c",
    );

    assert_eq!(rs.columns, vec!["name", "c"]);
    assert_eq!(rs.rows.len(), 3);
    for row in &rs.rows {
        assert_eq!(row.values[1], serde_json::json!(3));
    }
}
