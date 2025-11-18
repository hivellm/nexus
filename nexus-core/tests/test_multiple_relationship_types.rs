use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
use std::collections::HashMap;
use tempfile::TempDir;

fn create_test_executor() -> (Executor, TempDir) {
    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();

    let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
    (executor, dir)
}

#[test]
fn test_multiple_relationship_types_single() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create test data: Alice -[:KNOWS]-> Bob, Alice -[:WORKS_AT]-> Company
    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (c:Company {name: 'TechCorp'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT]->(c)".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test single type (should match 1 relationship)
    let query = Query {
        cypher: "MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS cnt".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(1))
    );
}

#[test]
fn test_multiple_relationship_types_or() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create test data
    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (c:Company {name: 'TechCorp'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT]->(c)".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test multiple types with | (should match 2 relationships)
    let query = Query {
        cypher: "MATCH (a)-[r:KNOWS|WORKS_AT]->(b) RETURN count(r) AS cnt".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(2))
    );
}

#[test]
fn test_multiple_relationship_types_with_return() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create test data
    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (c:Company {name: 'TechCorp'}), (d:Person {name: 'Charlie'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'TechCorp'}) CREATE (a)-[:WORKS_AT]->(c)".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'}), (d:Person {name: 'Charlie'}) CREATE (a)-[:LIKES]->(d)".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test multiple types and return target names
    let query = Query {
        cypher: "MATCH (a:Person {name: 'Alice'})-[r:KNOWS|WORKS_AT]->(b) RETURN b.name AS name ORDER BY name".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 2);
    // Should return "Bob" and "TechCorp" in alphabetical order
    assert_eq!(result.rows[0].values[0], Value::String("Bob".to_string()));
    assert_eq!(
        result.rows[1].values[0],
        Value::String("TechCorp".to_string())
    );
}

#[test]
fn test_multiple_relationship_types_nonexistent() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create test data with only KNOWS relationship
    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher:
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)"
                .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test multiple types where one doesn't exist (should match 1 relationship)
    let query = Query {
        cypher: "MATCH (a)-[r:KNOWS|NONEXISTENT]->(b) RETURN count(r) AS cnt".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(1))
    );
}
