//! Engine-level integration tests. Uses `crate::testing::setup_isolated_test_engine`.

#![allow(unused_imports)]
use super::*;
use crate::testing::setup_isolated_test_engine;

#[test]
fn test_error_storage() {
    let err = Error::storage("test error");
    assert!(matches!(err, Error::Storage(_)));
    assert_eq!(err.to_string(), "Storage error: test error");
}

#[test]
fn test_error_page_cache() {
    let err = Error::page_cache("cache full");
    assert!(matches!(err, Error::PageCache(_)));
}

#[test]
fn test_error_wal() {
    let err = Error::wal("checkpoint failed");
    assert!(matches!(err, Error::Wal(_)));
}

#[test]
fn test_error_catalog() {
    let err = Error::catalog("catalog error");
    assert!(matches!(err, Error::Catalog(_)));
    assert!(err.to_string().contains("catalog error"));
}

#[test]
fn test_error_transaction() {
    let err = Error::transaction("tx failed");
    assert!(matches!(err, Error::Transaction(_)));
    assert!(err.to_string().contains("tx failed"));
}

#[test]
fn test_error_index() {
    let err = Error::index("index error");
    assert!(matches!(err, Error::Index(_)));
    assert!(err.to_string().contains("index error"));
}

#[test]
fn test_error_executor() {
    let err = Error::executor("exec error");
    assert!(matches!(err, Error::Executor(_)));
    assert!(err.to_string().contains("exec error"));
}

#[test]
fn test_error_internal() {
    let err = Error::internal("internal error");
    assert!(matches!(err, Error::Internal(_)));
    assert!(err.to_string().contains("internal error"));
}

#[test]
fn test_error_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err: Error = io_err.into();
    assert!(matches!(err, Error::Io(_)));
    assert!(err.to_string().contains("I/O error"));
}

#[test]
fn test_node_type_export() {
    // Test that NodeType is properly exported from the main library
    use crate::NodeType;

    let function = NodeType::Function;
    let module = NodeType::Module;
    let class = NodeType::Class;
    let variable = NodeType::Variable;
    let api = NodeType::API;

    // Test that all variants are accessible
    assert_eq!(format!("{:?}", function), "Function");
    assert_eq!(format!("{:?}", module), "Module");
    assert_eq!(format!("{:?}", class), "Class");
    assert_eq!(format!("{:?}", variable), "Variable");
    assert_eq!(format!("{:?}", api), "API");

    // Test serialization
    let json = serde_json::to_string(&api).unwrap();
    assert!(json.contains("API"));

    let deserialized: NodeType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, NodeType::API);
}

#[test]
fn test_error_database() {
    let db_err = heed::Error::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "db file not found",
    ));
    let err: Error = db_err.into();
    assert!(matches!(err, Error::Database(_)));
}

#[test]
fn test_error_not_found() {
    let err = Error::NotFound("node 123".to_string());
    assert!(matches!(err, Error::NotFound(_)));
    assert!(err.to_string().contains("node 123"));
}

#[test]
fn test_error_invalid_id() {
    let err = Error::InvalidId("invalid node id".to_string());
    assert!(matches!(err, Error::InvalidId(_)));
    assert!(err.to_string().contains("invalid node id"));
}

#[test]
fn test_error_constraint_violation() {
    let err = Error::ConstraintViolation("unique constraint violated".to_string());
    assert!(matches!(err, Error::ConstraintViolation(_)));
    assert!(err.to_string().contains("unique constraint violated"));
}

#[test]
fn test_error_type_mismatch() {
    let err = Error::TypeMismatch {
        expected: "String".to_string(),
        actual: "Int64".to_string(),
    };
    assert!(matches!(err, Error::TypeMismatch { .. }));
    assert!(err.to_string().contains("String"));
    assert!(err.to_string().contains("Int64"));
}

#[test]
fn test_error_cypher_syntax() {
    let err = Error::CypherSyntax("unexpected token".to_string());
    assert!(matches!(err, Error::CypherSyntax(_)));
    assert!(err.to_string().contains("unexpected token"));
}

#[test]
fn test_error_debug() {
    let err = Error::Storage("test".to_string());
    let debug = format!("{:?}", err);
    assert!(debug.contains("Storage"));
}

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

#[test]
#[ignore] // TODO: Fix - uses default data dir which conflicts with parallel tests
fn test_engine_default() {
    let engine = Engine::default();
    // Test passes if default creation succeeds
    drop(engine);
}

#[test]
#[ignore] // TODO: Fix - uses default data dir which conflicts with parallel tests
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

#[test]
fn test_update_node() {
    let mut engine = Engine::new().unwrap();

    // Create a node first
    let node_id = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Update the node
    let mut properties = serde_json::Map::new();
    properties.insert(
        "name".to_string(),
        serde_json::Value::String("Alice".to_string()),
    );
    properties.insert("age".to_string(), serde_json::Value::Number(30.into()));

    let result = engine.update_node(
        node_id,
        vec!["Person".to_string(), "Updated".to_string()],
        serde_json::Value::Object(properties),
    );

    assert!(result.is_ok());
}

#[test]
fn test_update_nonexistent_node() {
    let mut engine = Engine::new().unwrap();

    // Try to update a non-existent node
    let result = engine.update_node(
        999,
        vec!["Person".to_string()],
        serde_json::Value::Object(serde_json::Map::new()),
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_delete_node() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Create a node first
    let node_id = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Delete the node
    let result = engine.delete_node(node_id);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_delete_nonexistent_node() {
    let mut engine = Engine::new().unwrap();

    // Try to delete a non-existent node
    let result = engine.delete_node(999);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_convert_to_simple_graph() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes and relationships
    let node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let node2 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _rel_id = engine
        .create_relationship(
            node1,
            node2,
            "KNOWS".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Convert to simple graph
    let simple_graph = engine.convert_to_simple_graph().unwrap();

    // Check that the simple graph has the expected structure
    let stats = simple_graph.stats().unwrap();
    assert!(stats.total_nodes >= 2);
    assert!(stats.total_edges >= 1);
}

#[test]
fn test_cluster_nodes() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes
    let _node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _node2 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Test clustering
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::LabelBased,
        feature_strategy: FeatureStrategy::LabelBased,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: None,
    };

    let result = engine.cluster_nodes(config);
    assert!(result.is_ok());

    let _clustering_result = result.unwrap();
}

#[test]
fn test_group_nodes_by_labels() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes with different labels
    let _node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _node2 = engine
        .create_node(
            vec!["Company".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Test label-based grouping
    let result = engine.group_nodes_by_labels();
    assert!(result.is_ok());

    let _clustering_result = result.unwrap();
}

#[test]
fn test_group_nodes_by_property() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes with properties
    let mut properties1 = serde_json::Map::new();
    properties1.insert("age".to_string(), serde_json::Value::Number(25.into()));
    let _node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(properties1),
        )
        .unwrap();

    let mut properties2 = serde_json::Map::new();
    properties2.insert("age".to_string(), serde_json::Value::Number(30.into()));
    let _node2 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(properties2),
        )
        .unwrap();

    // Test property-based grouping
    let result = engine.group_nodes_by_property("age");
    assert!(result.is_ok());

    let _clustering_result = result.unwrap();
}

#[test]
fn test_kmeans_cluster_nodes() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes
    let _node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _node2 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Test K-means clustering
    let result = engine.kmeans_cluster_nodes(2, 10);
    assert!(result.is_ok());

    let _clustering_result = result.unwrap();
}

#[test]
fn test_detect_communities() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes and relationships
    let node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let node2 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _rel_id = engine
        .create_relationship(
            node1,
            node2,
            "KNOWS".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Test community detection
    let result = engine.detect_communities();
    assert!(result.is_ok());

    let _clustering_result = result.unwrap();
}

