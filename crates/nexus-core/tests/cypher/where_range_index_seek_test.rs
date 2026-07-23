//! Range WHERE predicates (`>`, `>=`, `<`, `<=`) on an indexed property lift
//! to a `NodeIndexRangeSeek` (B-tree range scan) — but must return exactly the
//! same rows a full scan would, including the exclusive-bound handling for `>`
//! and `<`. See phase0_fix-where-clause-index-seek-extensions.

use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use std::collections::HashMap;

fn run(executor: &mut nexus_core::executor::Executor, cypher: &str) {
    executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        })
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
}

fn ages(executor: &nexus_core::executor::Executor, cypher: &str) -> Vec<i64> {
    executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        })
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"))
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().unwrap())
        .collect()
}

fn seeded() -> (
    nexus_core::executor::Executor,
    nexus_core::testing::TestContext,
) {
    let (mut executor, ctx) = create_isolated_test_executor();
    run(&mut executor, "CREATE INDEX age_idx FOR (n:P) ON (n.age)");
    run(
        &mut executor,
        "CREATE (:P {age: 10}), (:P {age: 20}), (:P {age: 30}), (:P {age: 40})",
    );
    (executor, ctx)
}

#[test]
fn range_seek_gt_excludes_the_threshold() {
    let (executor, _ctx) = seeded();
    let v = ages(
        &executor,
        "MATCH (n:P) WHERE n.age > 20 RETURN n.age AS age ORDER BY age",
    );
    assert_eq!(v, vec![30, 40], "> must exclude the threshold value");
}

#[test]
fn range_seek_ge_includes_the_threshold() {
    let (executor, _ctx) = seeded();
    let v = ages(
        &executor,
        "MATCH (n:P) WHERE n.age >= 20 RETURN n.age AS age ORDER BY age",
    );
    assert_eq!(v, vec![20, 30, 40], ">= must include the threshold value");
}

#[test]
fn range_seek_lt_excludes_the_threshold() {
    let (executor, _ctx) = seeded();
    let v = ages(
        &executor,
        "MATCH (n:P) WHERE n.age < 30 RETURN n.age AS age ORDER BY age",
    );
    assert_eq!(v, vec![10, 20], "< must exclude the threshold value");
}

#[test]
fn range_seek_le_includes_the_threshold() {
    let (executor, _ctx) = seeded();
    let v = ages(
        &executor,
        "MATCH (n:P) WHERE n.age <= 30 RETURN n.age AS age ORDER BY age",
    );
    assert_eq!(v, vec![10, 20, 30], "<= must include the threshold value");
}

#[test]
fn range_seek_mirrored_literal_on_left() {
    let (executor, _ctx) = seeded();
    // `20 < n.age` is `n.age > 20`.
    let v = ages(
        &executor,
        "MATCH (n:P) WHERE 20 < n.age RETURN n.age AS age ORDER BY age",
    );
    assert_eq!(v, vec![30, 40]);
}

#[test]
fn range_seek_combined_with_residual_upper_bound() {
    let (executor, _ctx) = seeded();
    // One bound lifts to the seek; the other stays a residual Filter. Result
    // must still be correct.
    let v = ages(
        &executor,
        "MATCH (n:P) WHERE n.age > 10 AND n.age < 40 RETURN n.age AS age ORDER BY age",
    );
    assert_eq!(v, vec![20, 30]);
}
