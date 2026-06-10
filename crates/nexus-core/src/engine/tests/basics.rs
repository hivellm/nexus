//! Tests for `Engine` construction, stats, health, component access, and
//! basic API surface (create/get node/rel, KNN search).

use super::*;

#[test]
fn test_engine_creation() {
    let mut engine = Engine::new();
    assert!(engine.is_ok());
    let engine = engine.unwrap();

    // Test that all components are initialized
    // Note: These are unsigned types, so >= 0 is always true
    // We just verify the methods don't panic
    let _ = engine.catalog.label_count();
    let _ = engine.storage.node_count();
    let _ = engine.storage.relationship_count();
    let _ = engine.page_cache.hit_count();
    let _ = engine.page_cache.miss_count();
    let _ = engine.wal.entry_count();
    let _ = engine.transaction_manager.read().active_count();
}

// `Engine::default()` and `Engine::new_default()` both delegate to
// `Engine::new()` which has used `tempfile::tempdir()` since at least
// 1.13.0, so each construction is isolated to its own per-process
// scratch directory. The `#[ignore]` attributes that lived here
// previously were stale carry-over from a pre-tempdir implementation
// and were preventing parallel CI from exercising these constructors.
// Both tests now run under default `--test-threads`.

#[test]
fn test_engine_default() {
    let engine = Engine::default();
    // Test passes if default creation succeeds
    drop(engine);
}

#[test]
fn test_engine_new_default() {
    let engine = Engine::new_default();
    assert!(engine.is_ok());
    drop(engine);
}

#[test]
fn test_engine_stats() {
    let mut engine = Engine::new().unwrap();
    let stats = engine.stats().unwrap();

    // Test that stats are accessible
    // Note: These are unsigned types, so >= 0 is always true
    // We just verify the stats are accessible
    let _ = stats.nodes;
    let _ = stats.relationships;
    let _ = stats.labels;
    let _ = stats.rel_types;
    let _ = stats.page_cache_hits;
    let _ = stats.page_cache_misses;
    let _ = stats.wal_entries;
    let _ = stats.active_transactions;
}

#[test]
fn test_engine_execute_cypher() {
    let mut engine = Engine::new().unwrap();

    // Test executing a simple query
    let result = engine.execute_cypher("MATCH (n) RETURN n");
    // Should not panic, even if query fails
    drop(result);
}

#[test]
fn test_engine_create_node() {
    let mut engine = Engine::new().unwrap();

    // Test creating a node
    let labels = vec!["Person".to_string()];
    let properties = serde_json::json!({"name": "Alice", "age": 30});

    let result = engine.create_node(labels, properties);
    // Should not panic, even if creation fails
    drop(result);
}

#[test]
fn test_engine_create_relationship() {
    let mut engine = Engine::new().unwrap();

    // Test creating a relationship
    let result = engine.create_relationship(
        1, // from
        2, // to
        "KNOWS".to_string(),
        serde_json::json!({"since": 2020}),
    );
    // Should not panic, even if creation fails
    drop(result);
}

#[test]
fn test_engine_get_node() {
    let mut engine = Engine::new().unwrap();

    // Test getting a node
    let result = engine.get_node(1);
    // Should not panic, even if node doesn't exist
    drop(result);
}

#[test]
fn test_engine_get_relationship() {
    let mut engine = Engine::new().unwrap();

    // Test getting a relationship
    let result = engine.get_relationship(1);
    // Should not panic, even if relationship doesn't exist
    drop(result);
}

#[test]
fn test_engine_knn_search() {
    let mut engine = Engine::new().unwrap();

    // Test KNN search
    let vector = vec![0.1, 0.2, 0.3, 0.4];
    let result = engine.knn_search("Person", &vector, 5);
    // Should not panic, even if search fails
    drop(result);
}

#[test]
fn test_engine_health_check() {
    let mut engine = Engine::new().unwrap();

    // Test health check
    let status = engine.health_check().unwrap();

    // Test that health status is properly structured
    assert!(matches!(
        status.overall,
        HealthState::Healthy | HealthState::Unhealthy | HealthState::Degraded
    ));
    assert!(!status.components.is_empty());

    // Test that all expected components are present
    let expected_components = ["catalog", "storage", "page_cache", "wal", "indexes"];
    for component in expected_components {
        assert!(status.components.contains_key(component));
    }
}

#[test]
fn test_engine_stats_serialization() {
    let mut engine = Engine::new().unwrap();
    let stats = engine.stats().unwrap();

    // Test JSON serialization
    let json = serde_json::to_string(&stats).unwrap();
    assert!(json.contains("nodes"));
    assert!(json.contains("relationships"));
    assert!(json.contains("labels"));
    assert!(json.contains("rel_types"));
    assert!(json.contains("page_cache_hits"));
    assert!(json.contains("page_cache_misses"));
    assert!(json.contains("wal_entries"));
    assert!(json.contains("active_transactions"));

    // Test deserialization
    let deserialized: EngineStats = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.nodes, stats.nodes);
    assert_eq!(deserialized.relationships, stats.relationships);
    assert_eq!(deserialized.labels, stats.labels);
    assert_eq!(deserialized.rel_types, stats.rel_types);
}