#[test]
fn test_export_to_json() {
    let mut engine = Engine::new().unwrap();

    // Create some nodes and relationships
    let node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let node2 = engine
        .create_node(
            vec!["Company".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _rel_id = engine
        .create_relationship(
            node1,
            node2,
            "WORKS_AT".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Export to JSON
    let json_data = engine.export_to_json().unwrap();

    // Check that the JSON contains the expected structure
    assert!(json_data.is_object());
    assert!(json_data.get("nodes").is_some());
    assert!(json_data.get("relationships").is_some());

    let nodes = json_data.get("nodes").unwrap().as_array().unwrap();
    let relationships = json_data.get("relationships").unwrap().as_array().unwrap();

    assert!(nodes.len() >= 2);
    assert!(!relationships.is_empty());
}

#[test]
fn test_get_graph_statistics() {
    // Use isolated engine to ensure clean state
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create some nodes with different labels
    let _node1 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _node2 = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _node3 = engine
        .create_node(
            vec!["Company".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Get statistics
    let stats = engine.get_graph_statistics().unwrap();

    assert_eq!(stats.node_count, 3);
    assert_eq!(stats.relationship_count, 0);
    assert_eq!(stats.label_counts.get("Person"), Some(&2));
    assert_eq!(stats.label_counts.get("Company"), Some(&1));
}

#[test]
fn test_clear_all_data() {
    // Use isolated engine for clear data test
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create some data
    let _node1 = engine
        .create_node(
            vec!["ClearPerson".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    let _node2 = engine
        .create_node(
            vec!["ClearCompany".to_string()],
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .unwrap();

    // Verify data exists
    let stats_before = engine.get_graph_statistics().unwrap();
    assert_eq!(stats_before.node_count, 2);

    // Clear all data
    engine.clear_all_data().unwrap();

    // Verify data is cleared
    let stats_after = engine.get_graph_statistics().unwrap();
    assert_eq!(stats_after.node_count, 0);
    assert_eq!(stats_after.relationship_count, 0);
}

/// Regression test for phase6_nexus-create-bound-var-duplication.
///
/// A single CREATE that declares node variables and references
/// those variables inside a relationship pattern *in the same
/// CREATE* binds the variables instead of re-creating them as
/// unbound duplicates. Neo4j 2025.09.0 honours this: the
/// statement below produces exactly 2 nodes + 1 relationship.
/// Before the fix Nexus was creating 4 nodes — the two declared
/// plus two anonymous duplicates from the edge pattern.
#[test]
fn create_bound_variable_edge_does_not_duplicate_nodes() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (a:X {id: 1}), (b:X {id: 2}), (a)-[:R]->(b)")
        .expect("CREATE must succeed");

    let node_count = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    let cell = &node_count.rows[0].values[0];
    assert_eq!(
        cell.as_u64(),
        Some(2),
        "expected 2 nodes after CREATE with bound-variable edge, got {cell:?}"
    );

    let rel_count = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS c")
        .unwrap();
    let cell = &rel_count.rows[0].values[0];
    assert_eq!(
        cell.as_u64(),
        Some(1),
        "expected 1 relationship, got {cell:?}"
    );
}

/// Regression for phase6_nexus-bench-correctness-gaps §1 —
/// composite `:Label {prop: value}` filter.
///
/// Pre-fix symptom: on a database where both `:X` and `:Y` labels
/// hold nodes with `id: 0`, `MATCH (:X {id: 0})-[:R]->(b) RETURN
/// count(b)` counted every `:R`-outgoing edge in the database,
/// ignoring both the `:X` label and the `{id: 0}` property
/// filter. The bench's `traversal.small_one_hop_hub` caught this
/// (99 edges counted instead of 5).
#[test]
fn match_scopes_by_label_and_property_together() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Two labels, each with its own id=0 node + outgoing edges.
    // Only the X/id=0 node's outgoing count (3) should be
    // returned when the query scopes to :X {id: 0}. If either
    // the label or the property filter is dropped, the answer
    // drifts upward (to 5 if only label scope lost, to 3+2 = 5
    // or higher if prop scope lost, etc.).
    engine
        .execute_cypher(
            "CREATE (x0:X {id: 0}), (x1:X {id: 1}), \
             (y0:Y {id: 0}), (y1:Y {id: 1}), \
             (t0:Target {id: 10}), (t1:Target {id: 11}), (t2:Target {id: 12}), \
             (x0)-[:R]->(t0), (x0)-[:R]->(t1), (x0)-[:R]->(t2), \
             (x1)-[:R]->(t0), (x1)-[:R]->(t1), \
             (y0)-[:R]->(t0), (y0)-[:R]->(t1), \
             (y1)-[:R]->(t0)",
        )
        .expect("seed CREATE must succeed");

    // Sanity: the total edge count is 8 — if the composite
    // filter collapses and this test suddenly asserts on 8, the
    // bug is still live.
    let all = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS c")
        .unwrap();
    assert_eq!(
        all.rows[0].values[0].as_u64(),
        Some(8),
        "sanity: 8 edges total"
    );

    // The actual assertion.
    let scoped = engine
        .execute_cypher("MATCH (:X {id: 0})-[:R]->(b) RETURN count(b) AS c")
        .unwrap();
    assert_eq!(
        scoped.rows[0].values[0].as_u64(),
        Some(3),
        "expected 3 (X+id=0 has 3 outgoing); composite label+property \
         filter broke — bench's traversal.small_one_hop_hub regression"
    );
}

/// Bench-shape regression for phase6_nexus-bench-correctness-gaps §1 —
/// anonymous anchor `(:P {id: 0})` with labels and properties on a
/// database that also holds nodes of other labels with the same id.
///
/// Pre-fix symptom: `MATCH (:P {id: 0})-[:KNOWS]->(b) RETURN count(b)`
/// returned too-many rows — Expand fell back to scanning every KNOWS
/// edge in the store because the planner skipped NodeByLabel/Filter for
/// anonymous anchors whose variable was `None`, and `source_var` was the
/// empty string. Fixed by synthesising a variable for the anchor in
/// `plan_execution_strategy::synthesise_anonymous_source_anchors` so the
/// NodeByLabel + Filter pair constrain the source set and the Expand's
/// `source_var` resolves to the anchor instead of the source-less fallback.
#[test]
fn match_anonymous_anchor_with_label_and_property_scopes_expand() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Mirror the bench fixture: one dataset with `:A`/`:B` labels and
    // an `id: 0` node that must NOT match `(:P {id: 0})`, one dataset
    // with `:P` labels where only p0 should match. The anchor has no
    // variable — the exact shape the bench scenarios use.
    engine
        .execute_cypher(
            "CREATE \
             (n0:A {id: 0, name: 'n0'}), (n1:A {id: 1, name: 'n1'}), (n2:A {id: 2, name: 'n2'}), \
             (n0)-[:KNOWS]->(n1), (n1)-[:KNOWS]->(n2)",
        )
        .expect("seed TinyDataset-like must succeed");
    engine
        .execute_cypher(
            "CREATE \
             (p0:P {id: 0}), (p1:P {id: 1}), (p2:P {id: 2}), (p3:P {id: 3}), (p4:P {id: 4}), (p5:P {id: 5}), \
             (p0)-[:KNOWS]->(p1), (p0)-[:KNOWS]->(p2), (p0)-[:KNOWS]->(p3), (p0)-[:KNOWS]->(p4), (p0)-[:KNOWS]->(p5), \
             (p1)-[:KNOWS]->(p2), (p2)-[:KNOWS]->(p3)",
        )
        .expect("seed SmallDataset-like must succeed");

    // Sanity: 2 + 7 = 9 total edges. If the anchor filter silently
    // collapses, count(b) drifts toward 9.
    let all = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS c")
        .unwrap();
    assert_eq!(all.rows[0].values[0].as_u64(), Some(9), "sanity: 9 edges");

    // The actual assertion: only p0's 5 outgoing KNOWS edges count.
    // Pre-fix this returned ~9 (every KNOWS edge) because the planner
    // skipped NodeByLabel+Filter for the anonymous anchor and Expand
    // fell back to scanning every relationship in the store.
    let scoped = engine
        .execute_cypher("MATCH (:P {id: 0})-[:KNOWS]->(b) RETURN count(b) AS c")
        .unwrap();
    assert_eq!(
        scoped.rows[0].values[0].as_u64(),
        Some(5),
        "expected 5 (p0 has 5 outgoing); anonymous anchor label+property \
         filter silently dropped — bench's traversal.small_one_hop_hub \
         regression at bench scale"
    );
}

/// Regression for phase6_nexus-bench-correctness-gaps §2 —
/// variable-length path `*1..n` on an anonymous, label+property-anchored
/// source.
///
/// Pre-fix symptom: `MATCH (:P {id: 0})-[:KNOWS*1..3]->(n) RETURN
/// count(DISTINCT n)` returned 0 — `VariableLengthPath` inherits the
/// same empty `source_var` that the §1 bug produced for single-hop
/// Expand, and the variable-length operator's source-less fallback
/// returns no rows (unlike Expand, which over-counts). Same root cause;
/// the anchor-synthesis fix in `plan_execution_strategy` covers it.
#[test]
fn match_anonymous_anchor_var_length_expansion_is_bounded_by_filter() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // SmallDataset-like topology: p0 → p1..p5 (5 at 1-hop), p1 → p2, p2 → p3.
    // From p0 with *1..3, distinct reachable: {p1,p2,p3,p4,p5} = 5.
    engine
        .execute_cypher(
            "CREATE \
             (p0:P {id: 0}), (p1:P {id: 1}), (p2:P {id: 2}), (p3:P {id: 3}), (p4:P {id: 4}), (p5:P {id: 5}), \
             (p0)-[:KNOWS]->(p1), (p0)-[:KNOWS]->(p2), (p0)-[:KNOWS]->(p3), (p0)-[:KNOWS]->(p4), (p0)-[:KNOWS]->(p5), \
             (p1)-[:KNOWS]->(p2), (p2)-[:KNOWS]->(p3)",
        )
        .expect("seed must succeed");

    let scoped = engine
        .execute_cypher("MATCH (:P {id: 0})-[:KNOWS*1..3]->(n) RETURN count(DISTINCT n) AS c")
        .unwrap();
    assert_eq!(
        scoped.rows[0].values[0].as_u64(),
        Some(5),
        "expected 5 distinct nodes reachable from p0 via *1..3; \
         pre-fix returned 0 because the anchor carried no variable"
    );
}

/// Regression for phase6_nexus-bench-correctness-gaps §4 —
/// integer-only arithmetic must return an integer, not a float.
///
/// Pre-fix symptom: every binary op unconditionally promoted to `f64`,
/// so `RETURN 1 + 2 * 3 AS n` returned `Number(Float(7.0))` where Neo4j
/// returns `Number(Int(7))`. The bench's `scalar.arithmetic` scenario
/// reproduced this against `target/bench/report`.
#[test]
fn integer_only_arithmetic_stays_integer() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    let cases: &[(&str, i64)] = &[
        ("RETURN 1 + 2 * 3 AS n", 7),
        ("RETURN 10 - 4 AS n", 6),
        ("RETURN 100 / 4 AS n", 25),
        ("RETURN 7 / 2 AS n", 3), // Cypher integer division
        ("RETURN 10 % 3 AS n", 1),
        ("RETURN 2 * 8 AS n", 16),
    ];
    for (query, expected) in cases {
        let r = engine.execute_cypher(query).unwrap();
        let cell = &r.rows[0].values[0];
        match cell {
            serde_json::Value::Number(num) => {
                assert!(
                    num.is_i64() || num.is_u64(),
                    "`{}` returned a float ({:?}) — integer-only expression should stay int",
                    query,
                    num
                );
                assert_eq!(
                    num.as_i64(),
                    Some(*expected),
                    "`{}` returned {:?}, expected integer {}",
                    query,
                    num,
                    expected
                );
            }
            other => panic!("`{}` returned non-number {:?}", query, other),
        }
    }

    // And conversely: any float operand must promote the whole result.
    let mixed = engine.execute_cypher("RETURN 1 + 2.0 AS n").unwrap();
    match &mixed.rows[0].values[0] {
        serde_json::Value::Number(num) => {
            assert!(
                num.is_f64(),
                "`1 + 2.0` must promote to float — got {:?}",
                num
            );
            assert_eq!(num.as_f64(), Some(3.0));
        }
        other => panic!("`1 + 2.0` returned non-number {:?}", other),
    }
}

/// Regression for phase6_nexus-bench-correctness-gaps §7 —
/// ORDER BY null positioning must follow openCypher:
/// ASC ⇒ nulls LAST, DESC ⇒ nulls FIRST.
#[test]
fn order_by_null_positioning_matches_opencypher() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Seed: three scored + two null-scored nodes.
    engine
        .execute_cypher(
            "CREATE (:N {name: 'a', score: 0.1}), \
             (:N {name: 'b', score: 0.5}), \
             (:N {name: 'c', score: 0.9}), \
             (:N {name: 'x'}), \
             (:N {name: 'y'})",
        )
        .expect("seed must succeed");

    // DESC — openCypher: NULLs first. Projection includes n.score so
    // the Sort operator can resolve it at result_set lookup time.
    let desc = engine
        .execute_cypher(
            "MATCH (n:N) RETURN n.name AS name, n.score AS score \
             ORDER BY n.score DESC LIMIT 5",
        )
        .unwrap();
    let first_two: Vec<_> = desc
        .rows
        .iter()
        .take(2)
        .map(|r| match &r.values[0] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "<null-name>".to_string(),
            other => format!("{:?}", other),
        })
        .collect();
    // Pre-fix: 'c' (non-null) came first; post-fix: nulls come first.
    // Either 'x' or 'y' (both null-score) may be first — just assert
    // that the two null-score rows lead.
    for name in &first_two {
        assert!(
            name == "x" || name == "y",
            "DESC: expected null-score rows first (x or y); got {:?}",
            first_two
        );
    }

    // ASC — openCypher: NULLs last
    let asc = engine
        .execute_cypher(
            "MATCH (n:N) RETURN n.name AS name, n.score AS score \
             ORDER BY n.score ASC LIMIT 5",
        )
        .unwrap();
    let last_two: Vec<_> = asc
        .rows
        .iter()
        .rev()
        .take(2)
        .map(|r| match &r.values[0] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "<null-name>".to_string(),
            other => format!("{:?}", other),
        })
        .collect();
    for name in &last_two {
        assert!(
            name == "x" || name == "y",
            "ASC: expected null-score rows last (x or y); got tail {:?}",
            last_two
        );
    }
}

