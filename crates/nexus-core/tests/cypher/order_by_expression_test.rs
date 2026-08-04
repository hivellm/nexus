//! `ORDER BY` over a key the `RETURN` does not project, and the cross-type total
//! order.
//!
//! Both were silent wrong answers rather than errors. `Sort` runs after the
//! projection and reads result-set columns by NAME, so a key the projection drops
//! simply was not there — and the unresolved key was skipped, leaving the rows in
//! whatever order they arrived. Mixed-type keys, meanwhile, fell through to a
//! stringified comparison, which put `1.5` before `'text'`.

use nexus_core::testing::setup_isolated_test_engine;

/// Column 0 of every row, rendered compactly. Entities collapse to a tag so the
/// assertions read as the type order they are testing.
fn column(engine: &mut nexus_core::Engine, setup: &[&str], query: &str) -> Vec<String> {
    for s in setup {
        engine.execute_cypher(s).expect("setup should succeed");
        engine.refresh_executor().unwrap();
    }
    engine
        .execute_cypher(query)
        .expect("query should execute")
        .rows
        .iter()
        .map(|r| match &r.values[0] {
            serde_json::Value::Object(m) => {
                if m.contains_key("nodes") && m.contains_key("relationships") {
                    "path".to_string()
                } else if m.contains_key("_nexus_rel_type") {
                    "rel".to_string()
                } else if m.contains_key("_nexus_id") {
                    "node".to_string()
                } else {
                    "map".to_string()
                }
            }
            other => other.to_string(),
        })
        .collect()
}

// ── Cross-type total order ───────────────────────────────────────────────────

/// openCypher TCK `clauses/return-orderby/ReturnOrderBy1.feature` [11] pins the
/// order as `MAP < NODE < RELATIONSHIP < LIST < PATH < STRING < BOOLEAN <
/// NUMBER < NaN < null`. `NaN` has no representation here (the value type is
/// `serde_json::Number`, which cannot hold a non-finite float — tracked by
/// `phase21_tck-non-finite-floats`), so it is absent from the fixture.
#[test]
fn order_by_ranks_distinct_types_in_the_specified_order() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let got = column(
        &mut engine,
        &["CREATE (:N)-[:REL]->()"],
        "MATCH (n:N)-[r:REL]->() \
         UNWIND [n, r, 1.5, ['list'], 'text', null, false, {a: 'map'}] AS types \
         RETURN types ORDER BY types",
    );
    assert_eq!(
        got,
        vec![
            "map",
            "node",
            "rel",
            "[\"list\"]",
            "\"text\"",
            "false",
            "1.5",
            "null"
        ],
        "type order must follow the spec, not a stringified comparison"
    );
}

/// `DESC` is the exact reverse of the ascending order, `null` included — so
/// `null` comes FIRST descending. Same TCK source, scenario [12].
#[test]
fn order_by_desc_reverses_the_type_order_including_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let got = column(
        &mut engine,
        &["CREATE (:N)-[:REL]->()"],
        "MATCH (n:N)-[r:REL]->() \
         UNWIND [n, r, 1.5, ['list'], 'text', null, false, {a: 'map'}] AS types \
         RETURN types ORDER BY types DESC",
    );
    assert_eq!(
        got,
        vec![
            "null",
            "1.5",
            "false",
            "\"text\"",
            "[\"list\"]",
            "rel",
            "node",
            "map"
        ]
    );
}

#[test]
fn a_string_sorts_before_a_boolean_which_sorts_before_a_number() {
    // The scalar core of the rule, and the one a stringified comparison got
    // backwards in both places: `1.5` < `'text'` by spelling, and `false` vs a
    // number by the letters of "false".
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let got = column(
        &mut engine,
        &[],
        "UNWIND [1.5, 'text', false, null] AS v RETURN v ORDER BY v",
    );
    assert_eq!(got, vec!["\"text\"", "false", "1.5", "null"]);
}

#[test]
fn same_type_ordering_is_unchanged() {
    // Control: introducing a type rank must not disturb ordering WITHIN a type.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        column(
            &mut engine,
            &[],
            "UNWIND [3, 1, 2] AS v RETURN v ORDER BY v"
        ),
        vec!["1", "2", "3"]
    );
    let (mut engine2, _ctx2) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        column(
            &mut engine2,
            &[],
            "UNWIND ['c', 'a', 'b'] AS v RETURN v ORDER BY v"
        ),
        vec!["\"a\"", "\"b\"", "\"c\""]
    );
}

// ── A sort key the RETURN does not project ───────────────────────────────────

#[test]
fn order_by_a_property_the_return_does_not_project() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let got = column(
        &mut engine,
        &["CREATE (:P {name: 'Alice', age: 30}), (:P {name: 'Bob', age: 20})"],
        "MATCH (n:P) RETURN n.name ORDER BY n.age",
    );
    assert_eq!(
        got,
        vec!["\"Bob\"", "\"Alice\""],
        "sorting by an unprojected key must work, not silently no-op"
    );
}

#[test]
fn the_hidden_sort_key_column_never_reaches_the_client() {
    // The key is carried through the projection to reach `Sort`; it must be
    // dropped again, or the caller sees a `__order_by_key_*` column it never
    // asked for.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:P {name: 'Alice', age: 30}), (:P {name: 'Bob', age: 20})")
        .expect("setup");
    engine.refresh_executor().unwrap();

    let rs = engine
        .execute_cypher("MATCH (n:P) RETURN n.name ORDER BY n.age")
        .expect("query");
    assert_eq!(rs.columns, vec!["n.name".to_string()], "exactly one column");
    for row in &rs.rows {
        assert_eq!(row.values.len(), 1, "no extra value per row");
    }
}

#[test]
fn order_by_an_expression_over_a_projected_variable() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let got = column(
        &mut engine,
        &[],
        "UNWIND [3, 1, 2] AS v RETURN v ORDER BY v * -1",
    );
    assert_eq!(
        got,
        vec!["3", "2", "1"],
        "the sort expression must be evaluated, not skipped"
    );
}

#[test]
fn order_by_a_projected_alias_still_works() {
    // Control for the resolution path that already worked: an ORDER BY naming an
    // alias must not be turned into a hidden column.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let got = column(
        &mut engine,
        &[],
        "UNWIND [3, 1, 2] AS v RETURN v AS x ORDER BY x DESC",
    );
    assert_eq!(got, vec!["3", "2", "1"]);
}

#[test]
fn order_by_a_projected_expression_still_works() {
    // Control: the key IS projected, so it resolves to that column rather than
    // being projected a second time under a hidden alias.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:P {name: 'Alice', age: 30}), (:P {name: 'Bob', age: 20})")
        .expect("setup");
    engine.refresh_executor().unwrap();

    let rs = engine
        .execute_cypher("MATCH (n:P) RETURN n.age ORDER BY n.age")
        .expect("query");
    assert_eq!(rs.columns, vec!["n.age".to_string()]);
    let ages: Vec<i64> = rs
        .rows
        .iter()
        .filter_map(|r| r.values[0].as_i64())
        .collect();
    assert_eq!(ages, vec![20, 30]);
}
