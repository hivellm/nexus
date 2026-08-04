//! A `WITH`-minted alias must bind for every downstream clause. Re-projecting
//! one used to return nulls — a silent data loss in a common clause shape.
//!
//! The planner inserted every `WITH` operator at the same index when no
//! `Project`/`Aggregate` sink existed yet (`last_unwind + 1`), so each `WITH`
//! landed BEFORE the one inserted for the preceding clause and the clause order
//! reversed. `UNWIND [5,1,4] AS i WITH i AS a WITH a` therefore ran `With(a)`
//! first — projecting `a` before anything bound it, and dropping `i` in the
//! process, so the `With(i AS a)` that followed had nothing left to read.
//! Renaming once was fine (one operator cannot be out of order) and so was
//! re-projecting without renaming (both operators project the same thing), which
//! is why the shape looked so specific.

use nexus_core::testing::setup_isolated_test_engine;

fn numbers(engine: &mut nexus_core::Engine, query: &str) -> Vec<i64> {
    engine
        .execute_cypher(query)
        .expect("query should execute")
        .rows
        .iter()
        .filter_map(|r| r.values.first().and_then(|v| v.as_i64()))
        .collect()
}

#[test]
fn a_renamed_unwind_variable_survives_a_second_with() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        numbers(
            &mut engine,
            "UNWIND [5,1,4] AS i WITH i AS a WITH a RETURN a"
        ),
        vec![5, 1, 4],
        "re-projecting a renamed variable must not lose its value"
    );
}

#[test]
fn a_renamed_unwind_variable_can_be_renamed_again() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        numbers(
            &mut engine,
            "UNWIND [5,1,4] AS i WITH i AS a WITH a AS b RETURN b"
        ),
        vec![5, 1, 4]
    );
}

#[test]
fn three_chained_renames_still_carry_the_value() {
    // The reversal got worse with each extra clause; three is the shape that
    // proves the fix is an ordering fix and not a two-clause special case.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        numbers(
            &mut engine,
            "UNWIND [5,1,4] AS i WITH i AS a WITH a AS b WITH b AS c RETURN c"
        ),
        vec![5, 1, 4]
    );
}

#[test]
fn renaming_once_still_works() {
    // Control: the single-WITH shape was never broken (one operator cannot be
    // mis-ordered) and must stay correct.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        numbers(&mut engine, "UNWIND [5,1,4] AS i WITH i AS a RETURN a"),
        vec![5, 1, 4]
    );
}

#[test]
fn re_projecting_without_renaming_still_works() {
    // Control: this shape hid the bug, because reversing two identical
    // projections changes nothing.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        numbers(&mut engine, "UNWIND [5,1,4] AS i WITH i WITH i RETURN i"),
        vec![5, 1, 4]
    );
}

#[test]
fn the_same_shape_over_a_match_source_still_works() {
    // Control for the other insertion branch: with a `Project`/`Aggregate` sink
    // present, each insertion pushes the sink right, so ordering was already
    // correct there and must remain so.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:P {x: 1}), (:P {x: 1}), (:P {x: 2})")
        .expect("setup");
    engine.refresh_executor().unwrap();

    let mut got = numbers(&mut engine, "MATCH (n:P) WITH n.x AS a WITH a RETURN a");
    got.sort_unstable();
    assert_eq!(got, vec![1, 1, 2]);
}

#[test]
fn a_where_attached_to_the_second_with_still_filters() {
    // The insertion carries the WITH's own WHERE with it; ordering must not
    // detach a predicate from the projection it belongs to.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        numbers(
            &mut engine,
            "UNWIND [5,1,4] AS i WITH i AS a WITH a WHERE a > 3 RETURN a"
        ),
        vec![5, 4]
    );
}
