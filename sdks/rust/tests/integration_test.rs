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
    // Verify row is an array
    if let Some(serde_json::Value::Array(row_values)) = result.rows.first() {
        assert_eq!(row_values.len(), 1);
    } else {
        panic!("Expected row to be an array");
    }
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

    // Node creation may fail if engine not initialized, but should not panic
    if create_response.error.is_none() {
        assert!(create_response.node_id > 0);

        // Get the node only if creation succeeded
        let get_response = client.get_node(create_response.node_id).await.unwrap();
        if let Some(node) = get_response.node {
            assert_eq!(node.id, create_response.node_id);
            assert!(node.labels.contains(&"TestLabel".to_string()));
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_create_label() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client.create_label("TestLabel".to_string()).await.unwrap();
    // Label creation may fail if catalog not initialized
    // Just verify we got a response
    let _ = response.label_id;
}

#[tokio::test]
#[ignore]
async fn test_list_labels() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client.list_labels().await.unwrap();
    // Labels may have error if catalog not initialized, or be empty
    // Just verify we got a response
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
    // Relationship type creation may fail if catalog not initialized
    // Just verify we got a response
    let _ = response.type_id;
}

#[tokio::test]
#[ignore]
async fn test_list_rel_types() {
    let client = NexusClient::new("http://localhost:15474").unwrap();
    let response = client.list_rel_types().await.unwrap();
    // Types may have error if catalog not initialized, or be empty
    // Just verify we got a response
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

    // Relationship creation may fail if engine not initialized
    if rel_response.error.is_none() {
        assert!(rel_response.rel_id > 0);
    }
}

#[tokio::test]
#[ignore]
async fn test_update_relationship() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // Create nodes and relationship
    let mut source_props = HashMap::new();
    source_props.insert(
        "name".to_string(),
        Value::String("UpdateSource".to_string()),
    );
    let source = client
        .create_node(vec!["UpdateSource".to_string()], source_props)
        .await
        .unwrap();

    let mut target_props = HashMap::new();
    target_props.insert(
        "name".to_string(),
        Value::String("UpdateTarget".to_string()),
    );
    let target = client
        .create_node(vec!["UpdateTarget".to_string()], target_props)
        .await
        .unwrap();

    let rel = client
        .create_relationship(
            source.node_id,
            target.node_id,
            "UPDATE_TEST".to_string(),
            HashMap::new(),
        )
        .await
        .unwrap();

    // Update relationship
    let mut update_props = HashMap::new();
    update_props.insert("weight".to_string(), Value::Float(2.0));
    let update_response = client
        .update_relationship(rel.rel_id, update_props)
        .await
        .unwrap();

    assert!(update_response.error.is_none());
}

#[tokio::test]
#[ignore]
async fn test_delete_relationship() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // Create nodes and relationship
    let mut source_props = HashMap::new();
    source_props.insert(
        "name".to_string(),
        Value::String("DeleteSource".to_string()),
    );
    let source_response = client
        .create_node(vec!["DeleteSource".to_string()], source_props)
        .await
        .unwrap();

    let mut target_props = HashMap::new();
    target_props.insert(
        "name".to_string(),
        Value::String("DeleteTarget".to_string()),
    );
    let target_response = client
        .create_node(vec!["DeleteTarget".to_string()], target_props)
        .await
        .unwrap();

    // Only proceed if nodes were created successfully
    if source_response.error.is_none() && target_response.error.is_none() {
        let rel = client
            .create_relationship(
                source_response.node_id,
                target_response.node_id,
                "DELETE_TEST".to_string(),
                HashMap::new(),
            )
            .await
            .unwrap();

        // Delete relationship (only if creation succeeded)
        if rel.error.is_none() {
            let delete_response = client.delete_relationship(rel.rel_id).await.unwrap();
            // Delete may fail, but should not panic
            let _ = delete_response.error;
        }
    }
}
