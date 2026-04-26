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
fn call_subquery_in_transactions_runs_unwind_create_to_completion() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(
        &mut executor,
        "UNWIND range(1, 100) AS i \
         CALL { WITH i CREATE (:Tmp {i: i}) } \
         IN TRANSACTIONS OF 10 ROWS \
         RETURN count(*) AS c",
    );
    let counts = run(&mut executor, "MATCH (t:Tmp) RETURN count(t) AS c");
    assert_eq!(counts.rows[0].values[0], serde_json::json!(100));
}

#[test]
fn call_subquery_in_transactions_report_status_emits_one_row_per_batch() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    let rs = run(
        &mut executor,
        "UNWIND range(1, 25) AS i \
         CALL { WITH i CREATE (:Tmp {i: i}) } \
         IN TRANSACTIONS OF 10 ROWS REPORT STATUS AS s \
         RETURN s.committed AS committed, s.rowsProcessed AS rows",
    );
    assert_eq!(rs.rows.len(), 3, "three batches → three status rows");
    let row_counts: Vec<_> = rs.rows.iter().map(|r| r.values[1].clone()).collect();
    assert_eq!(row_counts[0], serde_json::json!(10));
    assert_eq!(row_counts[1], serde_json::json!(10));
    assert_eq!(row_counts[2], serde_json::json!(5));
    for r in &rs.rows {
        assert_eq!(r.values[0], serde_json::json!(true));
    }
}

#[test]
fn call_subquery_in_transactions_default_fail_propagates_inner_error() {
    // Inner CALL invokes a procedure that doesn't exist — the
    // executor surfaces "Procedure 'nonexistent.proc' not found",
    // which bubbles through the default ON ERROR FAIL policy.
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(
        &mut executor,
        "UNWIND range(1, 5) AS i \
         CALL { CALL nonexistent.proc() YIELD x RETURN x } \
         IN TRANSACTIONS OF 2 ROWS \
         RETURN count(*) AS c",
    );
    assert!(
        outcome.is_err(),
        "default ON ERROR FAIL must surface inner failures"
    );
}

#[test]
fn call_subquery_in_transactions_continue_records_failed_batches() {
    // ON ERROR CONTINUE: each batch fails identically (procedure
    // not found). Three batches of 2 rows → three status rows
    // marked `committed = false`.
    let (mut executor, _ctx) = create_isolated_test_executor();
    let rs = run(
        &mut executor,
        "UNWIND range(1, 6) AS i \
         CALL { CALL nonexistent.proc() YIELD x } \
         IN TRANSACTIONS OF 2 ROWS REPORT STATUS AS s \
         ON ERROR CONTINUE \
         RETURN s.committed AS committed, s.err AS err",
    );
    assert_eq!(rs.rows.len(), 3);
    for r in &rs.rows {
        assert_eq!(r.values[0], serde_json::json!(false));
        assert!(matches!(r.values[1], serde_json::Value::String(_)));
    }
}

#[test]
fn call_subquery_in_transactions_break_stops_on_first_failure() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    let rs = run(
        &mut executor,
        "UNWIND range(1, 6) AS i \
         CALL { CALL nonexistent.proc() YIELD x } \
         IN TRANSACTIONS OF 2 ROWS REPORT STATUS AS s \
         ON ERROR BREAK \
         RETURN s.committed AS committed",
    );
    assert_eq!(rs.rows.len(), 1, "BREAK halts after the first failed batch");
    assert_eq!(rs.rows[0].values[0], serde_json::json!(false));
}

#[test]
fn call_subquery_in_transactions_retry_escalates_to_fail_when_exhausted() {
    // Deterministic failure (procedure not found) → retries cannot
    // succeed.
    let (mut executor, _ctx) = create_isolated_test_executor();
    let outcome = try_run(
        &mut executor,
        "UNWIND range(1, 4) AS i \
         CALL { CALL nonexistent.proc() YIELD x RETURN x } \
         IN TRANSACTIONS OF 2 ROWS \
         ON ERROR RETRY 2 \
         RETURN count(*) AS c",
    );
    assert!(
        outcome.is_err(),
        "RETRY n with deterministic failure must escalate to FAIL after n attempts"
    );
}

#[test]
fn call_subquery_write_inner_persists_anonymous_create() {
    // CALL { CREATE (:Audit) } with no preceding outer clause runs the
    // dispatch path against an empty driver row; the dispatch arm
    // detects the empty-scope case and routes to
    // `execute_create_pattern_with_variables`, which handles
    // anonymous nodes correctly (named-variable CREATE goes the same
    // path).
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(&mut executor, "CALL { CREATE (:Audit {ts: 'now'}) }");
    let counts = run(&mut executor, "MATCH (a:Audit) RETURN count(a) AS c");
    assert_eq!(counts.rows[0].values[0], serde_json::json!(1));
}

#[test]
fn call_subquery_write_inner_persists_unwind_driven_create() {
    // UNWIND-driven inner CREATE — the dispatch arm sees a populated
    // row scope (the outer's `i` projection) and routes to
    // `execute_create_with_context`, which now handles anonymous
    // nodes AND resolves property expressions like `{x: i}` against
    // the current row via the row-aware projection evaluator.
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(
        &mut executor,
        "UNWIND range(1, 5) AS i \
         CALL { WITH i CREATE (:T {x: i}) } \
         RETURN count(*) AS c",
    );
    let counts = run(&mut executor, "MATCH (t:T) RETURN count(t) AS c");
    assert_eq!(counts.rows[0].values[0], serde_json::json!(5));
}

#[test]
fn call_subquery_in_transactions_on_error_combinations_are_accepted() {
    // ON ERROR + IN TRANSACTIONS combinations now run for real.
    // Read-only inner has no failure → CONTINUE leaves the run
    // identical to a vanilla in-transactions path.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_three_people(&mut executor);
    let rs = run(
        &mut executor,
        "MATCH (p:Person) \
         CALL { WITH p CREATE (a:Audit {name: p.name}) } \
         IN TRANSACTIONS OF 5 ROWS REPORT STATUS AS s ON ERROR CONTINUE \
         RETURN s.committed AS committed",
    );
    assert_eq!(rs.rows.len(), 1, "single batch covers all 3 people");
    assert_eq!(rs.rows[0].values[0], serde_json::json!(true));
    let counts = run(&mut executor, "MATCH (a:Audit) RETURN count(a) AS c");
    assert_eq!(counts.rows[0].values[0], serde_json::json!(3));
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