#[test]
fn test_health_status_serialization() {
    let mut status = HealthStatus {
        overall: HealthState::Healthy,
        components: std::collections::HashMap::new(),
    };
    status
        .components
        .insert("test".to_string(), HealthState::Healthy);

    // Test JSON serialization
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("overall"));
    assert!(json.contains("components"));
    assert!(json.contains("test"));

    // Test deserialization
    let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.overall, HealthState::Healthy);
    assert!(deserialized.components.contains_key("test"));
}

#[test]
fn test_health_state_variants() {
    // Test all health state variants
    assert_eq!(HealthState::Healthy, HealthState::Healthy);
    assert_eq!(HealthState::Unhealthy, HealthState::Unhealthy);
    assert_eq!(HealthState::Degraded, HealthState::Degraded);

    assert_ne!(HealthState::Healthy, HealthState::Unhealthy);
    assert_ne!(HealthState::Healthy, HealthState::Degraded);
    assert_ne!(HealthState::Unhealthy, HealthState::Degraded);

    // Test serialization
    let healthy_json = serde_json::to_string(&HealthState::Healthy).unwrap();
    assert!(healthy_json.contains("Healthy"));

    let unhealthy_json = serde_json::to_string(&HealthState::Unhealthy).unwrap();
    assert!(unhealthy_json.contains("Unhealthy"));

    let degraded_json = serde_json::to_string(&HealthState::Degraded).unwrap();
    assert!(degraded_json.contains("Degraded"));
}

#[test]
fn test_engine_stats_clone() {
    let mut engine = Engine::new().unwrap();
    let stats = engine.stats().unwrap();
    let cloned_stats = stats.clone();

    assert_eq!(stats.nodes, cloned_stats.nodes);
    assert_eq!(stats.relationships, cloned_stats.relationships);
    assert_eq!(stats.labels, cloned_stats.labels);
    assert_eq!(stats.rel_types, cloned_stats.rel_types);
    assert_eq!(stats.page_cache_hits, cloned_stats.page_cache_hits);
    assert_eq!(stats.page_cache_misses, cloned_stats.page_cache_misses);
    assert_eq!(stats.wal_entries, cloned_stats.wal_entries);
    assert_eq!(stats.active_transactions, cloned_stats.active_transactions);
}

#[test]
fn test_health_status_clone() {
    let mut status = HealthStatus {
        overall: HealthState::Healthy,
        components: std::collections::HashMap::new(),
    };
    status
        .components
        .insert("test".to_string(), HealthState::Healthy);

    let cloned_status = status.clone();
    assert_eq!(status.overall, cloned_status.overall);
    assert_eq!(status.components.len(), cloned_status.components.len());
    assert!(cloned_status.components.contains_key("test"));
}

#[test]
fn test_health_state_copy() {
    let healthy = HealthState::Healthy;
    let copied = healthy;

    assert_eq!(healthy, copied);
    assert_eq!(format!("{:?}", healthy), "Healthy");
    assert_eq!(format!("{:?}", copied), "Healthy");
}

#[test]
fn test_engine_stats_debug() {
    let mut engine = Engine::new().unwrap();
    let stats = engine.stats().unwrap();
    let debug = format!("{:?}", stats);

    assert!(debug.contains("EngineStats"));
    assert!(debug.contains("nodes"));
    assert!(debug.contains("relationships"));
}

#[test]
fn test_health_status_debug() {
    let mut status = HealthStatus {
        overall: HealthState::Healthy,
        components: std::collections::HashMap::new(),
    };
    status
        .components
        .insert("test".to_string(), HealthState::Healthy);

    let debug = format!("{:?}", status);
    assert!(debug.contains("HealthStatus"));
    assert!(debug.contains("overall"));
    assert!(debug.contains("components"));
}

#[test]
fn test_health_state_debug() {
    let healthy = HealthState::Healthy;
    let debug = format!("{:?}", healthy);
    assert_eq!(debug, "Healthy");

    let unhealthy = HealthState::Unhealthy;
    let debug = format!("{:?}", unhealthy);
    assert_eq!(debug, "Unhealthy");

    let degraded = HealthState::Degraded;
    let debug = format!("{:?}", degraded);
    assert_eq!(debug, "Degraded");
}

#[test]
fn test_engine_component_access() {
    let mut engine = Engine::new().unwrap();

    // Test that all components are accessible
    let _catalog = &engine.catalog;
    let _storage = &engine.storage;
    let _page_cache = &engine.page_cache;
    let _wal = &engine.wal;
    let _transaction_manager = &engine.transaction_manager;
    let _indexes = &engine.indexes;
    let _executor = &engine.executor;

    // Test passes if all components are accessible
}

#[test]
fn test_engine_mut_operations() {
    let mut engine = Engine::new().unwrap();

    // Test mutable operations
    let _stats = engine.stats().unwrap();
    let _cypher_result = engine.execute_cypher("MATCH (n) RETURN n");
    let _node_result = engine.create_node(vec!["Test".to_string()], serde_json::Value::Null);
    let _rel_result = engine.create_relationship(1, 2, "TEST".to_string(), serde_json::Value::Null);
    let _get_node = engine.get_node(1);
    let _get_rel = engine.get_relationship(1);

    // Test passes if all mutable operations compile
}