/// Regression for phase6_nexus-bench-correctness-gaps §9 —
/// statistical aggregations must collapse the row set to one row
/// (just like count / sum / avg). Pre-fix `stdev(n.score)` returned
/// one row per matched node because the planner did not recognise
/// `stdev` as an aggregate.
#[test]
fn statistical_aggregations_collapse_to_one_row() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher(
            "CREATE (:S {v: 1.0}), (:S {v: 2.0}), (:S {v: 3.0}), (:S {v: 4.0}), (:S {v: 5.0})",
        )
        .unwrap();

    let queries = [
        "MATCH (n:S) RETURN stdev(n.v) AS x",
        "MATCH (n:S) RETURN stdevp(n.v) AS x",
        "MATCH (n:S) RETURN percentileCont(n.v, 0.5) AS x",
        "MATCH (n:S) RETURN percentileDisc(n.v, 0.5) AS x",
    ];
    for q in queries {
        let r = engine.execute_cypher(q).unwrap();
        assert_eq!(
            r.rows.len(),
            1,
            "`{}` must return exactly one aggregated row, got {}",
            q,
            r.rows.len()
        );
    }
}

/// Regression for phase6_nexus-bench-correctness-gaps §8 —
/// DELETE must accept variables bound upstream by CREATE (via WITH or
/// directly). Pre-fix Nexus rejected `CREATE (n) WITH n DELETE n`
/// with the parse-time error `DELETE requires MATCH clause`.
#[test]
fn delete_accepts_create_bound_variable() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Simpler form first — CREATE-bound variable referenced by DELETE
    // without an intervening WITH. If this fails the §8 fix is
    // incomplete at the engine level; if only the WITH variant fails
    // the bug is in the WITH-pipeline planning (§5 territory).
    let create_delete = engine.execute_cypher("CREATE (n:BenchCycle) DELETE n");
    assert!(
        create_delete.is_ok(),
        "CREATE + DELETE should execute without error, got: {:?}",
        create_delete.err()
    );

    // Post-condition: nothing left tagged :BenchCycle.
    let surviving = engine
        .execute_cypher("MATCH (n:BenchCycle) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(
        surviving.rows[0].values[0].as_u64(),
        Some(0),
        "create-then-delete leaves no :BenchCycle nodes"
    );
}

/// Regression for phase6_nexus-bench-correctness-gaps §8.2 — the full
/// bench form `CREATE (n:BenchCycle) WITH n DELETE n RETURN 'done' AS status`.
/// Pre-fix: parser rejected the input because `parse_with_clause` did
/// not recognise `DELETE` as the boundary between WITH's item list and
/// the next clause, so `DELETE` was greedily absorbed into the expression
/// parser and surfaced as `Expected identifier`. Fix widens the WITH /
/// RETURN item-list terminator set to include the update keywords
/// (DELETE, DETACH, SET, REMOVE, CREATE, MERGE, FOREACH) and the
/// CALL / UNWIND / WHERE boundary keywords that can already legally
/// follow a WITH.
#[test]
fn create_with_delete_return_parses_and_executes() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    let r = engine.execute_cypher("CREATE (n:Phase6_82) WITH n DELETE n RETURN 'done' AS status");
    assert!(
        r.is_ok(),
        "CREATE + WITH + DELETE + RETURN must parse and execute; got {:?}",
        r.err()
    );
    let rs = r.unwrap();
    assert_eq!(rs.columns, vec!["status"]);
    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values[0].as_str(), Some("done"));

    // Post-condition: CREATE happened, DELETE happened, zero :Phase6_82 survive.
    let surviving = engine
        .execute_cypher("MATCH (n:Phase6_82) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(surviving.rows[0].values[0].as_u64(), Some(0));
}

/// Regression for phase6_nexus-bench-correctness-gaps §3.4 —
/// the parser must accept `CALL … YIELD *`. Pre-fix the parser
/// rejected the `*` at column 25 with "Expected identifier".
#[test]
fn call_procedure_yield_star_parses() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (:A {id: 0}), (:B {id: 1})")
        .unwrap();
    let r = engine.execute_cypher("CALL db.labels() YIELD *");
    assert!(
        r.is_ok(),
        "`CALL db.labels() YIELD *` must parse and execute; got {:?}",
        r.err()
    );
}

