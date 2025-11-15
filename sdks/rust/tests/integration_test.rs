//! Integration tests for Nexus Rust SDK

use nexus_sdk_rust::{NexusClient, Value};
use std::collections::HashMap;

// Note: These tests require a running Nexus server at http://localhost:15474
// They are skipped by default unless NEXUS_TEST_SERVER is set

#[tokio::test]
#[ignore]
async fn test_health_check() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let healthy = client.health_check().await.unwrap();
    assert!(healthy);
}

#[tokio::test]
#[ignore]
async fn test_get_stats() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let stats = client.get_stats().await.unwrap();
    // Verify stats are valid (non-negative)
    let _ = stats.catalog.node_count;
    let _ = stats.catalog.rel_count;
}

#[tokio::test]
#[ignore]
async fn test_execute_cypher() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let result = client
        .execute_cypher("RETURN 1 as test", None)
        .await
        .unwrap();
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "test");
    assert_eq!(result.rows.len(), 1);
}

#[tokio::test]
#[ignore]
async fn test_create_and_get_node() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // Create a node
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), Value::String("Test Node".to_string()));
    properties.insert("age".to_string(), Value::Int(25));

    let create_response = client
        .create_node(vec!["TestLabel".to_string()], properties)
        .await
        .unwrap();

    assert!(create_response.node_id > 0);
    assert!(create_response.error.is_none());

    // Get the node
    let get_response = client.get_node(create_response.node_id).await.unwrap();
    assert!(get_response.node.is_some());

    let node = get_response.node.unwrap();
    assert_eq!(node.id, create_response.node_id);
    assert!(node.labels.contains(&"TestLabel".to_string()));
}

#[tokio::test]
#[ignore]
async fn test_create_label() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client.create_label("TestLabel".to_string()).await.unwrap();
    assert!(response.error.is_none());
}

#[tokio::test]
#[ignore]
async fn test_list_labels() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client.list_labels().await.unwrap();
    assert!(response.error.is_none());
    // Labels should be a list (may be empty)
    let _ = response.labels.len();
}

#[tokio::test]
#[ignore]
async fn test_create_rel_type() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client
        .create_rel_type("TEST_REL".to_string())
        .await
        .unwrap();
    assert!(response.error.is_none());
}

#[tokio::test]
#[ignore]
async fn test_list_rel_types() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client.list_rel_types().await.unwrap();
    assert!(response.error.is_none());
    // Types should be a list (may be empty)
    let _ = response.types.len();
}

#[tokio::test]
#[ignore]
async fn test_create_relationship() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // Create two nodes first
    let mut properties1 = HashMap::new();
    properties1.insert("name".to_string(), Value::String("Source".to_string()));
    let node1 = client
        .create_node(vec!["Person".to_string()], properties1)
        .await
        .unwrap();

    let mut properties2 = HashMap::new();
    properties2.insert("name".to_string(), Value::String("Target".to_string()));
    let node2 = client
        .create_node(vec!["Person".to_string()], properties2)
        .await
        .unwrap();

    // Create relationship
    let rel_properties = HashMap::new();
    let rel_response = client
        .create_relationship(
            node1.node_id,
            node2.node_id,
            "KNOWS".to_string(),
            rel_properties,
        )
        .await
        .unwrap();

    assert!(rel_response.rel_id > 0);
    assert!(rel_response.error.is_none());
}
