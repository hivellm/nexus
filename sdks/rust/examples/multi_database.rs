//! Multi-database support example for Nexus Rust SDK

use nexus_sdk::{NexusClient, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client connecting to the default database
    let client = NexusClient::new("http://localhost:15474")?;

    println!("=== Multi-Database Support Demo ===\n");

    // 1. List all databases
    println!("1. Listing all databases...");
    let databases = client.list_databases().await?;
    let db_names: Vec<&str> = databases
        .databases
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    println!("   Available databases: {:?}", db_names);
    println!("   Default database: {}\n", databases.default_database);

    // 2. Create a new database
    println!("2. Creating new database 'testdb'...");
    let create_result = client.create_database("testdb").await?;
    println!("   Result: {}\n", create_result.message);

    // 3. Switch to the new database
    println!("3. Switching to 'testdb'...");
    let switch_result = client.switch_database("testdb").await?;
    println!("   Result: {}\n", switch_result.message);

    // 4. Get current database
    println!("4. Getting current database...");
    let current_db = client.get_current_database().await?;
    println!("   Current database: {}\n", current_db);

    // 5. Create data in the new database
    println!("5. Creating data in 'testdb'...");
    let mut params = HashMap::new();
    params.insert("name".to_string(), Value::String("Laptop".to_string()));
    params.insert("price".to_string(), Value::Float(999.99));
    let result = client
        .execute_cypher(
            "CREATE (n:Product {name: $name, price: $price}) RETURN n",
            Some(params),
        )
        .await?;
    println!("   Created {} node(s)\n", result.rows.len());

    // 6. Query data from testdb
    println!("6. Querying data from 'testdb'...");
    let query_result = client
        .execute_cypher(
            "MATCH (n:Product) RETURN n.name AS name, n.price AS price",
            None,
        )
        .await?;
    // Rows are arrays matching column order: [name, price]
    for row in &query_result.rows {
        if let Some(arr) = row.as_array() {
            let name = arr.first().and_then(|v| v.as_str()).unwrap_or("unknown");
            let price = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            println!("   Product: {}, Price: ${}\n", name, price);
        }
    }

    // 7. Switch back to default database
    println!("7. Switching back to default database...");
    let switch_back = client.switch_database("neo4j").await?;
    println!("   Result: {}\n", switch_back.message);

    // 8. Verify data isolation - the Product node should not exist in default db
    println!("8. Verifying data isolation...");
    let isolation_check = client
        .execute_cypher("MATCH (n:Product) RETURN count(n) AS count", None)
        .await?;
    // Row is an array: [count_value]
    let product_count = isolation_check
        .rows
        .first()
        .and_then(|row| row.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    println!("   Product nodes in default database: {}", product_count);
    println!("   Data isolation verified: {}\n", product_count == 0);

    // 9. Get database info
    println!("9. Getting 'testdb' info...");
    let db_info = client.get_database("testdb").await?;
    println!("   Name: {}", db_info.name);
    println!("   Path: {}", db_info.path);
    println!("   Nodes: {}", db_info.node_count);
    println!("   Relationships: {}", db_info.relationship_count);
    println!("   Storage: {} bytes\n", db_info.storage_size);

    // 10. Clean up - drop the test database
    println!("10. Dropping 'testdb'...");
    let drop_result = client.drop_database("testdb").await?;
    println!("    Result: {}\n", drop_result.message);

    // 11. Verify database was dropped
    println!("11. Verifying 'testdb' was dropped...");
    let final_databases = client.list_databases().await?;
    let db_exists = final_databases
        .databases
        .iter()
        .any(|db| db.name == "testdb");
    println!("    'testdb' exists: {}", db_exists);
    println!("    Cleanup successful: {}\n", !db_exists);

    println!("=== Multi-Database Demo Complete ===");

    Ok(())
}
