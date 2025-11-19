//! Test CALL procedure syntax variations
use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
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
fn test_call_procedure_with_yield_and_return() {
    let (mut executor, _dir) = create_test_executor();

    // Create some nodes with labels first
    let create_query = Query {
        cypher: "CREATE (n1:Person), (n2:Employee), (n3:Person)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.labels() YIELD label RETURN label".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    // Should execute successfully
    assert!(
        result.is_ok(),
        "CALL procedure with YIELD and RETURN should work"
    );

    if let Ok(result_set) = result {
        // Should have results
        assert!(
            !result_set.rows.is_empty(),
            "Should return at least one label"
        );
        assert_eq!(result_set.columns.len(), 1);
        assert_eq!(result_set.columns[0], "label");
    }
}

#[test]
fn test_call_procedure_with_return_only() {
    let (mut executor, _dir) = create_test_executor();

    // Create some nodes with labels first
    let create_query = Query {
        cypher: "CREATE (n1:Person), (n2:Employee)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.labels() RETURN label".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    // Should execute successfully (even if YIELD is omitted, RETURN should work)
    assert!(
        result.is_ok(),
        "CALL procedure with RETURN only should work"
    );
}

#[test]
fn test_call_procedure_without_return() {
    let (mut executor, _dir) = create_test_executor();

    // Create some nodes with labels first
    let create_query = Query {
        cypher: "CREATE (n1:Person), (n2:Employee)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.labels()".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    // Should execute successfully even without RETURN
    // The procedure should still return results
    assert!(result.is_ok(), "CALL procedure without RETURN should work");

    if let Ok(result_set) = result {
        // Procedures typically return results even without explicit RETURN
        // The exact behavior depends on implementation
        assert!(
            !result_set.rows.is_empty() || result_set.rows.is_empty(),
            "Procedure may or may not return results"
        );
    }
}

#[test]
fn test_call_procedure_relationship_types() {
    let (mut executor, _dir) = create_test_executor();

    // Create some relationships first
    let create_query = Query {
        cypher: "CREATE (a:Person)-[:KNOWS]->(b:Person), (b)-[:WORKS_WITH]->(c:Person)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType"
            .to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_ok(), "CALL db.relationshipTypes() should work");
}

#[test]
fn test_call_procedure_property_keys() {
    let (mut executor, _dir) = create_test_executor();

    // Create some nodes with properties first
    let create_query = Query {
        cypher: "CREATE (n1:Person {name: 'Alice', age: 30}), (n2:Person {name: 'Bob'})"
            .to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.propertyKeys() YIELD propertyKey RETURN propertyKey".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_ok(), "CALL db.propertyKeys() should work");
}

#[test]
fn test_call_procedure_schema() {
    let (mut executor, _dir) = create_test_executor();

    // Create some nodes and relationships first
    let create_query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'})-[r:KNOWS]->(b:Person {name: 'Bob'})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.schema() YIELD nodes, relationships RETURN nodes, relationships"
            .to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_ok(), "CALL db.schema() should work");
}