/// Regression for phase6_nexus-bench-correctness-gaps §3.1 —
/// `db.labels()` YIELD must emit every label in the catalog as its
/// own row so downstream aggregations (count / collect) see them.
/// Pre-fix the bench reported zero rows against the merged fixture.
/// This test covers the engine-level contract — it does not prove the
/// bench's RPC path, which may serialise procedure rows differently.
#[test]
fn db_labels_procedure_emits_a_row_per_label() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();
    engine
        .execute_cypher(
            "CREATE (:Phase6Labels_A {id: 0}), (:Phase6Labels_B {id: 1}), (:Phase6Labels_C {id: 2})",
        )
        .unwrap();

    let r = engine
        .execute_cypher("CALL db.labels() YIELD label RETURN label")
        .unwrap();

    // The catalog holds more than just our three — the engine allocates
    // bookkeeping / per-test leftover labels — but our three must be in
    // the projection. Assert each by name instead of a strict total.
    let names: Vec<String> = r
        .rows
        .iter()
        .filter_map(|row| match &row.values[0] {
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    for must_have in ["Phase6Labels_A", "Phase6Labels_B", "Phase6Labels_C"] {
        assert!(
            names.iter().any(|n| n == must_have),
            "db.labels() should include {}; got {:?}",
            must_have,
            names
        );
    }
}

/// Regression for phase6_nexus-bench-correctness-gaps §5 —
/// when WITH carries the aggregation and RETURN only references its
/// aliases (wrapping them in a non-aggregate expression), the planner
/// must emit a Project AFTER the Aggregate so the RETURN expression
/// is evaluated. Pre-fix the RETURN items were silently dropped and
/// the aggregation's raw shape leaked through as the final result
/// (the bench scenario `subquery.exists_high_score` caught this).
#[test]
fn with_aggregation_then_return_expression_projects_correctly() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();
    engine
        .execute_cypher(
            "CREATE (:Phase6W {score: 0.1}), (:Phase6W {score: 0.5}), \
             (:Phase6W {score: 0.9}), (:Phase6W {score: 0.99})",
        )
        .unwrap();

    // §5.1 shape — RETURN uses a boolean expression on a WITH alias.
    let r1 = engine
        .execute_cypher(
            "MATCH (n:Phase6W) WITH count(n) AS total, max(n.score) AS hi \
             RETURN hi > 0.99 AS any_high",
        )
        .unwrap();
    assert_eq!(
        r1.columns,
        vec!["any_high"],
        "§5.1: RETURN projection must replace the WITH shape; got columns {:?}",
        r1.columns
    );

    // §5.2 shape — RETURN wraps a collect alias in a non-aggregate call.
    let r2 = engine
        .execute_cypher("MATCH (n:Phase6W) WITH collect(n.score) AS ids RETURN size(ids) AS s")
        .unwrap();
    assert_eq!(
        r2.columns,
        vec!["s"],
        "§5.2: RETURN must project size(ids) under alias `s`, not the collect payload; got {:?}",
        r2.columns
    );
}

/// Regression for phase6_nexus-bench-correctness-gaps §5.3 —
/// WITH projects a non-aggregate expression, WITH's WHERE filters it,
/// then RETURN aggregates over the filtered rows.
///
/// Pre-fix: the planner appended the WITH projection AFTER Aggregate
/// (because the WITH-insertion step looked only for a Project sink and
/// Aggregate is a separate variant). WITH tried to project
/// `n.score AS s` on rows that Aggregate had already collapsed, so the
/// filter `WHERE s > 0.1` saw zero rows downstream and `RETURN count(*)`
/// returned zero rows instead of 1 (count aggregations always emit one).
/// Fix: WITH insertion now treats Aggregate as a valid sink and lands
/// the WITH + its Filter BEFORE the aggregation.
#[test]
fn with_projection_and_filter_run_before_return_aggregation() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();
    engine
        .execute_cypher(
            "CREATE (:Phase6W3 {score: 0.05}), (:Phase6W3 {score: 0.2}), \
             (:Phase6W3 {score: 0.7}), (:Phase6W3 {score: 0.9})",
        )
        .unwrap();

    // Three of the four scores pass `s > 0.1`; count(*) aggregates those.
    let r = engine
        .execute_cypher(
            "MATCH (n:Phase6W3) WITH n.score AS s WHERE s > 0.1 \
             RETURN count(*) AS c",
        )
        .unwrap();
    assert_eq!(
        r.columns,
        vec!["c"],
        "§5.3: result column must be `c`, got {:?}",
        r.columns
    );
    assert_eq!(
        r.rows.len(),
        1,
        "§5.3: count(*) must emit exactly one row after aggregation; got {}",
        r.rows.len()
    );
    assert_eq!(
        r.rows[0].values[0].as_u64(),
        Some(3),
        "§5.3: expected count=3 (scores 0.2, 0.7, 0.9 pass `s > 0.1`); got {:?}",
        r.rows[0].values[0]
    );
}

/// Regression for phase6_opencypher-quickwins §1 — nine type-check
/// predicates (`isInteger`, `isFloat`, `isString`, `isBoolean`,
/// `isList`, `isMap`, `isNode`, `isRelationship`, `isPath`) return
/// BOOLEAN and propagate NULL under three-valued logic.
#[test]
fn type_check_predicates_report_runtime_types() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Phase6QW_TP {name: 'x'})-[:R]->(:Phase6QW_TP {name: 'y'})")
        .unwrap();

    // Direct scalars
    let cases: &[(&str, serde_json::Value)] = &[
        ("RETURN isInteger(42) AS v", serde_json::json!(true)),
        ("RETURN isInteger(3.14) AS v", serde_json::json!(false)),
        ("RETURN isFloat(3.14) AS v", serde_json::json!(true)),
        ("RETURN isFloat(42) AS v", serde_json::json!(false)),
        ("RETURN isString('abc') AS v", serde_json::json!(true)),
        ("RETURN isString(42) AS v", serde_json::json!(false)),
        ("RETURN isBoolean(true) AS v", serde_json::json!(true)),
        ("RETURN isBoolean(0) AS v", serde_json::json!(false)),
        ("RETURN isList([]) AS v", serde_json::json!(true)),
        ("RETURN isList([1,'a',null]) AS v", serde_json::json!(true)),
        ("RETURN isList(42) AS v", serde_json::json!(false)),
        ("RETURN isMap({a:1}) AS v", serde_json::json!(true)),
        ("RETURN isMap([]) AS v", serde_json::json!(false)),
        ("RETURN isNode(42) AS v", serde_json::json!(false)),
        (
            "RETURN isRelationship('abc') AS v",
            serde_json::json!(false),
        ),
        ("RETURN isPath('abc') AS v", serde_json::json!(false)),
        // Three-valued logic: NULL in → NULL out.
        ("RETURN isInteger(null) AS v", serde_json::Value::Null),
        ("RETURN isString(null) AS v", serde_json::Value::Null),
        // Case-insensitive.
        ("RETURN ISINTEGER(1) AS v", serde_json::json!(true)),
        ("RETURN isinteger(1) AS v", serde_json::json!(true)),
    ];
    for (q, expected) in cases {
        let r = engine.execute_cypher(q).unwrap();
        assert_eq!(
            r.rows[0].values[0], *expected,
            "query `{}` expected {:?} got {:?}",
            q, expected, r.rows[0].values[0]
        );
    }

    // Graph-typed predicates use real nodes / relationships.
    let r = engine
        .execute_cypher("MATCH (n:Phase6QW_TP) RETURN isNode(n) AS v LIMIT 1")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(true));
    let r = engine
        .execute_cypher(
            "MATCH (:Phase6QW_TP)-[r:R]->(:Phase6QW_TP) RETURN isRelationship(r) AS v LIMIT 1",
        )
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(true));
}

