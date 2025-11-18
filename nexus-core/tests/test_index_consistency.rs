//! Index Consistency Tests
//!
//! Tests to verify that indexes remain consistent after applying pending index updates
//! in batch during commit (Phase 1 optimization).

use nexus_core::Engine;
use tempfile::TempDir;

/// Helper function to extract count from result
fn extract_count(result: nexus_core::executor::ResultSet) -> u64 {
    result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Test that label indexes remain consistent after batch updates
#[test]
fn test_label_index_consistency_after_batch_updates() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Begin transaction
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();

    // Create nodes with labels
    engine
        .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30})")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Person {name: 'Bob', age: 25})")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Company {name: 'Acme Corp'})")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Person:Employee {name: 'Charlie', age: 35})")
        .unwrap();

    // Commit transaction (this applies pending index updates)
    engine.execute_cypher("COMMIT TRANSACTION").unwrap();

    // Query nodes by label to verify index consistency
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(count, 3, "Should have 3 Person nodes (Alice, Bob, Charlie)");

    let result = engine
        .execute_cypher("MATCH (n:Company) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(count, 1, "Should have 1 Company node");

    let result = engine
        .execute_cypher("MATCH (n:Employee) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(count, 1, "Should have 1 Employee node");
}

/// Test that relationship indexes remain consistent after batch updates
#[test]
fn test_relationship_index_consistency_after_batch_updates() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Begin transaction
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();

    // Create nodes and relationships
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (c:Company {name: 'Acme'})",
        )
        .unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'})-[:WORKS_FOR]->(c:Company {name: 'Acme'})",
        )
        .unwrap();
    engine
        .execute_cypher("CREATE (b:Person {name: 'Bob'})-[:KNOWS]->(a:Person {name: 'Alice'})")
        .unwrap();

    // Commit transaction (this applies pending index updates)
    engine.execute_cypher("COMMIT TRANSACTION").unwrap();

    // Verify relationship index consistency
    let result = engine
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN count(r) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(count, 2, "Should have 2 KNOWS relationships");

    let result = engine
        .execute_cypher("MATCH ()-[r:WORKS_FOR]->() RETURN count(r) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(count, 1, "Should have 1 WORKS_FOR relationship");
}

/// Test that property indexes remain consistent after batch updates
#[test]
fn test_property_index_consistency_after_batch_updates() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Begin transaction
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();

    // Create nodes with indexed properties
    engine
        .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30, email: 'alice@example.com'})")
        .unwrap();
    engine
        .execute_cypher("CREATE (n:Person {name: 'Bob', age: 25, email: 'bob@example.com'})")
        .unwrap();
    engine
        .execute_cypher(
            "CREATE (n:Person {name: 'Charlie', age: 35, email: 'charlie@example.com'})",
        )
        .unwrap();

    // Commit transaction (this applies pending index updates)
    engine.execute_cypher("COMMIT TRANSACTION").unwrap();

    // Verify property index consistency by querying with WHERE clause
    let result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.age > 28 RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(
        count, 2,
        "Should have 2 Person nodes with age > 28 (Alice=30, Charlie=35)"
    );

    let result = engine
        .execute_cypher(
            "MATCH (n:Person) WHERE n.email = 'alice@example.com' RETURN n.name as name",
        )
        .unwrap();
    assert_eq!(
        result.rows.len(),
        1,
        "Should find 1 node with email 'alice@example.com'"
    );
    let name = result.rows[0].values[0].as_str().unwrap();
    assert_eq!(name, "Alice");
}

/// Test that indexes remain consistent after rollback (updates should not be applied)
#[test]
fn test_index_consistency_after_rollback() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create initial node with unique identifier
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();
    engine
        .execute_cypher("CREATE (n:Person {name: 'AliceRollbackTest', id: 9999})")
        .unwrap();
    engine.execute_cypher("COMMIT TRANSACTION").unwrap();

    // Start new transaction and create node, then rollback
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();
    engine
        .execute_cypher("CREATE (n:Person {name: 'BobRollbackTest', id: 9998})")
        .unwrap();
    engine.execute_cypher("ROLLBACK TRANSACTION").unwrap();

    // Verify that only Alice exists (Bob should not be in index)
    // Check for Alice specifically
    let alice_result = engine
        .execute_cypher("MATCH (n:Person {name: 'AliceRollbackTest'}) RETURN count(n) as count")
        .unwrap();
    let alice_count = extract_count(alice_result);
    assert_eq!(alice_count, 1, "Alice should exist");

    // Verify Bob doesn't exist
    let bob_result = engine
        .execute_cypher("MATCH (n:Person {name: 'BobRollbackTest'}) RETURN count(n) as count")
        .unwrap();
    let bob_count = extract_count(bob_result);
    assert_eq!(bob_count, 0, "Bob should not exist after rollback");

    // Verify total count of nodes with our test IDs
    let total_result = engine
        .execute_cypher(
            "MATCH (n:Person) WHERE n.id = 9999 OR n.id = 9998 RETURN count(n) as count",
        )
        .unwrap();
    let total_count = extract_count(total_result);
    assert_eq!(
        total_count, 1,
        "Should have only 1 Person node with test IDs after rollback"
    );
}

/// Test that indexes remain consistent with concurrent transactions
#[tokio::test]
async fn test_index_consistency_with_concurrent_transactions() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::with_data_dir(dir.path()).unwrap();
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));

    // Create multiple transactions concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            engine.execute_cypher("BEGIN TRANSACTION").unwrap();

            let query = format!("CREATE (n:Person {{name: 'Person{}', age: {}}})", i, 20 + i);
            engine.execute_cypher(&query).unwrap();

            engine.execute_cypher("COMMIT TRANSACTION").unwrap();
        });
        handles.push(handle);
    }

    // Wait for all transactions to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all nodes are in the index
    let mut engine = engine.lock().unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(
        count, 10,
        "Should have 10 Person nodes from concurrent transactions"
    );
}
