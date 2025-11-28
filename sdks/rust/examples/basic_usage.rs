//! Basic usage example for Nexus Rust SDK

use nexus_sdk::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Create a client
    let client = NexusClient::new("http://localhost:15474")?;

    // Check server health
    tracing::info!("Checking server health...");
    let healthy = client.health_check().await?;
    tracing::info!("Server is healthy: {}", healthy);

    // Get database statistics
    tracing::info!("\nGetting database statistics...");
    let stats = client.get_stats().await?;
    tracing::info!("Nodes: {}", stats.catalog.node_count);
    tracing::info!("Relationships: {}", stats.catalog.rel_count);
    tracing::info!("Labels: {}", stats.catalog.label_count);
    tracing::info!("Relationship types: {}", stats.catalog.rel_type_count);

    // Execute a simple Cypher query
    tracing::info!("\nExecuting Cypher query...");
    let result = client
        .execute_cypher("RETURN 'Hello, Nexus!' as greeting", None)
        .await?;
    tracing::info!("Query result: {:?}", result);

    // Create a label
    tracing::info!("\nCreating label...");
    let label_response = client.create_label("Person".to_string()).await?;
    tracing::info!("Label creation: {}", label_response.message);

    // List all labels
    tracing::info!("\nListing labels...");
    let labels = client.list_labels().await?;
    tracing::info!("Labels: {:?}", labels.labels);

    // Create a node
    tracing::info!("\nCreating node...");
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), Value::String("Alice".to_string()));
    properties.insert("age".to_string(), Value::Int(30));
    properties.insert("active".to_string(), Value::Bool(true));

    let create_response = client
        .create_node(vec!["Person".to_string()], properties)
        .await?;
    tracing::info!("Created node with ID: {}", create_response.node_id);

    // Get the node
    tracing::info!("\nGetting node...");
    let get_response = client.get_node(create_response.node_id).await?;
    if let Some(node) = get_response.node {
        tracing::info!("Node ID: {}", node.id);
        tracing::info!("Labels: {:?}", node.labels);
        tracing::info!("Properties: {:?}", node.properties);
    }

    // Update the node
    tracing::info!("\nUpdating node...");
    let mut update_properties = HashMap::new();
    update_properties.insert("age".to_string(), Value::Int(31));
    let update_response = client
        .update_node(create_response.node_id, update_properties)
        .await?;
    tracing::info!("Update result: {}", update_response.message);

    // Create a relationship type
    tracing::info!("\nCreating relationship type...");
    let rel_type_response = client.create_rel_type("KNOWS".to_string()).await?;
    tracing::info!("Relationship type creation: {}", rel_type_response.message);

    // List relationship types
    tracing::info!("\nListing relationship types...");
    let types = client.list_rel_types().await?;
    tracing::info!("Relationship types: {:?}", types.types);

    tracing::info!("\nExample completed successfully!");
    Ok(())
}
