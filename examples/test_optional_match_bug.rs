//! Test to reproduce OPTIONAL MATCH bug

use nexus_core::testing::setup_isolated_test_engine;

fn main() {
    // Setup test environment
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create test graph (same as Section 11)
    println!("Creating test data...");
    engine.execute_cypher("CREATE (n1:Node {id: 1}), (n2:Node {id: 2}), (n3:Node {id: 3}), (n4:Node {id: 4}), (n5:Node {id: 5}), (n6:Node {id: 6})").unwrap();
    engine.execute_cypher("MATCH (n1:Node {id: 1}), (n2:Node {id: 2}) CREATE (n1)-[:CONNECTS]->(n2)").unwrap();
    engine.execute_cypher("MATCH (n2:Node {id: 2}), (n3:Node {id: 3}) CREATE (n2)-[:CONNECTS]->(n3)").unwrap();
    engine.execute_cypher("MATCH (n3:Node {id: 3}), (n1:Node {id: 1}) CREATE (n3)-[:CONNECTS]->(n1)").unwrap();
    engine.execute_cypher("MATCH (n4:Node {id: 4}), (n5:Node {id: 5}) CREATE (n4)-[:CONNECTS]->(n5)").unwrap();
    engine.execute_cypher("MATCH (n5:Node {id: 5}), (n6:Node {id: 6}) CREATE (n5)-[:CONNECTS]->(n6)").unwrap();

    println!("\nTest 1: Regular MATCH (should return 5 rows)");
    let result1 = engine.execute_cypher("MATCH (n:Node)-[:CONNECTS]->() RETURN n.id ORDER BY n.id").unwrap();
    println!("Result: {} rows", result1.rows.len());
    for row in &result1.rows {
        println!("  {:?}", row);
    }

    println!("\nTest 2: OPTIONAL MATCH with aggregation (should return 6 rows)");
    let result2 = engine.execute_cypher("MATCH (n:Node) OPTIONAL MATCH (n)-[:CONNECTS]->() RETURN n.id, count(*) AS out_degree ORDER BY n.id").unwrap();
    println!("Result: {} rows", result2.rows.len());
    for row in &result2.rows {
        println!("  {:?}", row);
    }

    println!("\nExpected: 6 rows (nodes 1-6)");
    println!("Actual: {} rows", result2.rows.len());

    if result2.rows.len() != 6 {
        println!("\n❌ BUG CONFIRMED: OPTIONAL MATCH is missing nodes without matches!");
        println!("   Node 6 and possibly node 3 are missing");
    } else {
        println!("\n✅ Bug fixed!");
    }
}