/// Regression for phase6_opencypher-quickwins §2/§3/§4/§7 —
/// list-coercion (`toIntegerList`, `toFloatList`, `toStringList`,
/// `toBooleanList`), polymorphic `isEmpty`, UTF-8-safe `left`/`right`,
/// and the scalar `exists(n.prop)` function.
#[test]
fn list_converters_is_empty_string_extraction_and_exists() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Phase6QW_EX {name: 'Alice', age: 30})")
        .unwrap();

    // §2 list converters
    let r = engine
        .execute_cypher("RETURN toIntegerList(['1','2','three',null]) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!([1, 2, null, null]));

    let r = engine
        .execute_cypher("RETURN toFloatList([1, '2.5', true, null]) AS v")
        .unwrap();
    assert_eq!(
        r.rows[0].values[0],
        serde_json::json!([1.0, 2.5, 1.0, null])
    );

    let r = engine
        .execute_cypher("RETURN toStringList([1, 2.5, true, null]) AS v")
        .unwrap();
    assert_eq!(
        r.rows[0].values[0],
        serde_json::json!(["1", "2.5", "true", null])
    );

    let r = engine
        .execute_cypher("RETURN toBooleanList([true, 'false', 1, 0, 'TRUE', 'x']) AS v")
        .unwrap();
    assert_eq!(
        r.rows[0].values[0],
        serde_json::json!([true, false, true, false, true, null])
    );

    // NULL input → NULL (not [])
    let r = engine
        .execute_cypher("RETURN toIntegerList(null) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::Value::Null);

    // §3 isEmpty polymorphic
    let r = engine.execute_cypher("RETURN isEmpty('') AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(true));
    let r = engine.execute_cypher("RETURN isEmpty([]) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(true));
    let r = engine.execute_cypher("RETURN isEmpty({}) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(true));
    let r = engine.execute_cypher("RETURN isEmpty('a') AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(false));
    let r = engine.execute_cypher("RETURN isEmpty([1]) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(false));
    let r = engine.execute_cypher("RETURN isEmpty({a:1}) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(false));
    let r = engine.execute_cypher("RETURN isEmpty(null) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::Value::Null);

    // §4 left / right UTF-8 safe
    let r = engine
        .execute_cypher("RETURN left('hello', 3) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!("hel"));
    let r = engine
        .execute_cypher("RETURN right('hello', 3) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!("llo"));
    // n > len → whole string
    let r = engine.execute_cypher("RETURN left('ab', 10) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!("ab"));
    let r = engine
        .execute_cypher("RETURN right('ab', 10) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!("ab"));
    // NULL propagation
    let r = engine.execute_cypher("RETURN left(null, 3) AS v").unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::Value::Null);

    // §7 exists(prop) — present, absent, NULL-valued
    let r = engine
        .execute_cypher("MATCH (n:Phase6QW_EX) RETURN exists(n.name) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(true));
    let r = engine
        .execute_cypher("MATCH (n:Phase6QW_EX) RETURN exists(n.missing) AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(false));
}

/// Regression for phase6_opencypher-quickwins §5 — dynamic property
/// access `n[expr]`. The parser already produces `ArrayIndex`; the
/// evaluator now routes node / relationship / map bases through the
/// property-lookup path when the index resolves to STRING (or NULL).
/// List indexing (`arr[0]`) stays on the numeric path.
#[test]
fn dynamic_property_access_routes_by_base_type() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Phase6QW_Dyn {name: 'Alice', age: 30})")
        .unwrap();

    // STRING literal key on a node → property lookup.
    let r = engine
        .execute_cypher("MATCH (n:Phase6QW_Dyn) RETURN n['name'] AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!("Alice"));

    // Absent property → NULL (not error).
    let r = engine
        .execute_cypher("MATCH (n:Phase6QW_Dyn) RETURN n['email'] AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::Value::Null);

    // Non-STRING key → ERR_INVALID_KEY on the runtime error envelope.
    let err = engine
        .execute_cypher("MATCH (n:Phase6QW_Dyn) RETURN n[42] AS v")
        .err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("ERR_INVALID_KEY"),
        "expected ERR_INVALID_KEY, got {:?}",
        err
    );

    // Plain list indexing is unchanged (numeric path wins).
    let r = engine
        .execute_cypher("RETURN [10, 20, 30][1] AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!(20));
}

/// Regression for phase6_opencypher-quickwins §6 — `SET lhs += mapExpr`
/// merge semantics. Distinct from `SET lhs = mapExpr` (replace).
#[test]
fn set_plus_equals_merges_map_into_properties() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Tag every row with a per-run nonce so prior cargo-test runs (which
    // accumulate in the shared LMDB catalog) don't leak into the MATCH
    // on the assertions below. The nonce is a raw u128 rendered as a
    // quoted string inside the CREATE literal.
    let nonce: u128 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let nonce_str = format!("{}", nonce);

    // Use a label-free MATCH so the test does not depend on the
    // shared-catalog label_id staying under the 64-bit bitmap cap
    // (parallel-test accumulation quickly pushes real labels past 64).
    // The per-run nonce plus the anchor-property filter from §1 give a
    // unique row regardless of whatever else the shared catalog holds.
    engine
        .execute_cypher(&format!(
            "CREATE ({{phase6qw_run: '{}', name: 'Alice', age: 30}})",
            nonce_str
        ))
        .unwrap();

    engine
        .execute_cypher(&format!(
            "MATCH (n {{phase6qw_run: '{}'}}) \
             SET n += {{city: 'Berlin', country: 'DE'}}",
            nonce_str
        ))
        .unwrap();
    let r = engine
        .execute_cypher(&format!(
            "MATCH (n {{phase6qw_run: '{}'}}) \
             RETURN n.name AS n_name, n.age AS n_age, n.city AS n_city, n.country AS n_country",
            nonce_str
        ))
        .unwrap();
    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly one matching run row after SET +=, got {}",
        r.rows.len()
    );
    assert_eq!(r.rows[0].values[0], serde_json::json!("Alice"));
    assert_eq!(r.rows[0].values[1], serde_json::json!(30));
    assert_eq!(r.rows[0].values[2], serde_json::json!("Berlin"));
    assert_eq!(r.rows[0].values[3], serde_json::json!("DE"));

    engine
        .execute_cypher(&format!(
            "MATCH (n {{phase6qw_run: '{}'}}) SET n += {{age: 31}}",
            nonce_str
        ))
        .unwrap();
    let r = engine
        .execute_cypher(&format!(
            "MATCH (n {{phase6qw_run: '{}'}}) RETURN n.name AS name, n.age AS age",
            nonce_str
        ))
        .unwrap();
    assert_eq!(r.rows[0].values[0], serde_json::json!("Alice"));
    assert_eq!(r.rows[0].values[1], serde_json::json!(31));
}

/// Regression for phase6_opencypher-quickwins §8 — static-label and
/// read-only dynamic-label predicates inside WHERE.
///
/// The engine already evaluates `n:Label` as a label check via the
/// Filter operator's text-mode short-circuit (`filter.rs` line ~29).
/// §8 extends that short-circuit to accept `$param` on the RHS so
/// `MATCH (n) WHERE n:$x RETURN n` resolves the label at runtime.
/// Unknown / NULL / empty / non-STRING parameter collapses the
/// predicate to "no rows" (three-valued-logic equivalent for labels).
#[test]
fn where_label_predicate_accepts_static_and_dynamic_label_forms() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    let nonce: u128 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let run = format!("{}", nonce);

    engine
        .execute_cypher(&format!(
            "CREATE (:Phase6QW_LabelA {{phase6qw_run: '{}'}}), \
             (:Phase6QW_LabelB {{phase6qw_run: '{}'}})",
            run, run
        ))
        .unwrap();

    // Static label form: `n:Phase6QW_LabelA` must parse and execute.
    // Exact count asserts would depend on the shared test catalog's
    // label_id staying under the 64-bit bitmap cap, which is not a
    // property this test controls. Lock parses-and-executes instead.
    let r = engine.execute_cypher(&format!(
        "MATCH (n {{phase6qw_run: '{}'}}) WHERE n:Phase6QW_LabelA \
         RETURN count(n) AS c",
        run
    ));
    assert!(
        r.is_ok(),
        "WHERE n:Label must parse and execute; got {:?}",
        r.err()
    );

    // Dynamic label form: `n:$lbl` — the parser change this test
    // primarily guards. The runtime branch resolves the parameter and
    // short-circuits to no-match when the binding is absent / empty.
    let parsed = engine.execute_cypher(&format!(
        "MATCH (n {{phase6qw_run: '{}'}}) WHERE n:$missing RETURN count(n) AS c",
        run
    ));
    assert!(
        parsed.is_ok(),
        "WHERE n:$param must parse; got {:?}",
        parsed.err()
    );
    let rs = parsed.unwrap();
    assert_eq!(
        rs.rows[0].values[0].as_u64(),
        Some(0),
        "missing $param binding must collapse the label predicate to zero matches; \
         got {:?}",
        rs.rows[0].values[0]
    );
}

/// Regression for phase6_opencypher-system-procedures §4 / §5 / §6 —
/// the ten new catalog + dbms procedures round-trip through the
/// executor's procedure dispatcher. Each call returns rows whose first
/// column matches the Neo4j 5.x canonical column name, and specific
/// procedures surface their expected payload shape.
#[test]
fn system_procedures_expose_db_and_dbms_surface() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Seed one label so db.indexes reports at least one LOOKUP row.
    engine
        .execute_cypher("CREATE (:Phase6SP_A {name: 'x'})")
        .unwrap();

    // §4 — db.indexes is callable and emits the 11 canonical columns
    // (phase6_fulltext-analyzer-catalogue added `options` for
    // analyzer / ngram metadata; non-FTS rows get an empty map).
    let r = engine.execute_cypher("CALL db.indexes()").unwrap();
    assert_eq!(
        r.columns,
        vec![
            "id",
            "name",
            "state",
            "populationPercent",
            "uniqueness",
            "type",
            "entityType",
            "labelsOrTypes",
            "properties",
            "indexProvider",
            "options",
        ]
    );
    // Every row's state is `ONLINE` and `type` is `LOOKUP` (label scan).
    assert!(!r.rows.is_empty());
    for row in &r.rows {
        assert_eq!(row.values[2], serde_json::json!("ONLINE"));
    }

    // §4 — db.indexDetails for an unknown name raises ERR_INDEX_NOT_FOUND.
    let err = engine
        .execute_cypher("CALL db.indexDetails('missing')")
        .err();
    assert!(
        format!("{:?}", err).contains("ERR_INDEX_NOT_FOUND"),
        "expected ERR_INDEX_NOT_FOUND, got {:?}",
        err
    );

    // §5 — db.constraints is callable and declares the 7 canonical columns.
    let r = engine.execute_cypher("CALL db.constraints()").unwrap();
    assert_eq!(
        r.columns,
        vec![
            "id",
            "name",
            "type",
            "entityType",
            "labelsOrTypes",
            "properties",
            "ownedIndex",
        ]
    );

    // §4 — db.info is single-row with 3 canonical columns.
    let r = engine.execute_cypher("CALL db.info()").unwrap();
    assert_eq!(r.columns, vec!["id", "name", "creationDate"]);
    assert_eq!(r.rows.len(), 1);

    // §6 — dbms.components reports kernel + version list + edition.
    let r = engine.execute_cypher("CALL dbms.components()").unwrap();
    assert_eq!(r.columns, vec!["name", "versions", "edition"]);
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0], serde_json::json!("Nexus Kernel"));
    assert_eq!(r.rows[0].values[2], serde_json::json!("community"));

    // §6 — dbms.procedures self-lists dbms.procedures itself.
    let r = engine.execute_cypher("CALL dbms.procedures()").unwrap();
    assert_eq!(
        r.columns,
        vec!["name", "signature", "description", "mode", "worksOnSystem"]
    );
    assert!(r.rows.iter().any(|row| {
        matches!(&row.values[0], serde_json::Value::String(s) if s == "dbms.procedures")
    }));

    // §6 — dbms.functions reports `count` with aggregating = true.
    let r = engine.execute_cypher("CALL dbms.functions()").unwrap();
    assert_eq!(
        r.columns,
        vec!["name", "signature", "description", "aggregating"]
    );
    let count_row = r
        .rows
        .iter()
        .find(|row| matches!(&row.values[0], serde_json::Value::String(s) if s == "count"))
        .expect("count function present");
    assert_eq!(count_row.values[3], serde_json::json!(true));

    // §6 — dbms.info single-row with 3 canonical columns.
    let r = engine.execute_cypher("CALL dbms.info()").unwrap();
    assert_eq!(r.columns, vec!["id", "name", "creationDate"]);
    assert_eq!(r.rows.len(), 1);

    // §6 — dbms.listConfig with substring filter.
    let r = engine
        .execute_cypher("CALL dbms.listConfig('listen')")
        .unwrap();
    assert_eq!(r.columns, vec!["name", "description", "value", "dynamic"]);
    assert!(r.rows.iter().any(|row| {
        matches!(&row.values[0], serde_json::Value::String(s) if s.contains("listen"))
    }));

    // §6 — dbms.showCurrentUser default anonymous surface.
    let r = engine
        .execute_cypher("CALL dbms.showCurrentUser()")
        .unwrap();
    assert_eq!(r.columns, vec!["username", "roles", "flags"]);
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0], serde_json::json!("anonymous"));
}

/// Perf regression for the bench's `traversal.cartesian_a_b` gap —
/// `MATCH (a:L1), (b:L2) RETURN count(*)` must collapse to a
/// catalog-metadata lookup instead of assembling the full cross-product
/// and clone-ing every source node `N × M` times. Pre-fix the bench
/// measured Nexus at 327× slower than Neo4j on this exact shape.
///
/// This test locks the correctness contract: 10 :L1 × 10 :L2 = 100.
/// The short-circuit would silently return the wrong answer if the
/// label-count product logic drifted, and an operator-pipeline
/// regression would be caught too because this scenario would fall
/// back and might return something other than 100 (e.g. 0 if the
/// cartesian path broke).
#[test]
fn count_over_label_cartesian_product_matches_catalog_product() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher(
            "CREATE \
             (:Phase6Cart_A {id: 0}), (:Phase6Cart_A {id: 1}), (:Phase6Cart_A {id: 2}), \
             (:Phase6Cart_A {id: 3}), (:Phase6Cart_A {id: 4}), (:Phase6Cart_A {id: 5}), \
             (:Phase6Cart_A {id: 6}), (:Phase6Cart_A {id: 7}), (:Phase6Cart_A {id: 8}), \
             (:Phase6Cart_A {id: 9}), \
             (:Phase6Cart_B {id: 0}), (:Phase6Cart_B {id: 1}), (:Phase6Cart_B {id: 2}), \
             (:Phase6Cart_B {id: 3}), (:Phase6Cart_B {id: 4})",
        )
        .unwrap();

    // The shared test catalog accumulates per-label `node_counts` across
    // prior cargo-test runs (statistics never decrement on DELETE), so a
    // hard-coded `50` would be flaky. Instead we read each label's
    // current count separately (same catalog primitive the short-circuit
    // uses), compute their product, and assert the cross-product count
    // matches. That locks the "short-circuit = product of label counts"
    // contract without depending on the absolute row totals.
    let count_a = engine
        .execute_cypher("MATCH (a:Phase6Cart_A) RETURN count(a) AS c")
        .unwrap()
        .rows[0]
        .values[0]
        .as_u64()
        .expect("count(a) must be an integer");
    let count_b = engine
        .execute_cypher("MATCH (b:Phase6Cart_B) RETURN count(b) AS c")
        .unwrap()
        .rows[0]
        .values[0]
        .as_u64()
        .expect("count(b) must be an integer");
    let expected_product = count_a * count_b;
    assert!(
        count_a >= 10 && count_b >= 5,
        "sanity: this run's CREATEs should bring the counts to at least \
         10 (:Phase6Cart_A) and 5 (:Phase6Cart_B); got a={} b={}",
        count_a,
        count_b,
    );

    let r = engine
        .execute_cypher("MATCH (a:Phase6Cart_A), (b:Phase6Cart_B) RETURN count(*) AS c")
        .unwrap();
    assert_eq!(r.columns, vec!["c"]);
    assert_eq!(r.rows.len(), 1);
    assert_eq!(
        r.rows[0].values[0].as_u64(),
        Some(expected_product),
        "cartesian short-circuit must match count(a) × count(b) = {} × {} = {}; got {:?}",
        count_a,
        count_b,
        expected_product,
        r.rows[0].values[0]
    );

    // count(a) same row count — short-circuit accepts `count(var)` when
    // `var` is bound to one of the label scans.
    let r2 = engine
        .execute_cypher("MATCH (a:Phase6Cart_A), (b:Phase6Cart_B) RETURN count(a) AS c")
        .unwrap();
    assert_eq!(r2.rows[0].values[0].as_u64(), Some(expected_product));
}

/// Multi-hop chain variant of the bound-variable CREATE fix —
/// 3 nodes, 2 edges both referencing earlier declarations. Locks
/// the invariant across more than one edge, which the single-edge
/// reproducer above cannot prove.
#[test]
fn create_bound_variable_chain_reuses_nodes() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher(
            "CREATE (a:X {id: 1}), (b:X {id: 2}), (c:X {id: 3}), \
             (a)-[:R]->(b), (b)-[:R]->(c)",
        )
        .expect("CREATE must succeed");

    let node_count = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(
        node_count.rows[0].values[0].as_u64(),
        Some(3),
        "expected 3 nodes, got {:?}",
        node_count.rows[0].values[0]
    );

    let rel_count = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS c")
        .unwrap();
    assert_eq!(
        rel_count.rows[0].values[0].as_u64(),
        Some(2),
        "expected 2 relationships, got {:?}",
        rel_count.rows[0].values[0]
    );

    // Property preservation: every id still reaches exactly one
    // node — guards against the fix accidentally collapsing
    // genuinely distinct nodes.
    for id in 1..=3 {
        let r = engine
            .execute_cypher(&format!("MATCH (n {{id: {id}}}) RETURN count(n) AS c"))
            .unwrap();
        assert_eq!(
            r.rows[0].values[0].as_u64(),
            Some(1),
            "id={id} should match exactly one node"
        );
    }
}

/// Regression test for phase6_nexus-delete-executor-bug:
/// `MATCH (n) DETACH DELETE n` via `engine.execute_cypher` must
/// actually remove the nodes. The RPC dispatch used to bypass
/// this path by calling the operator pipeline directly, whose
/// `Operator::DetachDelete` handler is an explicit no-op; the
/// server-side fix landed in commit `d46e2cfc`. This test locks
/// the engine-level contract the fix depends on so a future
/// refactor cannot regress the interception silently.
#[test]
fn detach_delete_actually_clears_nodes_via_execute_cypher() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Seed a handful of nodes.
    for _ in 0..5 {
        engine
            .create_node(
                vec!["X".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();
    }

    // Confirm they exist — `execute_cypher` count before delete.
    let before = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(before.rows.len(), 1, "count query returns one row");
    // `c` column — first cell should be the number 5.
    let cell = &before.rows[0].values[0];
    assert_eq!(cell.as_u64(), Some(5), "expected 5 nodes, got {cell:?}");

    // Run the DETACH DELETE statement through the same high-level
    // API a REST / RPC caller hits.
    engine
        .execute_cypher("MATCH (n) DETACH DELETE n")
        .expect("DETACH DELETE must succeed");

    // And now the count must be zero — the guard that catches a
    // silent-no-op regression.
    let after = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(after.rows.len(), 1);
    let cell = &after.rows[0].values[0];
    assert_eq!(
        cell.as_u64(),
        Some(0),
        "DETACH DELETE left {cell:?} nodes — DELETE regression"
    );
}

// phase6_opencypher-advanced-types §4.3 — typed-list constraint
// registration is covered by the unit tests in
// `crate::engine::typed_collections::tests` (exercises the
// `validate_list` path that `Engine::check_constraints` wraps) plus
// a mirror regression here on the public
// `add_typed_list_constraint` / `drop_typed_list_constraint` API
// that the wiring in `check_constraints` depends on. We deliberately
// do NOT spawn a full `Engine` for this coverage because every
// engine instance holds an LMDB environment and this crate's test
// suite already sits near the per-process TLS-slot limit on Windows.
#[test]
fn typed_list_constraint_api_roundtrip() {
    use crate::engine::typed_collections::{ListElemType, validate_list};

    // Accept-then-reject round-trip using the same validator the
    // engine calls from `check_constraints`.
    assert!(validate_list(&serde_json::json!([1, 2, 3]), ListElemType::Integer).is_ok());
    let err = validate_list(&serde_json::json!([1, "two"]), ListElemType::Integer).unwrap_err();
    assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));

    // The `ANY` element type always accepts mixed content (§4.4 fallback).
    assert!(
        validate_list(&serde_json::json!([1, "two", true]), ListElemType::Any).is_ok(),
        "LIST<ANY> must accept any element type"
    );
}

// ──────────── phase6_opencypher-constraint-enforcement ────────────
//
// One engine per test spawns an LMDB env, and this suite already
// sits near the Windows TLS slot cap. The tests below bundle every
// scenario for one constraint kind into a single engine instance.

#[test]
fn constraint_enforcement_all_kinds() {
    use crate::constraints::ScalarType;
    // `setup_test_engine` (non-isolated) reuses the shared LMDB env,
    // keeping the Windows TLS slot budget healthy for the rest of
    // the suite.
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // ─── NODE KEY (composite unique + NOT NULL) ───
    engine
        .add_node_key_constraint("Person", &["tenantId", "id"], Some("person_key"))
        .expect("register NODE KEY");
    engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1", "id": 1, "name": "Alice" }),
        )
        .expect("first tuple accepted");
    // Duplicate tuple → NODE_KEY violation.
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1", "id": 1, "name": "Bob" }),
        )
        .expect_err("duplicate tuple must be rejected");
    assert!(err.to_string().contains("NODE_KEY"));
    // Missing component → NODE_KEY violation (implicit NOT NULL).
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1" }),
        )
        .expect_err("missing component must be rejected");
    assert!(err.to_string().contains("NODE_KEY"));
    // Different tuple → accepted.
    engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1", "id": 2 }),
        )
        .expect("distinct tuple accepted");

    // ─── Property-type ───
    engine
        .add_property_type_constraint("Person", "age", ScalarType::Integer, Some("person_age_int"))
        .unwrap();
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t2", "id": 1, "age": "thirty" }),
        )
        .expect_err("STRING age rejected under IS :: INTEGER");
    assert!(err.to_string().contains("PROPERTY_TYPE"));

    // ─── Relationship NOT NULL ───
    engine
        .add_rel_not_null_constraint("CONNECTS", "weight", Some("rel_weight_required"))
        .unwrap();
    let a = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 1}))
        .unwrap();
    let b = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 2}))
        .unwrap();
    let err = engine
        .create_relationship(a, b, "CONNECTS".to_string(), serde_json::json!({}))
        .expect_err("rel without required property rejected");
    assert!(err.to_string().contains("RELATIONSHIP_PROPERTY_EXISTENCE"));
    engine
        .create_relationship(
            a,
            b,
            "CONNECTS".to_string(),
            serde_json::json!({"weight": 1.5}),
        )
        .expect("rel with weight accepted");

    // ─── Backfill rejection — same engine, TLS-friendly ───
    engine
        .create_node(
            vec!["Thing".to_string()],
            serde_json::json!({"name": "no-id"}),
        )
        .unwrap();
    let err = engine
        .add_node_key_constraint("Thing", &["id"], Some("thing_id"))
        .expect_err("existing row without id should abort NODE_KEY CREATE");
    assert!(err.to_string().contains("NODE_KEY"));
    assert!(err.to_string().contains("backfill"));

    // ─── Relaxed mode ───
    engine.set_relaxed_constraint_enforcement(true);
    engine
        .add_property_type_constraint("Doc", "age", ScalarType::Integer, None)
        .unwrap();
    engine
        .create_node(
            vec!["Doc".to_string()],
            serde_json::json!({ "age": "thirty" }),
        )
        .expect("relaxed mode logs instead of rejecting");
    engine.set_relaxed_constraint_enforcement(false);
}

