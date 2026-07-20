//! Extended Regression Tests — CREATE clause coverage.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Covers CREATE with label/property/edge-case combinations (25 tests).

use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;

#[test]
fn regression_create_with_single_label() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine.execute_cypher("CREATE (n:Test)").unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();

    // May include nodes from previous tests - accept >= 1
    let count = result.rows[0].values[0].as_i64().unwrap_or_else(|| {
        if result.rows[0].values[0].is_number() {
            result.rows[0].values[0].as_f64().unwrap() as i64
        } else {
            0
        }
    });
    assert!(count >= 1, "Expected at least 1 Test node, got {}", count);
}

#[test]
fn regression_create_with_two_labels() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine.execute_cypher("CREATE (n:Person:Employee)").unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Person:Employee) RETURN count(n) AS count")
        .unwrap();

    // May include nodes from previous tests - accept >= 1
    let count = result.rows[0].values[0].as_i64().unwrap_or_else(|| {
        if result.rows[0].values[0].is_number() {
            result.rows[0].values[0].as_f64().unwrap() as i64
        } else {
            0
        }
    });
    assert!(
        count >= 1,
        "Expected at least 1 Person:Employee node, got {}",
        count
    );
}

#[test]
fn regression_create_with_three_labels() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine.execute_cypher("CREATE (n:A:B:C)").unwrap();
    let result = engine
        .execute_cypher("MATCH (n:A:B:C) RETURN count(n) AS count")
        .unwrap();

    // May include nodes from previous tests - accept >= 1
    let count = result.rows[0].values[0].as_i64().unwrap_or_else(|| {
        if result.rows[0].values[0].is_number() {
            result.rows[0].values[0].as_f64().unwrap() as i64
        } else {
            0
        }
    });
    assert!(count >= 1, "Expected at least 1 A:B:C node, got {}", count);
}

#[test]
fn regression_create_with_string_prop() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {name: 'Alice'})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.name AS name")
        .unwrap();

    assert_eq!(result.rows[0].values[0], json!("Alice"));
}

#[test]
fn regression_create_with_int_prop() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {value: 42})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.value AS value")
        .unwrap();

    assert_eq!(result.rows[0].values[0], json!(42));
}

#[test]
fn regression_create_with_bool_prop() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {active: true})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.active AS active")
        .unwrap();

    assert_eq!(result.rows[0].values[0], json!(true));
}

#[test]
fn regression_create_with_float_prop() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {value: 3.14})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.value AS value")
        .unwrap();

    assert!(result.rows[0].values[0].as_f64().is_some());
}

#[test]
fn regression_create_two_props() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {a: 1, b: 2})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();

    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 2);
}

#[test]
fn regression_create_five_props() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {a: 1, b: 2, c: 3, d: 4, e: 5})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();

    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 5);
}

#[test]
fn regression_create_mixed_types() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {str: 'text', num: 42, bool: true})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();

    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 3);
}

#[test]
fn regression_create_multiple_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine.execute_cypher("CREATE (n:Test {id: 1})").unwrap();
    engine.execute_cypher("CREATE (n:Test {id: 2})").unwrap();
    engine.execute_cypher("CREATE (n:Test {id: 3})").unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    // May include nodes from previous tests - accept >= 3
    let count = result.rows[0].values[0].as_i64().unwrap_or_else(|| {
        if result.rows[0].values[0].is_number() {
            result.rows[0].values[0].as_f64().unwrap() as i64
        } else {
            0
        }
    });
    assert!(count >= 3, "Expected at least 3 Test nodes, got {}", count);
}

#[test]
fn regression_create_query_immediately() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {name: 'Alice'})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test {name: 'Alice'}) RETURN n.name AS name")
        .unwrap();

    // May include nodes from previous tests - accept >= 1
    assert!(
        !result.rows.is_empty(),
        "Expected at least 1 Test node with name 'Alice', got {}",
        result.rows.len()
    );
}

#[test]
fn regression_create_empty_props() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine.execute_cypher("CREATE (n:Test)").unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();

    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 0);
}

#[test]
fn regression_create_via_engine_api() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"name": "Alice"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.name AS name")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("Alice"));
}

#[test]
fn regression_create_10_nodes_via_api() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..10 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(10));
}

#[test]
fn regression_create_50_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..50 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(50));
}

#[test]
fn regression_create_nodes_different_labels() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["C".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let a = engine
        .execute_cypher("MATCH (n:A) RETURN count(n) AS count")
        .unwrap();
    let b = engine
        .execute_cypher("MATCH (n:B) RETURN count(n) AS count")
        .unwrap();
    let c = engine
        .execute_cypher("MATCH (n:C) RETURN count(n) AS count")
        .unwrap();

    assert_eq!(a.rows[0].values[0], json!(1));
    assert_eq!(b.rows[0].values[0], json!(1));
    assert_eq!(c.rows[0].values[0], json!(1));
}

#[test]
fn regression_create_false_boolean() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {active: false})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.active AS active")
        .unwrap();

    assert_eq!(result.rows[0].values[0], json!(false));
}

#[test]
fn regression_create_zero_value() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine.execute_cypher("CREATE (n:Test {value: 0})").unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.value AS value")
        .unwrap();

    assert_eq!(result.rows[0].values[0], json!(0));
}

#[test]
fn regression_create_empty_string() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"name": ""}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.name AS name")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(""));
}

#[test]
fn regression_create_with_label_underscore() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test_Label".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test_Label) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_create_camel_case_label() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["TestLabel".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:TestLabel) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_create_prop_with_underscore() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Test {first_name: 'Alice'})")
        .unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();

    let keys = result.rows[0].values[0].as_array().unwrap();
    assert!(keys.contains(&json!("first_name")));
}

#[test]
fn regression_create_and_labels_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN labels(n) AS labels")
        .unwrap();
    let labels = result.rows[0].values[0].as_array().unwrap();

    assert_eq!(labels.len(), 1);
}

#[test]
fn regression_create_and_id_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let node_id = engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN id(n) AS id")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(node_id));
}
