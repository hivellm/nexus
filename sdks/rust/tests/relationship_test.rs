//! Integration tests for relationship operations (update and delete)

use nexus_sdk::{NexusClient, Value};
use std::collections::HashMap;

// Note: These tests require a running Nexus server at http://localhost:15474
// They are skipped by default unless NEXUS_TEST_SERVER is set

#[tokio::test]
#[ignore]
async fn test_update_relationship() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // First create source and target nodes
    let mut source_props = HashMap::new();
    source_props.insert("name".to_string(), Value::String("SourceNode".to_string()));
    let source_response = client
        .create_node(vec!["UpdateRelSource".to_string()], source_props)
        .await
        .unwrap();

    let mut target_props = HashMap::new();
    target_props.insert("name".to_string(), Value::String("TargetNode".to_string()));
    let target_response = client
        .create_node(vec!["UpdateRelTarget".to_string()], target_props)
        .await
        .unwrap();

    // Create a relationship
    let mut rel_props = HashMap::new();
    rel_props.insert("weight".to_string(), Value::Float(1.0));
    let create_response = client
        .create_relationship(
            source_response.node_id,
            target_response.node_id,
            "UPDATE_TEST".to_string(),
            rel_props,
        )
        .await
        .unwrap();

    // Update the relationship
    let mut update_props = HashMap::new();
    update_props.insert("weight".to_string(), Value::Float(2.5));
    update_props.insert("since".to_string(), Value::Int(2024));

    let update_response = client
        .update_relationship(create_response.rel_id, update_props)
        .await
        .unwrap();

    assert!(update_response.error.is_none());
    assert_eq!(update_response.message, "Relationship updated successfully");
}

#[tokio::test]
#[ignore]
async fn test_delete_relationship() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // First create source and target nodes
    let mut source_props = HashMap::new();
    source_props.insert(
        "name".to_string(),
        Value::String("DeleteSource".to_string()),
    );
    let source_response = client
        .create_node(vec!["DeleteRelSource".to_string()], source_props)
        .await
        .unwrap();

    let mut target_props = HashMap::new();
    target_props.insert(
        "name".to_string(),
        Value::String("DeleteTarget".to_string()),
    );
    let target_response = client
        .create_node(vec!["DeleteRelTarget".to_string()], target_props)
        .await
        .unwrap();

    // Create a relationship
    let mut rel_props = HashMap::new();
    rel_props.insert("temp".to_string(), Value::String("to_delete".to_string()));
    let create_response = client
        .create_relationship(
            source_response.node_id,
            target_response.node_id,
            "DELETE_TEST".to_string(),
            rel_props,
        )
        .await
        .unwrap();

    // Delete the relationship
    let delete_response = client
        .delete_relationship(create_response.rel_id)
        .await
        .unwrap();

    assert!(delete_response.error.is_none());
    assert_eq!(delete_response.message, "Relationship deleted successfully");
}

#[tokio::test]
#[ignore]
async fn test_update_relationship_multiple_properties() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // Create nodes
    let mut source_props = HashMap::new();
    source_props.insert(
        "name".to_string(),
        Value::String("MultiPropSource".to_string()),
    );
    let source_response = client
        .create_node(vec!["MultiPropSource".to_string()], source_props)
        .await
        .unwrap();

    let mut target_props = HashMap::new();
    target_props.insert(
        "name".to_string(),
        Value::String("MultiPropTarget".to_string()),
    );
    let target_response = client
        .create_node(vec!["MultiPropTarget".to_string()], target_props)
        .await
        .unwrap();

    // Create relationship
    let create_response = client
        .create_relationship(
            source_response.node_id,
            target_response.node_id,
            "MULTI_PROP_TEST".to_string(),
            HashMap::new(),
        )
        .await
        .unwrap();

    // Update with multiple properties
    let mut update_props = HashMap::new();
    update_props.insert("prop1".to_string(), Value::String("value1".to_string()));
    update_props.insert("prop2".to_string(), Value::Int(42));
    update_props.insert("prop3".to_string(), Value::Bool(true));
    update_props.insert("prop4".to_string(), Value::Float(std::f64::consts::PI));

    let update_response = client
        .update_relationship(create_response.rel_id, update_props)
        .await
        .unwrap();

    assert!(update_response.error.is_none());
}

#[tokio::test]
#[ignore]
async fn test_delete_nonexistent_relationship() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    // Try to delete a non-existent relationship
    // This should still return success (Cypher DELETE doesn't fail if nothing matches)
    let delete_response = client.delete_relationship(999999).await.unwrap();

    // The operation should complete without error
    assert!(delete_response.error.is_none());
}
