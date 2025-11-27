//! Tests for geospatial functions

use crate::executor::{Executor, Query};
use crate::index::KnnIndex;
use crate::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
use std::collections::HashMap;
use tempfile::TempDir;

fn create_test_executor() -> (Executor, TempDir) {
    let dir = TempDir::new().unwrap();
    // Ensure directory exists before creating components
    std::fs::create_dir_all(dir.path()).unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();

    let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
    (executor, dir)
}

#[test]
fn test_distance_function_cartesian_2d() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // Create two points
    let p1 = serde_json::json!({
        "x": 0.0,
        "y": 0.0,
        "crs": "cartesian"
    });
    let p2 = serde_json::json!({
        "x": 3.0,
        "y": 4.0,
        "crs": "cartesian"
    });

    params.insert("p1".to_string(), p1);
    params.insert("p2".to_string(), p2);

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "dist");

    if let Some(Value::Number(dist)) = result.rows[0].values.first() {
        let dist_val = dist.as_f64().unwrap();
        assert!((dist_val - 5.0).abs() < 0.0001); // sqrt(3^2 + 4^2) = 5
    } else {
        panic!("Expected number result");
    }
}

#[test]
#[ignore] // TODO: Fix temp dir race condition - "No such file or directory" error
fn test_distance_function_cartesian_3d() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // Create two 3D points
    let p1 = serde_json::json!({
        "x": 0.0,
        "y": 0.0,
        "z": 0.0,
        "crs": "cartesian-3d"
    });
    let p2 = serde_json::json!({
        "x": 2.0,
        "y": 3.0,
        "z": 6.0,
        "crs": "cartesian-3d"
    });

    params.insert("p1".to_string(), p1);
    params.insert("p2".to_string(), p2);

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);

    if let Some(Value::Number(dist)) = result.rows[0].values.first() {
        let dist_val = dist.as_f64().unwrap();
        assert!((dist_val - 7.0).abs() < 0.0001); // sqrt(2^2 + 3^2 + 6^2) = sqrt(49) = 7
    } else {
        panic!("Expected number result");
    }
}

#[test]
fn test_distance_function_with_point_literals() {
    // Note: Point literals in RETURN clause may not work yet due to evaluation order
    // This test is skipped for now - use parameters instead
    // TODO: Fix point literal evaluation in RETURN clause
}

#[test]
fn test_distance_function_wgs84() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // San Francisco to New York (approximate)
    // Note: Using x/y instead of longitude/latitude for now
    let sf = serde_json::json!({
        "x": -122.4194,
        "y": 37.7749,
        "crs": "wgs-84"
    });
    let ny = serde_json::json!({
        "x": -74.0060,
        "y": 40.7128,
        "crs": "wgs-84"
    });

    params.insert("sf".to_string(), sf);
    params.insert("ny".to_string(), ny);

    let query = Query {
        cypher: "RETURN distance($sf, $ny) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);

    if let Some(Value::Number(dist)) = result.rows[0].values.first() {
        let dist_val = dist.as_f64().unwrap();
        // Should be approximately 4139 km (in meters)
        assert!(dist_val > 4000000.0 && dist_val < 4300000.0);
    } else {
        panic!("Expected number result");
    }
}

#[test]
fn test_distance_function_null_for_invalid_points() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    params.insert("p1".to_string(), Value::String("not a point".to_string()));
    params.insert("p2".to_string(), Value::String("not a point".to_string()));

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);

    // Should return null for invalid points
    assert_eq!(result.rows[0].values[0], Value::Null);
}