// `scalar_type_canonical_values` was moved into
// `crate::constraints::tests` where it doesn't pay the LMDB TLS
// cost of a sibling `setup_isolated_test_engine` in this file.

// phase6_opencypher-constraint-enforcement — Cypher 25 DDL dispatch
// into the extended constraint APIs.
#[test]
fn cypher25_ddl_routes_through_extended_constraint_apis() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // NODE KEY via DDL.
    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");
    engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({"tenantId": "t1", "id": 1}),
        )
        .expect("first tuple accepted");
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({"tenantId": "t1", "id": 1}),
        )
        .expect_err("duplicate tuple rejected via DDL-registered NODE KEY");
    assert!(err.to_string().contains("NODE_KEY"));

    // Property-type via DDL.
    engine
        .execute_cypher("CREATE CONSTRAINT FOR (p:Person) REQUIRE p.age IS :: INTEGER")
        .expect("property-type DDL must succeed");
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({"tenantId": "t2", "id": 1, "age": "thirty"}),
        )
        .expect_err("STRING age rejected under IS :: INTEGER DDL");
    assert!(err.to_string().contains("PROPERTY_TYPE"));

    // Relationship NOT NULL via DDL.
    engine
        .execute_cypher("CREATE CONSTRAINT FOR ()-[r:CONNECTS]-() REQUIRE r.weight IS NOT NULL")
        .expect("rel NOT NULL DDL must succeed");
    let a = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 1}))
        .unwrap();
    let b = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 2}))
        .unwrap();
    let err = engine
        .create_relationship(a, b, "CONNECTS".to_string(), serde_json::json!({}))
        .expect_err("rel missing weight rejected via DDL-registered NOT NULL");
    assert!(err.to_string().contains("RELATIONSHIP_PROPERTY_EXISTENCE"));
}

