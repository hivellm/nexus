//! `SKIP` / `LIMIT` semantics.
//!
//! Four distinct behaviours are pinned here, each of which was wrong in a
//! different way:
//!
//! 1. `LIMIT 0` must return zero rows. The dispatcher's final assembly step
//!    treats an empty result set as "no operator produced rows" and falls
//!    back to the pre-projection rows, which resurrected everything the
//!    limit had just discarded.
//! 2. `SKIP $n` / `LIMIT $n` must read the request's parameters. The planner
//!    matched an integer *literal* with no `else` branch, so a parameterised
//!    count was dropped in silence and the query answered as if the clause
//!    had never been written.
//! 3. A statically-illegal count must raise the spec's error kind rather
//!    than being ignored: negative → `NegativeIntegerArgument`, non-integer
//!    literal → `InvalidArgumentType`, row-dependent → `NonConstantExpression`.
//! 4. A `SKIP`/`LIMIT` attached to a `WITH` must cut the stream *there*, so
//!    the clause that follows — an aggregation included — sees only the rows
//!    that survived. The planner collected the counts into one pair of
//!    query-wide slots and applied them at the very end of the pipeline
//!    instead.

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Engine seeded with five `Person` nodes, ages 10, 20, 30, 40, 50.
fn engine_with_people() -> nexus_core::Engine {
    let (mut engine, ctx) = setup_isolated_test_engine().unwrap();
    for (name, age) in [
        ("Alice", 10),
        ("Bob", 20),
        ("Carol", 30),
        ("Dave", 40),
        ("Erin", 50),
    ] {
        engine
            .execute_cypher(&format!(
                "CREATE (:SkipLimitPerson {{name: '{name}', age: {age}}})"
            ))
            .unwrap();
    }
    std::mem::forget(ctx);
    engine
}

fn ints(rs: &nexus_core::executor::ResultSet) -> Vec<i64> {
    rs.rows
        .iter()
        .map(|r| match &r.values[0] {
            Value::Number(n) => n.as_i64().unwrap_or_else(|| panic!("not an i64: {n}")),
            other => panic!("expected a number, got {other:?}"),
        })
        .collect()
}

// ── 1. LIMIT 0 ─────────────────────────────────────────────────────────

#[test]
fn limit_zero_over_a_match_returns_no_rows() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("MATCH (n:SkipLimitPerson) RETURN n.name LIMIT 0")
        .unwrap();
    assert_eq!(rs.rows.len(), 0, "LIMIT 0 must discard every row");
}

#[test]
fn limit_zero_over_unwind_returns_no_rows() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3] AS x RETURN x LIMIT 0")
        .unwrap();
    assert_eq!(rs.rows.len(), 0, "LIMIT 0 must discard every row");
}

#[test]
fn skip_past_the_end_returns_no_rows() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("MATCH (n:SkipLimitPerson) RETURN n.name SKIP 100")
        .unwrap();
    assert_eq!(
        rs.rows.len(),
        0,
        "a SKIP past the last row must leave nothing"
    );
}

// ── 2. Parameters ──────────────────────────────────────────────────────

#[test]
fn limit_reads_its_count_from_a_parameter() {
    let mut engine = engine_with_people();
    let mut params = HashMap::new();
    params.insert("n".to_string(), json!(2));
    let rs = engine
        .execute_cypher_with_params(
            "MATCH (p:SkipLimitPerson) RETURN p.age ORDER BY p.age LIMIT $n",
            params,
        )
        .unwrap();
    assert_eq!(ints(&rs), vec![10, 20]);
}

#[test]
fn skip_reads_its_count_from_a_parameter() {
    let mut engine = engine_with_people();
    let mut params = HashMap::new();
    params.insert("n".to_string(), json!(3));
    let rs = engine
        .execute_cypher_with_params(
            "MATCH (p:SkipLimitPerson) RETURN p.age ORDER BY p.age SKIP $n",
            params,
        )
        .unwrap();
    assert_eq!(ints(&rs), vec![40, 50]);
}

#[test]
fn skip_and_limit_parameters_compose() {
    let mut engine = engine_with_people();
    let mut params = HashMap::new();
    params.insert("s".to_string(), json!(1));
    params.insert("l".to_string(), json!(2));
    let rs = engine
        .execute_cypher_with_params(
            "MATCH (p:SkipLimitPerson) RETURN p.age ORDER BY p.age SKIP $s LIMIT $l",
            params,
        )
        .unwrap();
    assert_eq!(ints(&rs), vec![20, 30]);
}

#[test]
fn a_zero_valued_limit_parameter_returns_no_rows() {
    let mut engine = engine_with_people();
    let mut params = HashMap::new();
    params.insert("n".to_string(), json!(0));
    let rs = engine
        .execute_cypher_with_params("MATCH (p:SkipLimitPerson) RETURN p.age LIMIT $n", params)
        .unwrap();
    assert_eq!(
        rs.rows.len(),
        0,
        "LIMIT $n with n = 0 must discard everything"
    );
}

// ── 3. Illegal arguments ───────────────────────────────────────────────

