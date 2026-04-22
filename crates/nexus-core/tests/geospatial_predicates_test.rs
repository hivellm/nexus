//! End-to-end coverage for `point.*` predicates and `spatial.*`
//! procedures (phase6_opencypher-geospatial-predicates slice A).
//!
//! These exercises drive the full Cypher surface — parser ->
//! executor -> projection evaluator -> procedure dispatch. They
//! complement `geospatial_integration_test.rs`, which targets the
//! `Point` / `distance()` primitives and the original procedure
//! signatures, by validating the predicates + dispatch wiring
//! this task actually ships.

use nexus_core::executor::Query;
use nexus_core::geospatial::{CoordinateSystem, Point};
use nexus_core::testing::create_test_executor;
use serde_json::{Value, json};
use std::collections::HashMap;

fn cart(x: f64, y: f64) -> Value {
    Point::new_2d(x, y, CoordinateSystem::Cartesian).to_json_value()
}

fn wgs(lon: f64, lat: f64) -> Value {
    Point::new_2d(lon, lat, CoordinateSystem::WGS84).to_json_value()
}

fn run(cypher: &str, params: HashMap<String, Value>) -> Vec<Vec<Value>> {
    let (executor, _ctx) = create_test_executor();
    let result = executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params,
        })
        .unwrap_or_else(|e| panic!("cypher execution failed: {e}\nquery: {cypher}"));
    result.rows.into_iter().map(|r| r.values).collect()
}

fn run_err(cypher: &str, params: HashMap<String, Value>) -> String {
    let (executor, _ctx) = create_test_executor();
    let err = executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params,
        })
        .expect_err("expected cypher execution to fail");
    err.to_string()
}

// ============================================================================
// point.withinBBox
// ============================================================================

#[test]
fn point_within_bbox_inside() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(1.0, 1.0));
    params.insert(
        "bbox".to_string(),
        json!({
            "bottomLeft": cart(0.0, 0.0),
            "topRight": cart(2.0, 2.0),
        }),
    );
    let rows = run("RETURN point.withinBBox($p, $bbox) AS v", params);
    assert_eq!(rows[0][0], Value::Bool(true));
}

#[test]
fn point_within_bbox_outside() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(3.0, 3.0));
    params.insert(
        "bbox".to_string(),
        json!({
            "bottomLeft": cart(0.0, 0.0),
            "topRight": cart(2.0, 2.0),
        }),
    );
    let rows = run("RETURN point.withinBBox($p, $bbox) AS v", params);
    assert_eq!(rows[0][0], Value::Bool(false));
}

#[test]
fn point_within_bbox_crs_mismatch_errors() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(1.0, 1.0));
    params.insert(
        "bbox".to_string(),
        json!({
            "bottomLeft": wgs(0.0, 0.0),
            "topRight": wgs(2.0, 2.0),
        }),
    );
    let err = run_err("RETURN point.withinBBox($p, $bbox) AS v", params);
    assert!(
        err.contains("ERR_CRS_MISMATCH"),
        "expected ERR_CRS_MISMATCH, got: {err}"
    );
}

#[test]
fn point_within_bbox_malformed_bbox_errors() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(1.0, 1.0));
    params.insert("bbox".to_string(), json!({"a": 1}));
    let err = run_err("RETURN point.withinBBox($p, $bbox) AS v", params);
    assert!(
        err.contains("ERR_BBOX_MALFORMED"),
        "expected ERR_BBOX_MALFORMED, got: {err}"
    );
}

// ============================================================================
// point.withinDistance
// ============================================================================

#[test]
fn point_within_distance_close_points_match() {
    // Paris -> Rivoli ~ 500 m; 1 km radius must hit.
    let mut params = HashMap::new();
    params.insert("paris".to_string(), wgs(2.3522, 48.8566));
    params.insert("rivoli".to_string(), wgs(2.3615, 48.8606));
    params.insert("d".to_string(), json!(1000.0));
    let rows = run(
        "RETURN point.withinDistance($paris, $rivoli, $d) AS v",
        params,
    );
    assert_eq!(rows[0][0], Value::Bool(true));
}

#[test]
fn point_within_distance_far_points_do_not_match() {
    // Paris -> Berlin ~ 878 km; 100 km radius must miss.
    let mut params = HashMap::new();
    params.insert("paris".to_string(), wgs(2.3522, 48.8566));
    params.insert("berlin".to_string(), wgs(13.4050, 52.5200));
    params.insert("d".to_string(), json!(100_000.0));
    let rows = run(
        "RETURN point.withinDistance($paris, $berlin, $d) AS v",
        params,
    );
    assert_eq!(rows[0][0], Value::Bool(false));
}

#[test]
fn point_within_distance_crs_mismatch_errors() {
    let mut params = HashMap::new();
    params.insert("a".to_string(), cart(0.0, 0.0));
    params.insert("b".to_string(), wgs(0.0, 0.0));
    params.insert("d".to_string(), json!(10.0));
    let err = run_err("RETURN point.withinDistance($a, $b, $d) AS v", params);
    assert!(err.contains("ERR_CRS_MISMATCH"));
}

