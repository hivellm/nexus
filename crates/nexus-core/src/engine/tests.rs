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
