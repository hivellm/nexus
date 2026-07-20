//! Extended Regression Tests — engine API surface.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Covers direct Engine API (create_node, create_relationship, stats, health,
//! refresh_executor, get_node, persistence).

use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;

#[test]
fn regression_engine_new() {
    let (_engine, _ctx) = setup_test_engine().unwrap();
}

#[test]
fn regression_engine_create_node_api() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let _id = engine
        .create_node(vec!["Test".to_string()], json!({"name": "test"}))
        .unwrap();
    // Node creation succeeded - ID is valid (zero-indexed)
}

#[test]
fn regression_engine_create_relationship_api() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();

    let _id = engine
        .create_relationship(a, b, "REL".to_string(), json!({}))
        .unwrap();
    // Relationship creation succeeded - ID is valid (zero-indexed)
}

#[test]
fn regression_engine_refresh_executor() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_engine_multiple_refreshes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    engine.refresh_executor().unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_engine_stats() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let stats = engine.stats().unwrap();
    assert!(stats.nodes >= 1);
}

#[test]
fn regression_engine_health_check() {
    let (engine, _ctx) = setup_test_engine().unwrap();

    let health = engine.health_check().unwrap();
    assert!(
        health.overall == nexus_core::HealthState::Healthy
            || health.overall == nexus_core::HealthState::Degraded
    );
}

#[test]
fn regression_engine_execute_cypher() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = engine.execute_cypher("CREATE (n:Test) RETURN n").unwrap();
    assert!(!result.rows.is_empty());
}

#[test]
fn regression_engine_create_10_nodes() {
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
fn regression_engine_create_10_rels() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    for i in 0..10 {
        engine
            .create_relationship(a, b, format!("REL_{}", i), json!({}))
            .unwrap();
    }

    engine.refresh_executor().unwrap();
    let result = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(10));
}

#[test]
fn regression_engine_tempdir_persistence() {
    let (mut engine, ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Temp dir should still exist while engine is alive
    assert!(ctx.path().exists());
}

#[test]
fn regression_engine_get_node() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let node_id = engine
        .create_node(vec!["Test".to_string()], json!({"name": "test"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let node = engine.get_node(node_id).unwrap();
    assert!(node.is_some());
}
