//! Regression coverage for the bounded-allocation guards on Cypher
//! functions and operators: `range()`, `lpad`/`rpad`, and the
//! variable-length-path BFS depth cap. Each guard turns a query that
//! would previously OOM the process (or, for `range` with a huge span
//! and step, loop forever once `i += step` wrapped past i64::MAX in a
//! release build with overflow-checks off) into a bounded Cypher error.
//!
//! SAFETY: these tests only run against the FIXED code — the guards
//! reject BEFORE allocating, so the oversized cases return an error
//! instantly and never actually allocate gigabytes or loop.

use nexus_core::testing::setup_isolated_test_engine;

// ---------------------------------------------------------------------------
// range()
// ---------------------------------------------------------------------------

#[test]
fn range_rejects_element_count_over_cap_instead_of_oom() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // ~5 billion elements — far over the element cap. Must error, not OOM.
    let err = engine
        .execute_cypher("RETURN range(0, 5000000000)")
        .expect_err("an over-cap range must return a Cypher error, not allocate ~5B elements");
    let msg = format!("{err}");
    assert!(
        msg.contains("RANGE_TOO_LARGE") || msg.to_lowercase().contains("cap"),
        "error should name the range cap; got: {msg}"
    );
}

#[test]
fn range_with_huge_span_and_step_terminates_with_error_not_infinite_loop() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Pre-fix, `i += step` wraps negative once i passes i64::MAX and the
    // `while i <= end` loop runs forever in release. The count check now
    // rejects it up front (this returns immediately — if it hangs, the
    // fix regressed).
    let result = engine.execute_cypher("RETURN range(0, 9223372036854775807, 3)");
    assert!(
        result.is_err(),
        "a range whose element count exceeds the cap must return an error, not hang"
    );
}

#[test]
fn range_within_bounds_still_works() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine
        .execute_cypher("RETURN range(0, 100) AS r")
        .expect("a small range must still succeed");
    let v = &result.rows[0].values[0];
    let arr = v.as_array().expect("range returns an array");
    assert_eq!(arr.len(), 101, "range(0, 100) is inclusive => 101 elements");
}

// ---------------------------------------------------------------------------
// lpad / rpad
// ---------------------------------------------------------------------------

#[test]
fn lpad_rejects_target_length_over_cap_instead_of_oom() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("RETURN lpad('a', 9000000000, 'x')")
        .expect_err("an over-cap lpad must error, not allocate a multi-GB string");
    let msg = format!("{err}");
    assert!(
        msg.contains("PAD_TOO_LARGE") || msg.to_lowercase().contains("cap"),
        "error should name the pad cap; got: {msg}"
    );
}

#[test]
fn rpad_rejects_target_length_over_cap_instead_of_oom() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher("RETURN rpad('a', 9000000000, 'x')");
    assert!(result.is_err(), "an over-cap rpad must error, not OOM");
}

#[test]
fn lpad_within_bounds_still_works() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine
        .execute_cypher("RETURN lpad('a', 5, 'x') AS p")
        .expect("a small lpad must still succeed");
    assert_eq!(result.rows[0].values[0], "xxxxa");
}

// ---------------------------------------------------------------------------
// variable-length path depth cap
// ---------------------------------------------------------------------------

#[test]
fn unbounded_var_length_path_over_a_cycle_terminates() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // A 3-node cycle: A->B->C->A. Pre-fix, `[*]` set max_length to
    // usize::MAX and the BFS explored an unbounded number of hops; now
    // it is clamped to a bounded depth, so the query completes quickly
    // instead of hanging / exhausting memory. If this test does not
    // return promptly, the depth cap regressed.
    engine
        .execute_cypher(
            "CREATE (a:N {name:'a'})-[:R]->(b:N {name:'b'})-[:R]->(c:N {name:'c'})-[:R]->(a)",
        )
        .expect("seed cycle");
    let result = engine.execute_cypher("MATCH (a:N)-[*]->(b:N) RETURN count(*) AS c");
    assert!(
        result.is_ok(),
        "unbounded var-length path over a cycle must terminate (bounded depth), got {result:?}"
    );
}

#[test]
fn bounded_var_length_path_is_unaffected_by_the_depth_cap() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // A short chain a->b->c; `[*1..2]` from `a` reaches `b` (1 hop) and
    // `c` (2 hops) — the bounded quantifier stays well under the cap and
    // returns the expected targets.
    engine
        .execute_cypher("CREATE (a:C {name:'a'})-[:R]->(b:C {name:'b'})-[:R]->(c:C {name:'c'})")
        .expect("seed chain");
    let result = engine
        .execute_cypher(
            "MATCH (a:C {name:'a'})-[:R*1..2]->(x:C) RETURN x.name AS name ORDER BY name",
        )
        .expect("bounded var-length path must succeed");
    let names: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["b", "c"], "[*1..2] from a reaches b and c");
}
