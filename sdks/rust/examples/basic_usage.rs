//! Basic usage example for Nexus Rust SDK

use nexus_sdk_rust::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Create a client
    let client = NexusClient::new("http://localhost:15474")?;

    // Check server health
    println!("Checking server health...");
    let healthy = client.health_check().await?;
    println!("Server is healthy: {}", healthy);

    // Get database statistics
    println!("\nGetting database statistics...");
    let stats = client.get_stats().await?;
    println!("Nodes: {}", stats.catalog.node_count);
    println!("Relationships: {}", stats.catalog.rel_count);
    println!("Labels: {}", stats.catalog.label_count);
    println!("Relationship types: {}", stats.catalog.rel_type_count);

    // Execute a simple Cypher query
    println!("\nExecuting Cypher query...");
    let result = client
        .execute_cypher("RETURN 'Hello, Nexus!' as greeting", None)
        .await?;
    println!("Query result: {:?}", result);

    // Create a label
    println!("\nCreating label...");
    let label_response = client.create_label("Person".to_string()).await?;
    println!("Label creation: {}", label_response.message);

    // List all labels
    println!("\nListing labels...");
    let labels = client.list_labels().await?;
    println!("Labels: {:?}", labels.labels);

    // Create a node
    println!("\nCreating node...");
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), Value::String("Alice".to_string()));
    properties.insert("age".to_string(), Value::Int(30));
    properties.insert("active".to_string(), Value::Bool(true));

    let create_response = client
        .create_node(vec!["Person".to_string()], properties)
        .await?;
    println!("Created node with ID: {}", create_response.node_id);

    // Get the node
    println!("\nGetting node...");
    let get_response = client.get_node(create_response.node_id).await?;
    if let Some(node) = get_response.node {
        println!("Node ID: {}", node.id);
        println!("Labels: {:?}", node.labels);
        println!("Properties: {:?}", node.properties);
    }

    // Update the node
    println!("\nUpdating node...");
    let mut update_properties = HashMap::new();
    update_properties.insert("age".to_string(), Value::Int(31));
    let update_response = client
        .update_node(create_response.node_id, update_properties)
        .await?;
    println!("Update result: {}", update_response.message);

    // Create a relationship type
    println!("\nCreating relationship type...");
    let rel_type_response = client.create_rel_type("KNOWS".to_string()).await?;
    println!("Relationship type creation: {}", rel_type_response.message);

    // List relationship types
    println!("\nListing relationship types...");
    let types = client.list_rel_types().await?;
    println!("Relationship types: {:?}", types.types);

    println!("\nExample completed successfully!");
    Ok(())
}
