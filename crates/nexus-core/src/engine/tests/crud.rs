//! Tests for node/relationship CRUD operations, graph conversion, clustering,
//! export, graph statistics, and data clearing.

use super::*;
use crate::testing::setup_isolated_test_engine;

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