// ============================================================================
// point.azimuth
// ============================================================================

#[test]
fn point_azimuth_due_east_wgs84() {
    let mut params = HashMap::new();
    params.insert("a".to_string(), wgs(0.0, 0.0));
    params.insert("b".to_string(), wgs(1.0, 0.0));
    let rows = run("RETURN point.azimuth($a, $b) AS deg", params);
    let deg = rows[0][0].as_f64().unwrap();
    assert!((deg - 90.0).abs() < 0.5, "deg={deg}");
}

#[test]
fn point_azimuth_cartesian_east_is_zero_degrees() {
    // Cartesian azimuth uses the +x-axis as 0 degrees.
    let mut params = HashMap::new();
    params.insert("a".to_string(), cart(0.0, 0.0));
    params.insert("b".to_string(), cart(10.0, 0.0));
    let rows = run("RETURN point.azimuth($a, $b) AS deg", params);
    let deg = rows[0][0].as_f64().unwrap();
    assert!(deg.abs() < 1e-6, "deg={deg}");
}

#[test]
fn point_azimuth_same_point_returns_null() {
    let mut params = HashMap::new();
    params.insert("a".to_string(), cart(1.0, 1.0));
    params.insert("b".to_string(), cart(1.0, 1.0));
    let rows = run("RETURN point.azimuth($a, $b) AS deg", params);
    assert_eq!(rows[0][0], Value::Null);
}

// ============================================================================
// point.distance (namespaced alias)
// ============================================================================

#[test]
fn point_distance_matches_bare_distance() {
    let mut params = HashMap::new();
    params.insert("a".to_string(), cart(0.0, 0.0));
    params.insert("b".to_string(), cart(3.0, 4.0));
    let rows = run(
        "RETURN point.distance($a, $b) AS a, distance($a, $b) AS b",
        params,
    );
    assert_eq!(rows[0][0], rows[0][1]);
    assert!((rows[0][0].as_f64().unwrap() - 5.0).abs() < 1e-9);
}

// ============================================================================
// spatial.* procedure dispatch
// ============================================================================

#[test]
fn spatial_bbox_procedure_returns_axis_aligned_rect() {
    let mut params = HashMap::new();
    params.insert(
        "pts".to_string(),
        Value::Array(vec![cart(1.0, 1.0), cart(5.0, 2.0), cart(3.0, 7.0)]),
    );
    let rows = run("CALL spatial.bbox($pts) YIELD bbox RETURN bbox", params);
    let m = rows[0][0].as_object().unwrap();
    let bl = Point::from_json_value(&m["bottomLeft"]).unwrap();
    let tr = Point::from_json_value(&m["topRight"]).unwrap();
    assert_eq!((bl.x, bl.y), (1.0, 1.0));
    assert_eq!((tr.x, tr.y), (5.0, 7.0));
}

#[test]
fn spatial_distance_procedure_paris_to_berlin() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), wgs(2.3522, 48.8566));
    params.insert("b".to_string(), wgs(13.4050, 52.5200));
    let rows = run(
        "CALL spatial.distance($p, $b) YIELD meters RETURN meters",
        params,
    );
    let m = rows[0][0].as_f64().unwrap();
    assert!((m - 878_000.0).abs() < 10_000.0, "meters={m}");
}

#[test]
fn spatial_within_distance_procedure_returns_boolean() {
    let mut params = HashMap::new();
    params.insert("a".to_string(), cart(0.0, 0.0));
    params.insert("b".to_string(), cart(3.0, 4.0));
    params.insert("d".to_string(), json!(10.0));
    let rows = run(
        "CALL spatial.withinDistance($a, $b, $d) YIELD within RETURN within",
        params,
    );
    assert_eq!(rows[0][0], Value::Bool(true));
}

#[test]
fn spatial_within_bbox_procedure_returns_boolean() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(5.0, 5.0));
    params.insert(
        "bbox".to_string(),
        json!({"bottomLeft": cart(0.0, 0.0), "topRight": cart(10.0, 10.0)}),
    );
    let rows = run(
        "CALL spatial.withinBBox($p, $bbox) YIELD within RETURN within",
        params,
    );
    assert_eq!(rows[0][0], Value::Bool(true));
}

#[test]
fn spatial_interpolate_midpoint() {
    let mut params = HashMap::new();
    params.insert(
        "line".to_string(),
        Value::Array(vec![cart(0.0, 0.0), cart(10.0, 0.0)]),
    );
    params.insert("frac".to_string(), json!(0.5));
    let rows = run(
        "CALL spatial.interpolate($line, $frac) YIELD point RETURN point",
        params,
    );
    let p = Point::from_json_value(&rows[0][0]).unwrap();
    assert!((p.x - 5.0).abs() < 1e-9);
    assert!((p.y - 0.0).abs() < 1e-9);
}

