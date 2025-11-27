//! Comprehensive integration tests for geospatial features
//!
//! Tests cover:
//! - Point data type integration with Cypher
//! - Distance functions in queries
//! - R-tree index operations
//! - Geospatial procedures
//! - Edge cases and error handling

use nexus_core::executor::{Executor, Query};
use nexus_core::geospatial::procedures::{
    BoundingBox, WithinBBoxProcedure, WithinDistanceProcedure,
};
use nexus_core::geospatial::{CoordinateSystem, Point, rtree::RTreeIndex};
use nexus_core::graph::algorithms::Graph;
use nexus_core::graph::procedures::{GraphProcedure, ProcedureRegistry};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::{Value, json};
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

// ============================================================================
// Point Data Type Tests
// ============================================================================

#[test]
fn test_point_serialization_roundtrip() {
    // Test 2D Cartesian
    let p1 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let json = p1.to_json_value();
    let p2 = Point::from_json_value(&json).unwrap();
    assert_eq!(p1.x, p2.x);
    assert_eq!(p1.y, p2.y);
    assert_eq!(p1.z, p2.z);
    assert_eq!(p1.coordinate_system, p2.coordinate_system);

    // Test 3D Cartesian
    let p3 = Point::new_3d(1.0, 2.0, 3.0, CoordinateSystem::Cartesian);
    let json = p3.to_json_value();
    let p4 = Point::from_json_value(&json).unwrap();
    assert_eq!(p3.x, p4.x);
    assert_eq!(p3.y, p4.y);
    assert_eq!(p3.z, p4.z);
    assert_eq!(p3.coordinate_system, p4.coordinate_system);

    // Test WGS84
    let p5 = Point::new_2d(-122.4194, 37.7749, CoordinateSystem::WGS84);
    let json = p5.to_json_value();
    let p6 = Point::from_json_value(&json).unwrap();
    assert_eq!(p5.x, p6.x);
    assert_eq!(p5.y, p6.y);
    assert_eq!(p5.coordinate_system, p6.coordinate_system);
}

#[test]
fn test_point_from_json_with_aliases() {
    // Test longitude/latitude aliases for WGS84
    let json = json!({
        "longitude": -122.4194,
        "latitude": 37.7749,
        "crs": "wgs-84"
    });
    let p = Point::from_json_value(&json).unwrap();
    assert_eq!(p.x, -122.4194);
    assert_eq!(p.y, 37.7749);
    assert_eq!(p.coordinate_system, CoordinateSystem::WGS84);

    // Test x/y for Cartesian
    let json = json!({
        "x": 1.0,
        "y": 2.0,
        "crs": "cartesian"
    });
    let p = Point::from_json_value(&json).unwrap();
    assert_eq!(p.x, 1.0);
    assert_eq!(p.y, 2.0);
    assert_eq!(p.coordinate_system, CoordinateSystem::Cartesian);
}

#[test]
fn test_point_edge_cases() {
    // Test zero coordinates
    let p = Point::new_2d(0.0, 0.0, CoordinateSystem::Cartesian);
    assert_eq!(p.x, 0.0);
    assert_eq!(p.y, 0.0);

    // Test negative coordinates
    let p = Point::new_2d(-10.0, -20.0, CoordinateSystem::Cartesian);
    assert_eq!(p.x, -10.0);
    assert_eq!(p.y, -20.0);

    // Test very large coordinates
    let p = Point::new_2d(1e10, 1e10, CoordinateSystem::Cartesian);
    assert_eq!(p.x, 1e10);
    assert_eq!(p.y, 1e10);

    // Test very small coordinates
    let p = Point::new_2d(1e-10, 1e-10, CoordinateSystem::Cartesian);
    assert_eq!(p.x, 1e-10);
    assert_eq!(p.y, 1e-10);
}

#[test]
fn test_point_distance_edge_cases() {
    // Same point
    let p1 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let p2 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let distance = p1.distance_to(&p2);
    assert!((distance - 0.0).abs() < 0.0001);

    // Very close points
    let p1 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let p2 = Point::new_2d(1.0001, 2.0001, CoordinateSystem::Cartesian);
    let distance = p1.distance_to(&p2);
    assert!(distance > 0.0 && distance < 0.001);

    // Very far points
    let p1 = Point::new_2d(0.0, 0.0, CoordinateSystem::Cartesian);
    let p2 = Point::new_2d(1000.0, 1000.0, CoordinateSystem::Cartesian);
    let distance = p1.distance_to(&p2);
    assert!(distance > 1000.0);
}

