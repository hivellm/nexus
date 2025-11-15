//! Transaction example for Nexus Rust SDK

use nexus_sdk_rust::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let client = NexusClient::new("http://localhost:15474")?;

    println!("=== Transaction Example ===\n");

    // Begin a transaction
    println!("1. Beginning transaction...");
    let mut tx = client.begin_transaction().await?;
    println!("   Transaction started");

    // Create nodes within transaction
    println!("\n2. Creating nodes within transaction...");
    let mut properties1 = HashMap::new();
    properties1.insert("name".to_string(), Value::String("Alice".to_string()));
    let node1 = tx
        .execute("CREATE (n:Person {name: $name}) RETURN id(n) as id", {
            let mut params = HashMap::new();
            params.insert("name".to_string(), Value::String("Alice".to_string()));
            Some(params)
        })
        .await?;
    println!("   Created node: {:?}", node1);

    let mut properties2 = HashMap::new();
    properties2.insert("name".to_string(), Value::String("Bob".to_string()));
    let node2 = tx
        .execute("CREATE (n:Person {name: $name}) RETURN id(n) as id", {
            let mut params = HashMap::new();
            params.insert("name".to_string(), Value::String("Bob".to_string()));
            Some(params)
        })
        .await?;
    println!("   Created node: {:?}", node2);

    // Create relationship within transaction
    println!("\n3. Creating relationship within transaction...");
    let rel = tx
        .execute(
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[r:KNOWS {since: 2020}]->(b) RETURN id(r) as id",
            None,
        )
        .await?;
    println!("   Created relationship: {:?}", rel);

    // Commit transaction
    println!("\n4. Committing transaction...");
    tx.commit().await?;
    println!("   Transaction committed successfully");

    println!("\n=== Example completed ===");
    Ok(())
}