#[test]
fn spatial_interpolate_rejects_out_of_range_frac() {
    let mut params = HashMap::new();
    params.insert(
        "line".to_string(),
        Value::Array(vec![cart(0.0, 0.0), cart(10.0, 0.0)]),
    );
    params.insert("frac".to_string(), json!(1.5));
    let err = run_err(
        "CALL spatial.interpolate($line, $frac) YIELD point RETURN point",
        params,
    );
    assert!(err.contains("ERR_INVALID_ARG_VALUE"), "got: {err}");
}

#[test]
fn spatial_azimuth_procedure_due_east_wgs84() {
    let mut params = HashMap::new();
    params.insert("a".to_string(), wgs(0.0, 0.0));
    params.insert("b".to_string(), wgs(1.0, 0.0));
    let rows = run(
        "CALL spatial.azimuth($a, $b) YIELD degrees RETURN degrees",
        params,
    );
    let deg = rows[0][0].as_f64().unwrap();
    assert!((deg - 90.0).abs() < 0.5, "deg={deg}");
}

#[test]
fn spatial_unknown_procedure_errors_cleanly() {
    let err = run_err(
        "CALL spatial.nonExistentName() YIELD x RETURN x",
        HashMap::new(),
    );
    assert!(
        err.contains("ERR_PROC_NOT_FOUND") || err.contains("not a known spatial.* procedure"),
        "got: {err}"
    );
}

// ============================================================================
// spatial.nearest (engine-aware)
// ============================================================================

#[test]
fn spatial_nearest_returns_top_k_sorted_by_distance() {
    let (executor, _ctx) = create_test_executor();

    // Create the spatial index via DDL and populate it via the
    // Cypher-level `spatial.addPoint` bulk-loader. Auto-populate
    // on CREATE / SET is a follow-up task
    // (phase6_spatial-index-autopopulate).
    executor
        .execute(&Query {
            cypher: "CREATE SPATIAL INDEX ON :Store(loc)".to_string(),
            params: HashMap::new(),
        })
        .unwrap();

    for i in 1..=10u64 {
        let mut params = HashMap::new();
        params.insert("id".to_string(), json!(i));
        params.insert(
            "p".to_string(),
            Point::new_2d(i as f64, 0.0, CoordinateSystem::Cartesian).to_json_value(),
        );
        executor
            .execute(&Query {
                cypher: "CALL spatial.addPoint('Store', 'loc', $id, $p) YIELD added RETURN added"
                    .to_string(),
                params,
            })
            .unwrap();
    }

    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(0.0, 0.0));
    params.insert("k".to_string(), json!(3));
    let result = executor
        .execute(&Query {
            cypher: "CALL spatial.nearest($p, 'Store', $k) YIELD node, dist RETURN node, dist"
                .to_string(),
            params,
        })
        .unwrap();

    assert_eq!(result.rows.len(), 3, "expected top-3 rows");
    let distances: Vec<f64> = result
        .rows
        .iter()
        .map(|r| r.values[1].as_f64().unwrap())
        .collect();
    // Must be ascending.
    for w in distances.windows(2) {
        assert!(w[0] <= w[1], "distances not ascending: {distances:?}");
    }
    assert!((distances[0] - 1.0).abs() < 1e-9);
    assert!((distances[1] - 2.0).abs() < 1e-9);
    assert!((distances[2] - 3.0).abs() < 1e-9);
}

#[test]
fn spatial_add_point_errors_when_index_missing() {
    let mut params = HashMap::new();
    params.insert("id".to_string(), json!(1));
    params.insert("p".to_string(), cart(1.0, 2.0));
    let err = run_err(
        "CALL spatial.addPoint('Ghost', 'nope', $id, $p) YIELD added RETURN added",
        params,
    );
    assert!(
        err.contains("ERR_SPATIAL_INDEX_NOT_FOUND"),
        "expected ERR_SPATIAL_INDEX_NOT_FOUND, got: {err}"
    );
}

#[test]
fn spatial_nearest_errors_when_index_missing() {
    let mut params = HashMap::new();
    params.insert("p".to_string(), cart(0.0, 0.0));
    params.insert("k".to_string(), json!(5));
    let err = run_err(
        "CALL spatial.nearest($p, 'Ghost', $k) YIELD node, dist RETURN node, dist",
        params,
    );
    assert!(
        err.contains("ERR_SPATIAL_INDEX_NOT_FOUND"),
        "expected ERR_SPATIAL_INDEX_NOT_FOUND, got: {err}"
    );
}

// ============================================================================
// Parser regression - namespaced call must NOT shadow property access
// ============================================================================

#[test]
fn namespaced_call_does_not_steal_property_access() {
    // `n.prop` with NO following `(` must keep PropertyAccess
    // semantics; the namespaced-call lookahead only fires on
    // `identifier.identifier(`. Exercise via a real node created
    // in-query so the evaluator reaches the PropertyAccess branch
    // the exact same way existing Cypher queries do.
    let rows = run(
        "CREATE (n:Thing {name: 'hello'}) RETURN n.name AS v",
        HashMap::new(),
    );
    assert_eq!(rows[0][0], Value::String("hello".to_string()));
}