fn error_of(query: &str) -> String {
    let mut engine = engine_with_people();
    match engine.execute_cypher(query) {
        Ok(rs) => panic!("expected `{query}` to fail, got {} rows", rs.rows.len()),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_negative_count_is_rejected() {
    for q in [
        "MATCH (p:SkipLimitPerson) RETURN p SKIP -1",
        "MATCH (p:SkipLimitPerson) RETURN p LIMIT -1",
    ] {
        let msg = error_of(q);
        assert!(
            msg.contains("NegativeIntegerArgument"),
            "`{q}` raised the wrong error: {msg}"
        );
    }
}

#[test]
fn a_non_integer_literal_count_is_rejected() {
    for q in [
        "MATCH (p:SkipLimitPerson) RETURN p SKIP 1.5",
        "MATCH (p:SkipLimitPerson) RETURN p LIMIT 1.5",
        "MATCH (p:SkipLimitPerson) RETURN p LIMIT 'x'",
        "MATCH (p:SkipLimitPerson) RETURN p LIMIT true",
    ] {
        let msg = error_of(q);
        assert!(
            msg.contains("InvalidArgumentType"),
            "`{q}` raised the wrong error: {msg}"
        );
    }
}

#[test]
fn a_row_dependent_count_is_rejected() {
    for q in [
        "MATCH (p:SkipLimitPerson) RETURN p SKIP p.age",
        "MATCH (p:SkipLimitPerson) RETURN p LIMIT p.age",
    ] {
        let msg = error_of(q);
        assert!(
            msg.contains("NonConstantExpression"),
            "`{q}` raised the wrong error: {msg}"
        );
    }
}

// ── 4. SKIP / LIMIT attached to WITH ───────────────────────────────────

#[test]
fn with_limit_cuts_the_stream_before_the_next_clause() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3, 4, 5] AS x WITH x LIMIT 2 RETURN x")
        .unwrap();
    assert_eq!(ints(&rs), vec![1, 2]);
}

#[test]
fn with_skip_cuts_the_stream_before_the_next_clause() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3, 4, 5] AS x WITH x SKIP 3 RETURN x")
        .unwrap();
    assert_eq!(ints(&rs), vec![4, 5]);
}

#[test]
fn an_aggregation_after_with_limit_sees_only_the_surviving_rows() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3, 4, 5] AS x WITH x LIMIT 2 RETURN count(x)")
        .unwrap();
    assert_eq!(
        ints(&rs),
        vec![2],
        "count(x) must count the limited stream, not the whole input"
    );
}

#[test]
fn an_aggregation_after_with_skip_sees_only_the_surviving_rows() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3, 4, 5] AS x WITH x SKIP 3 RETURN sum(x)")
        .unwrap();
    assert_eq!(ints(&rs), vec![9], "sum must see only 4 and 5");
}

#[test]
fn with_order_by_limit_selects_the_smallest_rows() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher(
            "MATCH (p:SkipLimitPerson) WITH p ORDER BY p.age LIMIT 2 RETURN p.age ORDER BY p.age",
        )
        .unwrap();
    assert_eq!(ints(&rs), vec![10, 20]);
}

#[test]
fn a_final_limit_after_a_with_limit_applies_on_top_of_it() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3, 4, 5] AS x WITH x LIMIT 4 RETURN x LIMIT 2")
        .unwrap();
    assert_eq!(ints(&rs), vec![1, 2]);
}

#[test]
fn a_tail_on_the_second_of_two_chained_withs_still_binds_to_it() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher(
            "MATCH (p:SkipLimitPerson) WITH p, p.age AS age WITH p, age \
             ORDER BY age LIMIT 3 RETURN age",
        )
        .unwrap();
    assert_eq!(
        ints(&rs),
        vec![10, 20, 30],
        "the tail belongs to the WITH it follows, not to the first one"
    );
}

#[test]
fn a_with_tail_sorts_by_a_key_the_with_does_not_project() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("MATCH (p:SkipLimitPerson) WITH p ORDER BY p.age DESC LIMIT 2 RETURN p.age")
        .unwrap();
    assert_eq!(
        ints(&rs),
        vec![50, 40],
        "p.age is not projected by the WITH but must still order it"
    );
}

#[test]
fn an_aggregating_with_tail_sorts_by_the_grouping_key_it_projects() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher(
            "MATCH (p:SkipLimitPerson) WITH p.age AS age, count(*) AS cnt \
             ORDER BY p.age ASC LIMIT 1 RETURN age",
        )
        .unwrap();
    assert_eq!(
        ints(&rs),
        vec![10],
        "ORDER BY names the grouping expression the aggregation projects as `age`"
    );
}

#[test]
fn a_with_tail_sort_key_reads_the_projected_value_not_the_shadowed_one() {
    let mut engine = engine_with_people();
    // `x` is rebound by the second WITH; the sort key must see the new value
    // (age % 25 → 10, 20, 5, 15, 0), ordered descending by `x * -1` ⇒ 0, 5, 10.
    let rs = engine
        .execute_cypher(
            "MATCH (p:SkipLimitPerson) WITH p.age AS x WITH x % 25 AS x \
             ORDER BY x * -1 LIMIT 3 RETURN x",
        )
        .unwrap();
    assert_eq!(
        ints(&rs),
        vec![20, 15, 10],
        "the sort key must read the WITH's own `x`, not the one it shadowed"
    );
}

#[test]
fn a_with_limit_bounds_a_later_wider_limit() {
    let mut engine = engine_with_people();
    let rs = engine
        .execute_cypher("UNWIND [1, 2, 3, 4, 5] AS x WITH x LIMIT 2 RETURN x LIMIT 4")
        .unwrap();
    assert_eq!(
        ints(&rs),
        vec![1, 2],
        "the WITH limit is the tighter of the two and must survive"
    );
}
