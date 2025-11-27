use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use tempfile::TempDir;

/// Counter for unique test labels to prevent cross-test interference
static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonSingle{}", test_id);
    let company_label = format!("CompanySingle{}", test_id);
    let knows_type = format!("KNOWS_S{}", test_id);
    let works_type = format!("WORKS_AT_S{}", test_id);
    let (mut executor, _dir) = create_test_executor();

    // Create test data with unique labels
    let query = Query {
        cypher: format!(
            "CREATE (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}), (c:{} {{name: 'TechCorp'}})",
            person_label, person_label, company_label
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
            person_label, person_label, knows_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'TechCorp'}}) CREATE (a)-[:{}]->(c)",
            person_label, company_label, works_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test single type (should match 1 relationship)
    let query = Query {
        cypher: format!("MATCH (a)-[r:{}]->(b) RETURN count(r) AS cnt", knows_type),
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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonOr{}", test_id);
    let company_label = format!("CompanyOr{}", test_id);
    let knows_type = format!("KNOWS_O{}", test_id);
    let works_type = format!("WORKS_AT_O{}", test_id);
    let (mut executor, _dir) = create_test_executor();

    // Create test data with unique labels
    let query = Query {
        cypher: format!(
            "CREATE (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}), (c:{} {{name: 'TechCorp'}})",
            person_label, person_label, company_label
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
            person_label, person_label, knows_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'TechCorp'}}) CREATE (a)-[:{}]->(c)",
            person_label, company_label, works_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test multiple types with | (should match 2 relationships)
    let query = Query {
        cypher: format!(
            "MATCH (a)-[r:{}|{}]->(b) RETURN count(r) AS cnt",
            knows_type, works_type
        ),
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
#[ignore] // TODO: Fix multiple relationship types with RETURN clause
fn test_multiple_relationship_types_with_return() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonRet{}", test_id);
    let company_label = format!("CompanyRet{}", test_id);
    let knows_type = format!("KNOWS_R{}", test_id);
    let works_type = format!("WORKS_AT_R{}", test_id);
    let likes_type = format!("LIKES_R{}", test_id);
    let (mut executor, _dir) = create_test_executor();

    // Create test data with unique labels
    let query = Query {
        cypher: format!(
            "CREATE (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}), (c:{} {{name: 'TechCorp'}}), (d:{} {{name: 'Charlie'}})",
            person_label, person_label, company_label, person_label
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
            person_label, person_label, knows_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'TechCorp'}}) CREATE (a)-[:{}]->(c)",
            person_label, company_label, works_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (d:{} {{name: 'Charlie'}}) CREATE (a)-[:{}]->(d)",
            person_label, person_label, likes_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test multiple types and return target names
    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}})-[r:{}|{}]->(b) RETURN b.name AS name ORDER BY name",
            person_label, knows_type, works_type
        ),
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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonNE{}", test_id);
    let knows_type = format!("KNOWS_NE{}", test_id);
    let nonexistent_type = format!("NONEXISTENT{}", test_id);
    let (mut executor, _dir) = create_test_executor();

    // Create test data with only KNOWS relationship
    let query = Query {
        cypher: format!(
            "CREATE (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}})",
            person_label, person_label
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    let query = Query {
        cypher: format!(
            "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
            person_label, person_label, knows_type
        ),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test multiple types where one doesn't exist (should match 1 relationship)
    let query = Query {
        cypher: format!(
            "MATCH (a)-[r:{}|{}]->(b) RETURN count(r) AS cnt",
            knows_type, nonexistent_type
        ),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(1))
    );
}
