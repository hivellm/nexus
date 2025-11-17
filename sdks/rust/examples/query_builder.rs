//! Query builder example for Nexus Rust SDK

use nexus_sdk::*;

#[tokio::main]
async fn main() -> Result<()> {
    let client = NexusClient::new("http://localhost:15474")?;

    println!("=== Query Builder Example ===\n");

    // Example 1: Simple MATCH query
    println!("1. Building MATCH query...");
    let query1 = QueryBuilder::new()
        .match_("(n:Person)")
        .where_("n.age > $min_age")
        .return_("n.name, n.age")
        .order_by("n.age DESC")
        .limit(10)
        .param("min_age", 18)
        .build();

    println!("   Query: {}", query1.query());
    let result1 = client
        .execute_cypher(query1.query(), query1.params().cloned())
        .await?;
    println!("   Results: {} rows", result1.rows.len());

    // Example 2: CREATE query with parameters
    println!("\n2. Building CREATE query...");
    let query2 = QueryBuilder::new()
        .create("(n:Person {name: $name, age: $age})")
        .return_("n")
        .param("name", "Charlie")
        .param("age", 25)
        .build();

    println!("   Query: {}", query2.query());
    let result2 = client
        .execute_cypher(query2.query(), query2.params().cloned())
        .await?;
    println!("   Created: {} rows", result2.rows.len());

    // Example 3: Complex query with multiple clauses
    println!("\n3. Building complex query...");
    let query3 = QueryBuilder::new()
        .match_("(a:Person)-[r:KNOWS]->(b:Person)")
        .where_("a.age > $min_age AND b.age > $min_age")
        .return_("a.name, b.name, r.since")
        .order_by("r.since ASC")
        .limit(5)
        .param("min_age", 21)
        .build();

    println!("   Query: {}", query3.query());
    let result3 = client
        .execute_cypher(query3.query(), query3.params().cloned())
        .await?;
    println!("   Results: {} rows", result3.rows.len());

    println!("\n=== Example completed ===");
    Ok(())
}
