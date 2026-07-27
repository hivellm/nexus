//! `SKIP` on pattern-driven (`MATCH ...`) Cypher queries.
//!
//! Root cause: `QueryPlanner::plan_execution_strategy` — the operator
//! lowering pass for MATCH-driven queries — never turned a collected
//! `skip_count` into `Operator::Skip`. Only the no-pattern branches
//! (`CALL ... YIELD ...` / bare `RETURN`, see `test_call_procedures.rs`)
//! and the UNION post-processing branch consumed `skip_count`; the latter
//! didn't consume it either, it just discarded the clause outright. A
//! plain `MATCH (n) RETURN n.v AS v ORDER BY v SKIP 2` silently ignored
//! SKIP and returned every row. See
//! `crates/nexus-core/src/executor/planner/queries/strategy.rs` and
//! `crates/nexus-core/src/executor/planner/queries/planner_core.rs`.

use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;

fn seed_five_nodes(executor: &mut nexus_core::executor::Executor) {
    let create_query = Query {
        cypher: "CREATE (:N {v:1}), (:N {v:2}), (:N {v:3}), (:N {v:4}), (:N {v:5})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();
}

fn values(executor: &nexus_core::executor::Executor, cypher: &str) -> Vec<i64> {
    let query = Query {
        cypher: cypher.to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor
        .execute(&query)
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
    result
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().unwrap())
        .collect()
}

#[test]
fn test_pattern_skip_after_order_by() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(&executor, "MATCH (n:N) RETURN n.v AS v ORDER BY v SKIP 2");
    assert_eq!(
        v,
        vec![3, 4, 5],
        "SKIP on a MATCH-driven query must drop the leading N sorted rows, \
         not be silently ignored"
    );
}

#[test]
fn test_pattern_skip_and_limit_order() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:N) RETURN n.v AS v ORDER BY v SKIP 1 LIMIT 2",
    );
    assert_eq!(
        v,
        vec![2, 3],
        "SKIP + LIMIT together must apply standard ORDER BY, SKIP, LIMIT \
         pipeline order over the sorted set"
    );
}

#[test]
fn test_pattern_skip_descending() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:N) RETURN n.v AS v ORDER BY v DESC SKIP 1",
    );
    assert_eq!(
        v,
        vec![4, 3, 2, 1],
        "SKIP must apply after descending ORDER BY too"
    );
}

#[test]
fn test_pattern_skip_zero_is_a_noop() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(&executor, "MATCH (n:N) RETURN n.v AS v ORDER BY v SKIP 0");
    assert_eq!(v, vec![1, 2, 3, 4, 5], "SKIP 0 must not drop any rows");
}

#[test]
fn test_pattern_order_by_without_skip_control() {
    // Regression lock: plain ORDER BY (no SKIP at all) on a MATCH-driven
    // query must keep returning every row, unaffected by the SKIP fix.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(&executor, "MATCH (n:N) RETURN n.v AS v ORDER BY v");
    assert_eq!(v, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_pattern_skip_with_aggregation() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:N) RETURN n.v AS v, count(*) AS c ORDER BY v SKIP 2",
    );
    assert_eq!(
        v,
        vec![3, 4, 5],
        "SKIP must apply after an aggregation projection too"
    );
}

#[test]
fn test_union_skip_after_order_by() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:N) WHERE n.v <= 3 RETURN n.v AS v \
         UNION MATCH (n:N) WHERE n.v > 3 RETURN n.v AS v \
         ORDER BY v SKIP 2",
    );
    assert_eq!(
        v,
        vec![3, 4, 5],
        "SKIP after UNION (and post-UNION ORDER BY) must drop the leading \
         N sorted rows of the combined result, not be silently discarded"
    );
}

#[test]
fn test_with_pipeline_skip_after_order_by() {
    // SKIP attached to a WITH (not the final RETURN) still flows through
    // the same collected `order_by_clause`/`skip_count` and the same
    // pattern-driven `plan_execution_strategy` path fixed above.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_five_nodes(&mut executor);

    let v = values(
        &executor,
        "MATCH (n:N) WITH n.v AS v ORDER BY v SKIP 2 RETURN v",
    );
    assert_eq!(
        v,
        vec![3, 4, 5],
        "SKIP on a WITH clause must drop the leading N sorted rows"
    );
}
