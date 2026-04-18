//! Transaction example for Nexus Rust SDK

use nexus_sdk::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let client = NexusClient::new("http://localhost:15474")?;

    tracing::info!("=== Transaction Example ===\n");

    // Begin a transaction
    tracing::info!("1. Beginning transaction...");
    let mut tx = client.begin_transaction().await?;
    tracing::info!("   Transaction started");

    // Create nodes within transaction
    tracing::info!("\n2. Creating nodes within transaction...");
    let mut properties1 = HashMap::new();
    properties1.insert("name".to_string(), Value::String("Alice".to_string()));
    let node1 = tx
        .execute("CREATE (n:Person {name: $name}) RETURN id(n) as id", {
            let mut params = HashMap::new();
            params.insert("name".to_string(), Value::String("Alice".to_string()));
            Some(params)
        })
        .await?;
    tracing::info!("   Created node: {:?}", node1);

    let mut properties2 = HashMap::new();
    properties2.insert("name".to_string(), Value::String("Bob".to_string()));
    let node2 = tx
        .execute("CREATE (n:Person {name: $name}) RETURN id(n) as id", {
            let mut params = HashMap::new();
            params.insert("name".to_string(), Value::String("Bob".to_string()));
            Some(params)
        })
        .await?;
    tracing::info!("   Created node: {:?}", node2);

    // Create relationship within transaction
    tracing::info!("\n3. Creating relationship within transaction...");
    let rel = tx
        .execute(
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[r:KNOWS {since: 2020}]->(b) RETURN id(r) as id",
            None,
        )
        .await?;
    tracing::info!("   Created relationship: {:?}", rel);

    // Commit transaction
    tracing::info!("\n4. Committing transaction...");
    tx.commit().await?;
    tracing::info!("   Transaction committed successfully");

    tracing::info!("\n=== Example completed ===");
    Ok(())
}