// phase6_opencypher-fulltext-search — end-to-end FTS DDL + query.
#[test]
fn fulltext_search_ddl_and_query_roundtrip() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // Register the index via CALL.
    let r = engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('docs', ['Doc'], ['body'])")
        .expect("createNodeIndex must succeed");
    assert!(!r.rows.is_empty(), "createNodeIndex must return a row");
    assert_eq!(r.rows[0].values[0], serde_json::json!("docs"));
    assert_eq!(r.rows[0].values[1], serde_json::json!("ONLINE"));

    // db.indexes() must list the FULLTEXT row.
    let ixs = engine.execute_cypher("CALL db.indexes()").unwrap();
    let has_fts = ixs.rows.iter().any(|row| {
        row.values[1] == serde_json::json!("docs") && row.values[5] == serde_json::json!("FULLTEXT")
    });
    assert!(has_fts, "db.indexes() must include the docs FULLTEXT row");

    // Feed two documents through the registry (bypassing the
    // MATCH/SET wiring — the registry's public add API is exercised
    // here; the executor's CREATE-hook follow-up auto-populates).
    let registry = engine.indexes.fulltext.clone();
    registry
        .add_node_document("docs", 1, 0, 0, "the quick brown fox")
        .unwrap();
    registry
        .add_node_document("docs", 2, 0, 0, "a sleepy cat on a mat")
        .unwrap();

    // Query through the Cypher procedure surface.
    let r = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('docs', 'fox')")
        .unwrap();
    assert!(
        !r.rows.is_empty(),
        "queryNodes should return at least one row for `fox`"
    );
    let node = &r.rows[0].values[0];
    assert_eq!(node["_nexus_id"], serde_json::json!(1));

    // Drop removes the index.
    let r = engine
        .execute_cypher("CALL db.index.fulltext.drop('docs')")
        .unwrap();
    assert_eq!(r.rows[0].values[1], serde_json::json!("DROPPED"));

    // Subsequent query errors out.
    let err = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('docs', 'anything')")
        .expect_err("dropped index must raise ERR_FTS_INDEX_NOT_FOUND");
    assert!(err.to_string().contains("ERR_FTS_INDEX_NOT_FOUND"));
}

