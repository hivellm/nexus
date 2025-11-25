//! Test Point literal in RETURN clause
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
fn test_return_point_literal_2d() {
    let (mut executor, _dir) = create_test_executor();

    let query = Query {
        cypher: "RETURN point({x: 1, y: 2, crs: 'cartesian'}) AS p".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "p");

    // Check that the first value is a Point object
    if let Some(Value::Object(obj)) = result.rows[0].values.first() {
        assert!(obj.contains_key("x"), "Point should have 'x' field");
        assert!(obj.contains_key("y"), "Point should have 'y' field");
        assert!(obj.contains_key("crs"), "Point should have 'crs' field");

        if let Some(Value::Number(x)) = obj.get("x") {
            assert_eq!(x.as_f64().unwrap(), 1.0);
        } else {
            panic!("x should be a number");
        }

        if let Some(Value::Number(y)) = obj.get("y") {
            assert_eq!(y.as_f64().unwrap(), 2.0);
        } else {
            panic!("y should be a number");
        }

        if let Some(Value::String(crs)) = obj.get("crs") {
            assert_eq!(crs, "cartesian");
        } else {
            panic!("crs should be a string");
        }
    } else {
        panic!(
            "Expected Point object, got: {:?}",
            result.rows[0].values.first()
        );
    }
}
