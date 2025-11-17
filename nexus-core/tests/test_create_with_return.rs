//! Test CREATE with RETURN clause
//! Critical bug: CREATE executes successfully but returns 0 rows instead of 1

use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
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
fn test_create_single_node_with_property_return() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with RETURN - should return 1 row with name='Alice'
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Alice', age: 30}) RETURN n.name AS name".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "name");

    // Verify the returned value
    let row = &result.rows[0];
    if let Some(Value::String(name)) = row.values.first() {
        assert_eq!(name, "Alice");
    } else {
        panic!("Expected string 'Alice', got: {:?}", row.values.first());
    }
}

#[test]
fn test_create_and_return_literal() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with RETURN literal - should return 1 row with status='created'
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Bob'}) RETURN 'created' AS status".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "status");

    // Verify the returned value
    let row = &result.rows[0];
    if let Some(Value::String(status)) = row.values.first() {
        assert_eq!(status, "created");
    } else {
        panic!("Expected string 'created', got: {:?}", row.values.first());
    }
}

#[test]
fn test_create_multiple_properties_return() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with RETURN multiple properties
    let query = Query {
        cypher:
            "CREATE (n:Person {name: 'Charlie', age: 35, city: 'NYC'}) RETURN n.name, n.age, n.city"
                .to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 3);

    // Verify the returned values
    let row = &result.rows[0];
    assert_eq!(row.values[0], Value::String("Charlie".to_string()));
    assert_eq!(row.values[1], Value::Number(serde_json::Number::from(35)));
    assert_eq!(row.values[2], Value::String("NYC".to_string()));
}

#[test]
fn test_create_return_node_object() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with RETURN node object
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Eve'}) RETURN n".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "n");

    // Verify the returned value is an object
    let row = &result.rows[0];
    match row.values.first() {
        Some(Value::Object(_)) => {
            // Success
        }
        other => panic!("Expected node object, got: {:?}", other),
    }
}

#[test]
fn test_create_return_id_function() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with RETURN id() function
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Frank'}) RETURN id(n) AS node_id".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "node_id");

    // Verify the returned value is a number (ID)
    let row = &result.rows[0];
    match row.values.first() {
        Some(Value::Number(_)) => {
            // Success
        }
        other => panic!("Expected numeric node ID, got: {:?}", other),
    }
}

#[test]
fn test_create_multiple_nodes_with_return() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE multiple nodes with RETURN
    let query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) RETURN a.name, b.name"
            .to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 2);

    // Verify the returned values
    let row = &result.rows[0];
    assert_eq!(row.values[0], Value::String("Alice".to_string()));
    assert_eq!(row.values[1], Value::String("Bob".to_string()));
}

#[test]
fn test_create_arithmetic_expression_in_return() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with arithmetic expression in RETURN
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Grace', age: 28}) RETURN n.age * 2 AS double_age"
            .to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(
        result.rows.len(),
        1,
        "Expected 1 row, got {}",
        result.rows.len()
    );
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "double_age");

    // Verify the returned value
    let row = &result.rows[0];
    match &row.values[0] {
        Value::Number(n) => {
            assert_eq!(n.as_f64().unwrap(), 56.0, "Expected 56.0, got {:?}", n);
        }
        other => panic!("Expected number, got: {:?}", other),
    }
}