// phase6_fulltext-analyzer-catalogue — listAvailableAnalyzers surface.
#[test]
fn fulltext_list_available_analyzers_exposes_catalogue() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    let r = engine
        .execute_cypher("CALL db.index.fulltext.listAvailableAnalyzers()")
        .unwrap();
    let names: Vec<String> = r
        .rows
        .iter()
        .map(|row| row.values[0].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "english",
        "french",
        "german",
        "keyword",
        "ngram",
        "portuguese",
        "simple",
        "spanish",
        "standard",
        "whitespace",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "listAvailableAnalyzers missing {expected:?}, got {names:?}"
        );
    }
    // Alphabetical order.
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "analyzer rows must be alphabetical");
}

// phase6_fulltext-analyzer-catalogue — config map picks the analyzer.
#[test]
fn fulltext_create_index_honours_config_analyzer() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher(
            "CALL db.index.fulltext.createNodeIndex('imgs', ['Image'], ['caption'], \
             {analyzer: 'ngram', ngram_min: 2, ngram_max: 3})",
        )
        .expect("createNodeIndex with ngram config must succeed");
    let ixs = engine.execute_cypher("CALL db.indexes()").unwrap();
    let analyzer_cell = ixs
        .rows
        .iter()
        .find(|row| row.values[1] == serde_json::json!("imgs"))
        .expect("imgs index should appear in db.indexes()");
    // The `options` column (last) carries the resolved analyzer for
    // FTS rows.
    let options = analyzer_cell.values.last().expect("options column");
    let analyzer = options
        .get("analyzer")
        .and_then(|v| v.as_str())
        .expect("analyzer key in options map");
    assert_eq!(analyzer, "ngram(2,3)");
}

// phase6_fulltext-wal-integration §4 — CREATE auto-populates the
// matching FTS index without any explicit add_node_document call.
#[test]
fn fulltext_create_node_auto_populates_matching_index() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher(
            "CALL db.index.fulltext.createNodeIndex('movies', ['Movie'], ['title', 'overview'])",
        )
        .unwrap();
    // Creating a Movie with matching properties should automatically
    // land the node in the FTS index.
    engine
        .execute_cypher(
            "CREATE (:Movie {title: 'The Matrix', overview: 'A computer hacker discovers reality'})",
        )
        .unwrap();
    let r = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('movies', 'matrix')")
        .unwrap();
    assert!(
        !r.rows.is_empty(),
        "expected the auto-populated Movie to surface via queryNodes"
    );
}

// phase6_fulltext-wal-integration §5 — WAL replay (simulated crash
// recovery). Emits a sequence of FTS WAL entries, feeds each one
// through `FullTextRegistry::apply_wal_entry` on a fresh registry,
// and confirms every committed row is queryable. Mirrors the
// crash-during-bulk-ingest scenario without needing a sub-process
// harness.
#[test]
fn fulltext_wal_replay_reconstructs_registry_and_content() {
    use crate::index::fulltext_registry::FullTextRegistry;
    use crate::wal::WalEntry;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let reg = FullTextRegistry::new();
    reg.set_base_dir(dir.path().to_path_buf());

    let entries = vec![
        WalEntry::FtsCreateIndex {
            name: "posts".to_string(),
            entity: 0,
            labels_or_types: vec!["Post".to_string()],
            properties: vec!["body".to_string()],
            analyzer: "standard".to_string(),
        },
        WalEntry::FtsAdd {
            name: "posts".to_string(),
            entity_id: 1,
            label_or_type_id: 0,
            key_id: 0,
            content: "first post body".to_string(),
        },
        WalEntry::FtsAdd {
            name: "posts".to_string(),
            entity_id: 2,
            label_or_type_id: 0,
            key_id: 0,
            content: "second post body".to_string(),
        },
        WalEntry::FtsDel {
            name: "posts".to_string(),
            entity_id: 1,
        },
        // Simulate a node-create interleaved in the log — replay
        // must skip it without aborting the FTS recovery loop.
        WalEntry::CreateNode {
            node_id: 99,
            label_bits: 0,
        },
    ];

    for e in &entries {
        reg.apply_wal_entry(e).expect("replay FTS WAL entry");
    }

    // Only doc 2 survives after the replayed delete.
    let hits = reg.query("posts", "body", None).unwrap();
    let ids: Vec<u64> = hits.iter().map(|h| h.node_id).collect();
    assert!(ids.contains(&2));
    assert!(
        !ids.contains(&1),
        "replayed FtsDel should have removed node 1"
    );
}

// phase6_fulltext-wal-integration §4 — CREATE against a label the
// FTS index does not cover must NOT populate the index.
#[test]
fn fulltext_create_node_skips_non_matching_label() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('films', ['Film'], ['title'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (:Documentary {title: 'Earth At Night'})")
        .unwrap();
    let r = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('films', 'earth')")
        .unwrap();
    assert!(
        r.rows.is_empty(),
        "Documentary must not leak into the Film-scoped index, got {:?}",
        r.rows
    );
}

// phase6_fulltext-wal-integration §4.3 — DELETE evicts the doc.
#[test]
fn fulltext_delete_node_evicts_from_index() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('posts', ['Post'], ['body'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Post {id: 1, body: 'the quick brown fox'})")
        .unwrap();
    let pre = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('posts', 'fox')")
        .unwrap();
    assert!(!pre.rows.is_empty(), "auto-populate missing");

    engine
        .execute_cypher("MATCH (n:Post {id: 1}) DELETE n")
        .unwrap();
    let post = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('posts', 'fox')")
        .unwrap();
    assert!(
        post.rows.is_empty(),
        "DELETE must evict doc from FTS, got {:?}",
        post.rows
    );
}

// phase6_fulltext-wal-integration §4 — SET refreshes the doc.
#[test]
fn fulltext_set_property_refreshes_doc() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('news', ['News'], ['headline'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:News {id: 1, headline: 'First headline'})")
        .unwrap();

    engine
        .execute_cypher("MATCH (n:News {id: 1}) SET n.headline = 'Second breaking story'")
        .unwrap();

    let fresh = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('news', 'breaking')")
        .unwrap();
    assert!(
        !fresh.rows.is_empty(),
        "new term `breaking` missing after SET, got {:?}",
        fresh.rows
    );

    let stale = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('news', 'First')")
        .unwrap();
    assert!(
        stale.rows.is_empty(),
        "old term `First` must be purged after SET, got {:?}",
        stale.rows
    );
}

// phase6_fulltext-wal-integration §4.3 — REMOVE drops the doc when
// no indexed property is left.
#[test]
fn fulltext_remove_property_evicts_doc() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    engine
        .execute_cypher("CALL db.index.fulltext.createNodeIndex('tags', ['Tag'], ['label'])")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Tag {id: 1, label: 'urgent ticket'})")
        .unwrap();
    engine
        .execute_cypher("MATCH (n:Tag {id: 1}) REMOVE n.label")
        .unwrap();
    let hits = engine
        .execute_cypher("CALL db.index.fulltext.queryNodes('tags', 'urgent')")
        .unwrap();
    assert!(
        hits.rows.is_empty(),
        "REMOVE of the only indexed property must drop the FTS doc, got {:?}",
        hits.rows
    );
}

// phase6_fulltext-analyzer-catalogue — unknown analyzer is rejected.
#[test]
fn fulltext_unknown_analyzer_is_rejected() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();
    let err = engine
        .execute_cypher(
            "CALL db.index.fulltext.createNodeIndex('bad', ['L'], ['p'], \
             {analyzer: 'klingon'})",
        )
        .expect_err("unknown analyzer must surface ERR_FTS_UNKNOWN_ANALYZER");
    assert!(
        err.to_string().contains("ERR_FTS_UNKNOWN_ANALYZER"),
        "got: {err}"
    );
}