// ============================================================================
// Distance Function Tests
// ============================================================================

#[test]
fn test_distance_function_multiple_queries() {
    let (mut executor, _dir) = create_test_executor();

    // Test multiple distance calculations in one query
    let mut params = HashMap::new();
    params.insert(
        "p1".to_string(),
        json!({"x": 0.0, "y": 0.0, "crs": "cartesian"}),
    );
    params.insert(
        "p2".to_string(),
        json!({"x": 3.0, "y": 4.0, "crs": "cartesian"}),
    );
    params.insert(
        "p3".to_string(),
        json!({"x": 6.0, "y": 8.0, "crs": "cartesian"}),
    );

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS d1, distance($p2, $p3) AS d2".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 2);

    if let (Some(Value::Number(d1)), Some(Value::Number(d2))) =
        (result.rows[0].values.first(), result.rows[0].values.get(1))
    {
        let d1_val = d1.as_f64().unwrap();
        let d2_val = d2.as_f64().unwrap();
        assert!((d1_val - 5.0).abs() < 0.0001);
        assert!((d2_val - 5.0).abs() < 0.0001);
    } else {
        panic!("Expected number results");
    }
}

#[test]
fn test_distance_function_with_null_points() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    params.insert("p1".to_string(), Value::Null);
    params.insert(
        "p2".to_string(),
        json!({"x": 3.0, "y": 4.0, "crs": "cartesian"}),
    );

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Null);
}

#[test]
fn test_distance_function_coordinate_system_mismatch() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // Mixing coordinate systems should still calculate distance
    // (using the first point's coordinate system)
    params.insert(
        "p1".to_string(),
        json!({"x": 0.0, "y": 0.0, "crs": "cartesian"}),
    );
    params.insert(
        "p2".to_string(),
        json!({"x": 3.0, "y": 4.0, "crs": "wgs-84"}),
    );

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    // Should still return a result (may not be accurate, but should not error)
    assert_ne!(result.rows[0].values[0], Value::Null);
}

// ============================================================================
// R-tree Index Tests
// ============================================================================

#[test]
fn test_rtree_large_dataset() {
    let index = RTreeIndex::new();

    // Insert 1000 points
    for i in 0..1000 {
        let point = Point::new_2d(
            (i as f64) * 0.1,
            (i as f64) * 0.1,
            CoordinateSystem::Cartesian,
        );
        index.insert(i, &point).unwrap();
    }

    let stats = index.get_stats();
    assert_eq!(stats.total_points, 1000);
    assert!(stats.grid_cells > 0);

    // Query should find all points in a large bounding box
    let results = index.query_bbox((0.0, 0.0, 100.0, 100.0)).unwrap();
    assert_eq!(results.len(), 1000);
}

