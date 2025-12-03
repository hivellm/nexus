use nexus_sdk::{NexusClient, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Rust SDK ===\n");

    let client = NexusClient::with_api_key("http://localhost:15474", "test-key")?;

    // Test 1: Simple query
    print!("1. Simple query: ");
    let result = client.execute_cypher("RETURN 1 as num", None).await?;
    println!("OK - Columns: {}", result.columns.join(", "));

    // Test 2: Create nodes
    print!("2. Create nodes: ");
    let result = client
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice', age: 28}) \
             CREATE (b:Person {name: 'Bob', age: 32}) \
             RETURN a.name, b.name",
            None,
        )
        .await?;
    println!("OK - Rows: {}", result.rows.len());

    // Test 3: Query with parameters
    print!("3. Query with parameters: ");
    let mut params = HashMap::new();
    params.insert("minAge".to_string(), Value::Int(25));
    let result = client
        .execute_cypher(
            "MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age",
            Some(params),
        )
        .await?;
    println!("OK - Found {} nodes", result.rows.len());

    // Test 4: Create relationship
    print!("4. Create relationship: ");
    client
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'}) \
             MATCH (b:Person {name: 'Bob'}) \
             CREATE (a)-[r:KNOWS {since: '2020'}]->(b) \
             RETURN type(r) as type",
            None,
        )
        .await?;
    println!("OK");

    // Test 5: Query relationships
    print!("5. Query relationships: ");
    let result = client
        .execute_cypher(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person) \
             RETURN a.name as person1, b.name as person2",
            None,
        )
        .await?;
    println!("OK - Found {} relationships", result.rows.len());

    // Test 6: Cleanup
    print!("6. Cleanup: ");
    client
        .execute_cypher("MATCH (n) DETACH DELETE n", None)
        .await?;
    println!("OK");

    println!("\n[SUCCESS] All Rust SDK tests passed!");

    Ok(())
}
