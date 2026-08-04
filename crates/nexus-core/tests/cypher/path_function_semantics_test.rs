//! Path-function semantics: `nodes()` / `relationships()` / `length()` over
//! paths, null paths and wrongly-typed arguments. Mirrors the openCypher TCK
//! `expressions/path` feature files (Path1 / Path2 / Path3).

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

#[test]
fn nodes_on_a_null_path_is_null_not_an_empty_list() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine
        .execute_cypher(
            "WITH null AS a OPTIONAL MATCH p = (a)-[r]->() RETURN nodes(p), nodes(null)",
        )
        .expect("query should execute");

    assert_eq!(
        result.columns,
        vec!["nodes(p)".to_string(), "nodes(null)".to_string()],
        "unaliased column names echo the source text, including a lower-case `null`"
    );
    let row = &result
        .rows
        .first()
        .expect("expected exactly one row")
        .values;
    assert_eq!(row, &vec![Value::Null, Value::Null]);
}

#[test]
fn relationships_of_a_variable_length_path_are_returned_in_order() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (s:Start)-[:REL {num: 1}]->(b:B)-[:REL {num: 2}]->(c:C)")
        .expect("fixture should be created");

    let result = engine
        .execute_cypher("MATCH p = (a:Start)-[:REL*2..2]->(b) RETURN relationships(p)")
        .expect("query should execute");

    let row = &result.rows.first().expect("expected one row").values;
    let rels = row
        .first()
        .and_then(|v| v.as_array())
        .expect("relationships(p) must be a list");
    let nums: Vec<i64> = rels
        .iter()
        .filter_map(|r| r.get("num").and_then(|n| n.as_i64()))
        .collect();
    assert_eq!(
        nums,
        vec![1, 2],
        "both hops of the path must be present, in traversal order: {rels:?}"
    );
}

#[test]
fn relationships_of_a_variable_length_path_anchored_at_the_end() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:REL {num: 1}]->(b:B)-[:REL {num: 2}]->(e:End)")
        .expect("fixture should be created");

    let result = engine
        .execute_cypher("MATCH p = (a)-[:REL*2..2]->(b:End) RETURN relationships(p)")
        .expect("query should execute");

    let row = &result.rows.first().expect("expected one row").values;
    let rels = row
        .first()
        .and_then(|v| v.as_array())
        .expect("relationships(p) must be a list");
    let nums: Vec<i64> = rels
        .iter()
        .filter_map(|r| r.get("num").and_then(|n| n.as_i64()))
        .collect();
    assert_eq!(nums, vec![1, 2], "got: {rels:?}");
}

#[test]
fn length_of_a_variable_length_path_counts_hops_including_zero() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:REL]->(b:B)")
        .expect("fixture should be created");

    let result = engine
        .execute_cypher("MATCH p = (a)-[*0..1]->(b) RETURN length(p) AS l")
        .expect("query should execute");

    // `*0..1` over a single edge yields the two zero-length self paths plus the
    // one-hop path, so the lengths are 0, 0 and 1 — not 0 for all three.
    let mut lengths: Vec<i64> = result
        .rows
        .iter()
        .filter_map(|r| r.values.first().and_then(|v| v.as_i64()))
        .collect();
    lengths.sort_unstable();
    assert_eq!(lengths, vec![0, 0, 1]);
}

#[test]
fn nodes_of_a_variable_length_path_are_unaffected_by_the_interleaving() {
    // Control: the path value now carries relationships too, so `nodes()` must
    // still return exactly the nodes — three of them for a two-hop path.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (s:Start)-[:REL]->(b:B)-[:REL]->(c:C)")
        .expect("fixture should be created");

    let result = engine
        .execute_cypher("MATCH p = (a:Start)-[:REL*2..2]->(b) RETURN nodes(p)")
        .expect("query should execute");

    let row = &result.rows.first().expect("expected one row").values;
    let nodes = row
        .first()
        .and_then(|v| v.as_array())
        .expect("nodes(p) must be a list");
    assert_eq!(nodes.len(), 3, "got: {nodes:?}");
}

#[test]
fn length_on_a_node_is_a_compile_time_type_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("MATCH (n) RETURN length(n)")
        .expect_err("length() on a node must be rejected, not answered with 0");
    assert!(
        err.to_string().contains("InvalidArgumentType"),
        "expected an InvalidArgumentType detail token, got: {err}"
    );
}

#[test]
fn length_on_a_relationship_is_a_compile_time_type_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("MATCH ()-[r]->() RETURN length(r)")
        .expect_err("length() on a relationship must be rejected");
    assert!(
        err.to_string().contains("InvalidArgumentType"),
        "expected an InvalidArgumentType detail token, got: {err}"
    );
}

#[test]
fn length_rejects_a_node_argument_nested_inside_an_expression() {
    // The check recurses, so wrapping the call does not smuggle it past.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("MATCH (n) RETURN 1 + length(n) AS x")
        .expect_err("a nested length(node) must be rejected too");
    assert!(
        err.to_string().contains("InvalidArgumentType"),
        "expected an InvalidArgumentType detail token, got: {err}"
    );
}

#[test]
fn length_on_a_string_literal_still_works() {
    // Control: the check only judges variables the query binds as a node or a
    // relationship. `length()` over a string must keep working.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine
        .execute_cypher("RETURN length('abcd') AS l")
        .expect("length() over a string must not be rejected");
    assert_eq!(
        result.rows.first().expect("one row").values,
        vec![Value::Number(4.into())]
    );
}

#[test]
fn length_on_a_node_property_is_left_to_runtime() {
    // Control: a property access is not a statically-typed pattern variable, so
    // the pass must not flag it — `length(n.name)` is a legal string length.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (n:Person {name: 'abcd'})")
        .expect("CREATE should succeed");
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN length(n.name) AS l")
        .expect("length() over a string property must not be rejected");
    assert_eq!(
        result.rows.first().expect("one row").values,
        vec![Value::Number(4.into())]
    );
}

#[test]
fn relationships_on_a_null_path_is_null_not_an_empty_list() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine
        .execute_cypher(
            "WITH null AS a OPTIONAL MATCH p = (a)-[r]->() \
             RETURN relationships(p), relationships(null)",
        )
        .expect("query should execute");

    assert_eq!(
        result.columns,
        vec![
            "relationships(p)".to_string(),
            "relationships(null)".to_string()
        ]
    );
    let row = &result
        .rows
        .first()
        .expect("expected exactly one row")
        .values;
    assert_eq!(row, &vec![Value::Null, Value::Null]);
}