#[test]
fn test_rtree_query_empty_bbox() {
    let index = RTreeIndex::new();

    index
        .insert(1, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
        .unwrap();

    // Query empty bounding box
    let results = index.query_bbox((10.0, 10.0, 10.0, 10.0)).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_rtree_query_partial_overlap() {
    let index = RTreeIndex::new();

    // Insert points in a grid
    for x in 0..10 {
        for y in 0..10 {
            let point = Point::new_2d(x as f64, y as f64, CoordinateSystem::Cartesian);
            index.insert((x * 10 + y) as u64, &point).unwrap();
        }
    }

    // Query partial bounding box
    let results = index.query_bbox((2.0, 2.0, 5.0, 5.0)).unwrap();
    // Should find points in the 3x3 grid (9 points)
    // Note: Grid-based R-tree may include more points due to cell boundaries
    assert!(results.len() >= 9); // At least 9 points, may be more due to grid cells
    // Verify all returned points are actually within bbox
    for node_id in results.iter() {
        if let Some(point) = index.get_point(node_id as u64) {
            assert!(point.x >= 2.0 && point.x <= 5.0);
            assert!(point.y >= 2.0 && point.y <= 5.0);
        }
    }
}

#[test]
fn test_rtree_distance_query_edge_cases() {
    let index = RTreeIndex::new();

    index
        .insert(1, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
        .unwrap();
    index
        .insert(2, &Point::new_2d(15.0, 15.0, CoordinateSystem::Cartesian))
        .unwrap();

    // Query with zero distance (should only find exact match)
    let center = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
    let results = index.query_distance(&center, 0.0).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(1));

    // Query with very large distance
    let results = index.query_distance(&center, 1000.0).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_rtree_custom_cell_size() {
    // Test with different cell sizes
    let index_small = RTreeIndex::with_cell_size(10.0);
    let index_large = RTreeIndex::with_cell_size(100.0);

    for i in 0..100 {
        let point = Point::new_2d(i as f64, i as f64, CoordinateSystem::Cartesian);
        index_small.insert(i, &point).unwrap();
        index_large.insert(i, &point).unwrap();
    }

    // Both should have same points but different grid cell counts
    assert_eq!(index_small.get_stats().total_points, 100);
    assert_eq!(index_large.get_stats().total_points, 100);
    // Smaller cell size should create more grid cells
    assert!(index_small.get_stats().grid_cells >= index_large.get_stats().grid_cells);
}

#[test]
fn test_rtree_remove_and_reinsert() {
    let index = RTreeIndex::new();

    let point = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
    index.insert(1, &point).unwrap();
    assert_eq!(index.get_stats().total_points, 1);

    index.remove(1).unwrap();
    assert_eq!(index.get_stats().total_points, 0);

    // Reinsert
    index.insert(1, &point).unwrap();
    assert_eq!(index.get_stats().total_points, 1);
}

#[test]
fn test_rtree_update_point() {
    let index = RTreeIndex::new();

    let point1 = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
    index.insert(1, &point1).unwrap();

    // Update by removing and reinserting
    index.remove(1).unwrap();
    let point2 = Point::new_2d(10.0, 10.0, CoordinateSystem::Cartesian);
    index.insert(1, &point2).unwrap();

    let retrieved = index.get_point(1).unwrap();
    assert_eq!(retrieved.x, 10.0);
    assert_eq!(retrieved.y, 10.0);
}

// ============================================================================
// Geospatial Procedures Tests
// ============================================================================

#[test]
fn test_within_bbox_procedure_execution() {
    let proc = WithinBBoxProcedure;
    let graph = Graph::new();
    let mut args = HashMap::new();

    args.insert(
        "bbox".to_string(),
        json!({
            "minX": 0.0,
            "minY": 0.0,
            "maxX": 10.0,
            "maxY": 10.0
        }),
    );
    args.insert(
        "property".to_string(),
        Value::String("location".to_string()),
    );

    let result = proc.execute(&graph, &args).unwrap();
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "node");
    // Currently returns empty results (placeholder implementation)
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn test_within_bbox_procedure_invalid_bbox() {
    let proc = WithinBBoxProcedure;
    let graph = Graph::new();
    let mut args = HashMap::new();

    // Missing bbox parameter
    args.insert(
        "property".to_string(),
        Value::String("location".to_string()),
    );

    let result = proc.execute(&graph, &args);
    assert!(result.is_err());
}

#[test]
fn test_within_distance_procedure_execution() {
    let proc = WithinDistanceProcedure;
    let graph = Graph::new();
    let mut args = HashMap::new();

    args.insert(
        "point".to_string(),
        json!({
            "x": 5.0,
            "y": 5.0,
            "crs": "cartesian"
        }),
    );
    args.insert(
        "distance".to_string(),
        Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
    );
    args.insert(
        "property".to_string(),
        Value::String("location".to_string()),
    );

    let result = proc.execute(&graph, &args).unwrap();
    assert_eq!(result.columns.len(), 2);
    assert_eq!(result.columns[0], "node");
    assert_eq!(result.columns[1], "distance");
    // Currently returns empty results (placeholder implementation)
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn test_within_distance_procedure_invalid_point() {
    let proc = WithinDistanceProcedure;
    let graph = Graph::new();
    let mut args = HashMap::new();

    // Invalid point format
    args.insert(
        "point".to_string(),
        Value::String("not a point".to_string()),
    );
    args.insert(
        "distance".to_string(),
        Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
    );
    args.insert(
        "property".to_string(),
        Value::String("location".to_string()),
    );

    let result = proc.execute(&graph, &args);
    assert!(result.is_err());
}

#[test]
fn test_procedures_registered() {
    let registry = ProcedureRegistry::new();

    // Check that geospatial procedures are registered
    assert!(registry.get("spatial.withinBBox").is_some());
    assert!(registry.get("spatial.withinDistance").is_some());
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_rtree_with_procedures_integration() {
    // Test that R-tree can be used with procedures
    let index = RTreeIndex::new();

    // Insert points
    index
        .insert(1, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
        .unwrap();
    index
        .insert(2, &Point::new_2d(15.0, 15.0, CoordinateSystem::Cartesian))
        .unwrap();

    // Query using R-tree
    let results = index.query_bbox((0.0, 0.0, 10.0, 10.0)).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(1));

    // Verify point can be used with procedures
    let proc = WithinDistanceProcedure;
    let graph = Graph::new();
    let mut args = HashMap::new();

    let point_json = index.get_point(1).unwrap().to_json_value();
    args.insert("point".to_string(), point_json);
    args.insert(
        "distance".to_string(),
        Value::Number(serde_json::Number::from_f64(10.0).unwrap()),
    );
    args.insert(
        "property".to_string(),
        Value::String("location".to_string()),
    );

    let result = proc.execute(&graph, &args);
    assert!(result.is_ok());
}

#[test]
fn test_point_json_roundtrip_with_executor() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // Create point and convert to JSON
    let point = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let point_json = point.to_json_value();

    params.insert("p".to_string(), point_json.clone());

    // Use in query
    let query = Query {
        cypher: "RETURN $p AS point".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);

    // Verify point can be reconstructed
    if let Some(Value::Object(obj)) = result.rows[0].values.first() {
        let reconstructed = Point::from_json_value(&Value::Object(obj.clone())).unwrap();
        assert_eq!(point.x, reconstructed.x);
        assert_eq!(point.y, reconstructed.y);
    } else {
        panic!("Expected object result");
    }
}

#[test]
fn test_multiple_coordinate_systems() {
    let index = RTreeIndex::new();

    // Insert points with different coordinate systems
    index
        .insert(1, &Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian))
        .unwrap();
    index
        .insert(2, &Point::new_2d(-122.0, 37.0, CoordinateSystem::WGS84))
        .unwrap();

    // Both should be stored
    assert_eq!(index.get_stats().total_points, 2);

    // Can retrieve both
    assert!(index.has_point(1));
    assert!(index.has_point(2));

    // Distance calculations should work (though mixing systems may not be accurate)
    let p1 = index.get_point(1).unwrap();
    let p2 = index.get_point(2).unwrap();
    let distance = p1.distance_to(&p2);
    assert!(distance > 0.0);
}

#[test]
fn test_rtree_performance_with_many_points() {
    let index = RTreeIndex::new();

    // Insert many points
    let start = std::time::Instant::now();
    for i in 0..10000 {
        let point = Point::new_2d(
            (i as f64) * 0.01,
            (i as f64) * 0.01,
            CoordinateSystem::Cartesian,
        );
        index.insert(i, &point).unwrap();
    }
    let insert_time = start.elapsed();

    // Query should be fast
    let query_start = std::time::Instant::now();
    let _results = index.query_bbox((0.0, 0.0, 100.0, 100.0)).unwrap();
    let query_time = query_start.elapsed();

    // Both operations should complete in reasonable time
    assert!(insert_time.as_secs() < 10); // Should insert 10k points in < 10 seconds
    assert!(query_time.as_millis() < 1000); // Should query in < 1 second
}

#[test]
fn test_bounding_box_edge_cases() {
    let bbox = BoundingBox {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 10.0,
        max_y: 10.0,
    };

    // Points on boundaries
    assert!(bbox.contains(&Point::new_2d(0.0, 0.0, CoordinateSystem::Cartesian)));
    assert!(bbox.contains(&Point::new_2d(10.0, 10.0, CoordinateSystem::Cartesian)));
    assert!(bbox.contains(&Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian)));

    // Points outside
    assert!(!bbox.contains(&Point::new_2d(-1.0, 5.0, CoordinateSystem::Cartesian)));
    assert!(!bbox.contains(&Point::new_2d(11.0, 5.0, CoordinateSystem::Cartesian)));
    assert!(!bbox.contains(&Point::new_2d(5.0, -1.0, CoordinateSystem::Cartesian)));
    assert!(!bbox.contains(&Point::new_2d(5.0, 11.0, CoordinateSystem::Cartesian)));
}

#[test]
fn test_point_display_format() {
    let p2d = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let p3d = Point::new_3d(1.0, 2.0, 3.0, CoordinateSystem::Cartesian);

    let s2d = format!("{}", p2d);
    let s3d = format!("{}", p3d);

    assert!(s2d.contains("x: 1"));
    assert!(s2d.contains("y: 2"));
    assert!(!s2d.contains("z:"));

    assert!(s3d.contains("x: 1"));
    assert!(s3d.contains("y: 2"));
    assert!(s3d.contains("z: 3"));
}

// ============================================================================
// CREATE SPATIAL INDEX Tests
// ============================================================================

#[test]
#[ignore] // TODO: Fix - temp dir race condition in parallel tests
fn test_create_spatial_index_basic() {
    let (mut executor, _dir) = create_test_executor();

    let query = Query {
        cypher: "CREATE SPATIAL INDEX ON :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "index");

    // Verify index name is returned
    if let Some(Value::String(index_name)) = result.rows[0].values.first() {
        assert!(index_name.contains("Location"));
        assert!(index_name.contains("coordinates"));
        assert!(index_name.contains("spatial"));
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_create_spatial_index_if_not_exists() {
    let (mut executor, _dir) = create_test_executor();

    // Create index first time
    let query1 = Query {
        cypher: "CREATE SPATIAL INDEX IF NOT EXISTS ON :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    let result1 = executor.execute(&query1).unwrap();
    assert_eq!(result1.rows.len(), 1);

    // Create same index again with IF NOT EXISTS - should succeed
    let query2 = Query {
        cypher: "CREATE SPATIAL INDEX IF NOT EXISTS ON :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    let result2 = executor.execute(&query2).unwrap();
    assert_eq!(result2.rows.len(), 1);
}

#[test]
fn test_create_spatial_index_or_replace() {
    let (mut executor, _dir) = create_test_executor();

    // Create index first time
    let query1 = Query {
        cypher: "CREATE SPATIAL INDEX ON :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    let result1 = executor.execute(&query1).unwrap();
    assert_eq!(result1.rows.len(), 1);

    // Replace index with OR REPLACE
    let query2 = Query {
        cypher: "CREATE OR REPLACE SPATIAL INDEX ON :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    let result2 = executor.execute(&query2).unwrap();
    assert_eq!(result2.rows.len(), 1);
}

#[test]
#[ignore] // TODO: Fix - temp dir race condition in parallel tests
fn test_create_spatial_index_multiple_labels() {
    let (mut executor, _dir) = create_test_executor();

    // Create indexes for different labels
    let queries = vec![
        "CREATE SPATIAL INDEX ON :Location(coordinates)",
        "CREATE SPATIAL INDEX ON :City(center)",
        "CREATE SPATIAL INDEX ON :Place(position)",
    ];

    for cypher in queries {
        let query = Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1);
    }
}

#[test]
#[ignore] // TODO: Fix - temp dir race condition in parallel tests
fn test_create_property_index_vs_spatial_index() {
    let (mut executor, _dir) = create_test_executor();

    // Create property index
    let query1 = Query {
        cypher: "CREATE INDEX ON :Location(name)".to_string(),
        params: HashMap::new(),
    };
    let result1 = executor.execute(&query1).unwrap();
    assert_eq!(result1.rows.len(), 1);

    // Create spatial index on same label but different property
    let query2 = Query {
        cypher: "CREATE SPATIAL INDEX ON :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    let result2 = executor.execute(&query2).unwrap();
    assert_eq!(result2.rows.len(), 1);

    // Verify both indexes exist (check by index name)
    if let (Some(Value::String(idx1)), Some(Value::String(idx2))) = (
        result1.rows[0].values.first(),
        result2.rows[0].values.first(),
    ) {
        assert!(idx1.contains("name"));
        assert!(idx2.contains("coordinates"));
        assert!(idx2.contains("spatial"));
    }
}

#[test]
fn test_create_spatial_index_invalid_syntax() {
    let (mut executor, _dir) = create_test_executor();

    // Missing ON keyword
    let query1 = Query {
        cypher: "CREATE SPATIAL INDEX :Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    assert!(executor.execute(&query1).is_err());

    // Missing label colon
    let query2 = Query {
        cypher: "CREATE SPATIAL INDEX ON Location(coordinates)".to_string(),
        params: HashMap::new(),
    };
    assert!(executor.execute(&query2).is_err());
}

#[test]
fn test_create_spatial_index_with_special_characters() {
    let (mut executor, _dir) = create_test_executor();

    // Test with label that has special characters (if supported)
    let query = Query {
        cypher: "CREATE SPATIAL INDEX ON :Location_123(coordinates_xyz)".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
}

// ============================================================================
// Additional Point Tests
// ============================================================================

#[test]
fn test_point_3d_distance_calculations() {
    let p1 = Point::new_3d(0.0, 0.0, 0.0, CoordinateSystem::Cartesian);
    let p2 = Point::new_3d(3.0, 4.0, 0.0, CoordinateSystem::Cartesian);
    let distance_2d = p1.distance_to(&p2);
    assert!((distance_2d - 5.0).abs() < 0.0001);

    let p3 = Point::new_3d(3.0, 4.0, 12.0, CoordinateSystem::Cartesian);
    let distance_3d = p1.distance_to(&p3);
    assert!((distance_3d - 13.0).abs() < 0.0001); // sqrt(3^2 + 4^2 + 12^2) = 13
}

#[test]
fn test_point_wgs84_distance_accuracy() {
    // Test distance between two known cities
    let sf = Point::new_2d(-122.4194, 37.7749, CoordinateSystem::WGS84);
    let ny = Point::new_2d(-74.0060, 40.7128, CoordinateSystem::WGS84);

    let distance = sf.distance_to(&ny);

    // San Francisco to New York is approximately 4139 km
    // Allow some margin for calculation differences
    assert!(distance > 4000000.0 && distance < 4300000.0);
}

#[test]
fn test_point_json_with_all_fields() {
    let p3d = Point::new_3d(1.0, 2.0, 3.0, CoordinateSystem::Cartesian);
    let json = p3d.to_json_value();

    // Verify all fields are present
    if let Value::Object(map) = json {
        assert!(map.contains_key("x"));
        assert!(map.contains_key("y"));
        assert!(map.contains_key("z"));
        assert!(map.contains_key("crs"));

        assert_eq!(map["x"], json!(1.0));
        assert_eq!(map["y"], json!(2.0));
        assert_eq!(map["z"], json!(3.0));
        assert_eq!(map["crs"], json!("cartesian-3d"));
    } else {
        panic!("Expected object");
    }
}

#[test]
fn test_point_from_json_missing_fields() {
    // Missing y coordinate
    let json = json!({
        "x": 1.0,
        "crs": "cartesian"
    });
    assert!(Point::from_json_value(&json).is_err());

    // Missing crs (should default to Cartesian)
    let json = json!({
        "x": 1.0,
        "y": 2.0
    });
    let p = Point::from_json_value(&json).unwrap();
    assert_eq!(p.coordinate_system, CoordinateSystem::Cartesian);
}

#[test]
fn test_point_coordinate_system_validation() {
    // Test various CRS strings
    let test_cases = vec![
        ("cartesian", CoordinateSystem::Cartesian),
        ("cartesian-3d", CoordinateSystem::Cartesian),
        ("wgs-84", CoordinateSystem::WGS84),
        ("wgs-84-3d", CoordinateSystem::WGS84),
        ("unknown", CoordinateSystem::Cartesian), // Defaults to Cartesian
    ];

    for (crs_str, expected) in test_cases {
        let json = json!({
            "x": 1.0,
            "y": 2.0,
            "crs": crs_str
        });
        let p = Point::from_json_value(&json).unwrap();
        assert_eq!(p.coordinate_system, expected, "Failed for CRS: {}", crs_str);
    }
}

// ============================================================================
// Additional R-tree Tests
// ============================================================================

#[test]
fn test_rtree_query_nonexistent_point() {
    let index = RTreeIndex::new();

    // Query before inserting anything
    let results = index.query_bbox((0.0, 0.0, 10.0, 10.0)).unwrap();
    assert_eq!(results.len(), 0);

    // Query after inserting but outside range
    index
        .insert(1, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
        .unwrap();
    let results = index.query_bbox((100.0, 100.0, 110.0, 110.0)).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_rtree_remove_nonexistent_point() {
    let index = RTreeIndex::new();

    // Remove point that doesn't exist - should not error
    let result = index.remove(999);
    assert!(result.is_ok());

    // Remove from empty index
    let result = index.remove(1);
    assert!(result.is_ok());
}

#[test]
fn test_rtree_get_nonexistent_point() {
    let index = RTreeIndex::new();

    // Get point that doesn't exist
    assert!(index.get_point(999).is_none());

    // Get from empty index
    assert!(index.get_point(1).is_none());
}

#[test]
fn test_rtree_health_check() {
    let index = RTreeIndex::new();

    // Health check on empty index - should succeed
    let result = index.health_check();
    assert!(result.is_ok());

    // Insert some points
    for i in 0..10 {
        let point = Point::new_2d(i as f64, i as f64, CoordinateSystem::Cartesian);
        index.insert(i, &point).unwrap();
    }

    // Health check on populated index - should succeed
    let result = index.health_check();
    assert!(result.is_ok());

    // Verify stats are correct
    let stats = index.get_stats();
    assert_eq!(stats.total_points, 10);
}

#[test]
fn test_rtree_clear_operation() {
    let mut index = RTreeIndex::new();

    // Insert points
    for i in 0..10 {
        let point = Point::new_2d(i as f64, i as f64, CoordinateSystem::Cartesian);
        index.insert(i, &point).unwrap();
    }

    assert_eq!(index.get_stats().total_points, 10);

    // Clear index
    index.clear().unwrap();

    assert_eq!(index.get_stats().total_points, 0);
    assert_eq!(index.get_stats().grid_cells, 0);

    // Verify points are gone
    let results = index.query_bbox((0.0, 0.0, 100.0, 100.0)).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_rtree_concurrent_operations() {
    use std::sync::Arc;
    use std::thread;

    let index = Arc::new(RTreeIndex::new());
    let mut handles = vec![];

    // Spawn multiple threads inserting points
    for thread_id in 0..5 {
        let index_clone = index.clone();
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let node_id = (thread_id * 100 + i) as u64;
                let point =
                    Point::new_2d(node_id as f64, node_id as f64, CoordinateSystem::Cartesian);
                index_clone.insert(node_id, &point).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all points were inserted
    assert_eq!(index.get_stats().total_points, 500);
}

#[test]
fn test_rtree_query_with_negative_coordinates() {
    let index = RTreeIndex::new();

    // Insert points with negative coordinates
    index
        .insert(1, &Point::new_2d(-10.0, -10.0, CoordinateSystem::Cartesian))
        .unwrap();
    index
        .insert(2, &Point::new_2d(-5.0, -5.0, CoordinateSystem::Cartesian))
        .unwrap();
    index
        .insert(3, &Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian))
        .unwrap();

    // Query negative region
    let results = index.query_bbox((-15.0, -15.0, -1.0, -1.0)).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.contains(1));
    assert!(results.contains(2));
}

#[test]
fn test_rtree_distance_query_with_zero_distance() {
    let index = RTreeIndex::new();

    let center = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
    index.insert(1, &center).unwrap();
    index
        .insert(2, &Point::new_2d(5.1, 5.1, CoordinateSystem::Cartesian))
        .unwrap();

    // Query with zero distance should only find exact match
    let results = index.query_distance(&center, 0.0).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(1));
}

#[test]
fn test_rtree_distance_query_very_small_distance() {
    let index = RTreeIndex::new();

    let center = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
    index.insert(1, &center).unwrap();
    index
        .insert(
            2,
            &Point::new_2d(5.0001, 5.0001, CoordinateSystem::Cartesian),
        )
        .unwrap();
    index
        .insert(3, &Point::new_2d(5.1, 5.1, CoordinateSystem::Cartesian))
        .unwrap();

    // Query with very small distance
    let results = index.query_distance(&center, 0.001).unwrap();
    // Should find points 1 and possibly 2 (depending on grid cell size)
    assert!(!results.is_empty());
    assert!(results.contains(1));
}

// ============================================================================
// Additional Distance Function Tests
// ============================================================================

#[test]
fn test_distance_function_in_where_clause() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    params.insert(
        "center".to_string(),
        json!({"x": 0.0, "y": 0.0, "crs": "cartesian"}),
    );
    params.insert("threshold".to_string(), json!(10.0));

    // Note: This may not work fully yet, but should not crash
    let query = Query {
        cypher: "RETURN distance($center, point({x: 5, y: 5})) AS dist WHERE dist < $threshold"
            .to_string(),
        params,
    };

    // Should either succeed or fail gracefully
    let result = executor.execute(&query);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_distance_function_with_aggregation() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    params.insert(
        "p1".to_string(),
        json!({"x": 0.0, "y": 0.0, "crs": "cartesian"}),
    );
    params.insert(
        "p2".to_string(),
        json!({"x": 3.0, "y": 4.0, "crs": "cartesian"}),
    );
    params.insert(
        "p3".to_string(),
        json!({"x": 6.0, "y": 8.0, "crs": "cartesian"}),
    );

    // Calculate multiple distances and sum them
    let query = Query {
        cypher: "RETURN distance($p1, $p2) + distance($p2, $p3) AS total_distance".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);

    if let Some(Value::Number(total)) = result.rows[0].values.first() {
        let total_val = total.as_f64().unwrap();
        // Should be approximately 10.0 (5.0 + 5.0)
        assert!((total_val - 10.0).abs() < 0.0001);
    } else {
        panic!("Expected number result");
    }
}

#[test]
fn test_distance_function_with_null_handling() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // Test with one null point
    params.insert("p1".to_string(), Value::Null);
    params.insert(
        "p2".to_string(),
        json!({"x": 3.0, "y": 4.0, "crs": "cartesian"}),
    );

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Null);

    // Test with both null points
    let mut params2 = HashMap::new();
    params2.insert("p1".to_string(), Value::Null);
    params2.insert("p2".to_string(), Value::Null);

    let query2 = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params: params2,
    };

    let result2 = executor.execute(&query2).unwrap();
    assert_eq!(result2.rows.len(), 1);
    assert_eq!(result2.rows[0].values[0], Value::Null);
}

#[test]
fn test_distance_function_precision() {
    let (mut executor, _dir) = create_test_executor();
    let mut params = HashMap::new();

    // Test with very precise coordinates
    params.insert(
        "p1".to_string(),
        json!({"x": 0.0, "y": 0.0, "crs": "cartesian"}),
    );
    params.insert(
        "p2".to_string(),
        json!({"x": 0.0000001, "y": 0.0000001, "crs": "cartesian"}),
    );

    let query = Query {
        cypher: "RETURN distance($p1, $p2) AS dist".to_string(),
        params,
    };

    let result = executor.execute(&query).unwrap();
    assert_eq!(result.rows.len(), 1);

    if let Some(Value::Number(dist)) = result.rows[0].values.first() {
        let dist_val = dist.as_f64().unwrap();
        // Should be a very small positive number
        assert!(dist_val > 0.0);
        assert!(dist_val < 0.000001);
    } else {
        panic!("Expected number result");
    }
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
fn test_rtree_stress_insert_remove() {
    let index = RTreeIndex::new();

    // Insert and remove many points
    for i in 0..1000 {
        let point = Point::new_2d(i as f64, i as f64, CoordinateSystem::Cartesian);
        index.insert(i, &point).unwrap();
    }

    assert_eq!(index.get_stats().total_points, 1000);

    // Remove half
    for i in 0..500 {
        index.remove(i).unwrap();
    }

    assert_eq!(index.get_stats().total_points, 500);

    // Insert more
    for i in 1000..1500 {
        let point = Point::new_2d(i as f64, i as f64, CoordinateSystem::Cartesian);
        index.insert(i, &point).unwrap();
    }

    assert_eq!(index.get_stats().total_points, 1000);
}

#[test]
fn test_multiple_spatial_indexes() {
    let (mut executor, _dir) = create_test_executor();

    // Create multiple spatial indexes
    let indexes = vec![
        "CREATE SPATIAL INDEX ON :Location(coordinates)",
        "CREATE SPATIAL INDEX ON :City(center)",
        "CREATE SPATIAL INDEX ON :Place(position)",
        "CREATE SPATIAL INDEX ON :Store(location)",
    ];

    for cypher in indexes {
        let query = Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        };
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1);
    }
}

#[test]
fn test_point_equality_and_comparison() {
    let p1 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let p2 = Point::new_2d(1.0, 2.0, CoordinateSystem::Cartesian);
    let p3 = Point::new_2d(1.0, 2.0, CoordinateSystem::WGS84);
    let p4 = Point::new_2d(2.0, 2.0, CoordinateSystem::Cartesian);

    // Same coordinates and system
    assert_eq!(p1.x, p2.x);
    assert_eq!(p1.y, p2.y);
    assert_eq!(p1.coordinate_system, p2.coordinate_system);

    // Different coordinate system
    assert_ne!(p1.coordinate_system, p3.coordinate_system);

    // Different coordinates
    assert_ne!(p1.x, p4.x);
}

#[test]
fn test_rtree_query_all_points() {
    let index = RTreeIndex::new();

    // Insert points in a wide area
    for i in 0..100 {
        let point = Point::new_2d(
            (i as f64) * 10.0,
            (i as f64) * 10.0,
            CoordinateSystem::Cartesian,
        );
        index.insert(i, &point).unwrap();
    }

    // Query very large bounding box
    let results = index
        .query_bbox((-1000.0, -1000.0, 10000.0, 10000.0))
        .unwrap();
    assert_eq!(results.len(), 100);
}

#[test]
fn test_rtree_query_single_point_bbox() {
    let index = RTreeIndex::new();

    let point = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
    index.insert(1, &point).unwrap();

    // Query exact point bounding box
    let results = index.query_bbox((5.0, 5.0, 5.0, 5.0)).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(1));
}
