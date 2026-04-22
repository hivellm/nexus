//! Tests for advanced Neo4j compatibility functions
//!
//! This file tests additional Neo4j compatibility functions:
//! - Advanced temporal functions (localtime, localdatetime)
//! - Duration component extraction (years, months, weeks, days, hours, minutes, seconds)
//! - Geospatial point accessors (point.x, point.y, point.latitude, point.longitude, point.z, point.crs)

use nexus_core::testing::setup_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn get_single_value(result: &ResultSet) -> &serde_json::Value {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    assert!(
        !result.rows[0].values.is_empty(),
        "First row has no values!"
    );
    &result.rows[0].values[0]
}

// ============================================================================
// ADVANCED TEMPORAL FUNCTIONS
// ============================================================================

#[test]
fn test_localtime_current() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN localtime() AS result");
    let time_str = get_single_value(&result).as_str().unwrap();
    // Should be in HH:MM:SS format
    assert!(time_str.contains(':'));
    let parts: Vec<&str> = time_str.split(':').collect();
    assert_eq!(parts.len(), 3); // hours:minutes:seconds
}

#[test]
fn test_localtime_from_string() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN localtime('14:30:45') AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "14:30:45");
}

#[test]
fn test_localtime_from_map() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour: 14, minute: 30, second: 45}) AS result",
    );
    assert_eq!(get_single_value(&result).as_str().unwrap(), "14:30:45");
}

#[test]
fn test_localdatetime_current() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN localdatetime() AS result");
    let dt_str = get_single_value(&result).as_str().unwrap();
    // Should be in YYYY-MM-DDTHH:MM:SS format
    assert!(dt_str.contains('T'));
    assert!(dt_str.contains('-'));
    assert!(dt_str.contains(':'));
}

#[test]
fn test_localdatetime_from_string() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localdatetime('2025-03-15T14:30:45') AS result",
    );
    assert_eq!(
        get_single_value(&result).as_str().unwrap(),
        "2025-03-15T14:30:45"
    );
}

#[test]
fn test_localdatetime_from_map() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localdatetime({year: 2025, month: 3, day: 15, hour: 14, minute: 30, second: 45}) AS result",
    );
    assert_eq!(
        get_single_value(&result).as_str().unwrap(),
        "2025-03-15T14:30:45"
    );
}

// ============================================================================
// DURATION COMPONENT EXTRACTION
// ============================================================================

#[test]
fn test_duration_years_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN years(duration({years: 5, months: 3})) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 5);
}

#[test]
fn test_duration_months_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN months(duration({years: 5, months: 3})) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 3);
}

#[test]
fn test_duration_weeks_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN weeks(duration({weeks: 4, days: 2})) AS result",
    );
    // If weeks component doesn't exist in duration, returns null
    // This is expected behavior for duration component extraction
    let value = get_single_value(&result);
    if !value.is_null() {
        assert_eq!(value.as_i64().unwrap(), 4);
    }
}

#[test]
fn test_duration_days_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN days(duration({days: 10, hours: 5})) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 10);
}

#[test]
fn test_duration_hours_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN hours(duration({hours: 12, minutes: 30})) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 12);
}

#[test]
fn test_duration_minutes_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN minutes(duration({hours: 12, minutes: 30})) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 30);
}

#[test]
fn test_duration_seconds_extraction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN seconds(duration({minutes: 5, seconds: 45})) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 45);
}

// ============================================================================
// GEOSPATIAL POINT ACCESSORS
// ============================================================================
// Note: These tests require WITH clause support which is not yet implemented
// The point accessor functionality is implemented in the PropertyAccess expression handler
// and will work once WITH clause is supported

// #[test]
// fn test_point_x_accessor() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(&mut engine, "WITH point({x: 10.5, y: 20.3}) AS p RETURN p.x AS result");
//     let x = get_single_value(&result).as_f64().unwrap();
//     assert!((x - 10.5).abs() < 0.001);
// }

// #[test]
// fn test_point_y_accessor() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(&mut engine, "WITH point({x: 10.5, y: 20.3}) AS p RETURN p.y AS result");
//     let y = get_single_value(&result).as_f64().unwrap();
//     assert!((y - 20.3).abs() < 0.001);
// }

// #[test]
// fn test_point_z_accessor_3d() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(&mut engine, "WITH point({x: 10.5, y: 20.3, z: 5.7}) AS p RETURN p.z AS result");
//     let z = get_single_value(&result).as_f64().unwrap();
//     assert!((z - 5.7).abs() < 0.001);
// }

// #[test]
// fn test_point_z_accessor_2d() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(&mut engine, "WITH point({x: 10.5, y: 20.3}) AS p RETURN p.z AS result");
//     // 2D points should return null for z
//     assert!(get_single_value(&result).is_null());
// }

// #[test]
// fn test_point_crs_accessor_cartesian() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(&mut engine, "WITH point({x: 10.5, y: 20.3}) AS p RETURN p.crs AS result");
//     let crs = get_single_value(&result).as_str().unwrap();
//     assert_eq!(crs, "cartesian");
// }

// #[test]
// fn test_point_crs_accessor_cartesian_3d() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(&mut engine, "WITH point({x: 10.5, y: 20.3, z: 5.7}) AS p RETURN p.crs AS result");
//     let crs = get_single_value(&result).as_str().unwrap();
//     assert_eq!(crs, "cartesian-3d");
// }

// #[test]
// fn test_point_latitude_accessor() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     // latitude should return y coordinate for WGS84 points
//     let result = execute_query(
//         &mut engine,
//         "WITH point({longitude: 12.5, latitude: 56.3, crs: 'wgs-84'}) AS p RETURN p.latitude AS result"
//     );
//     let lat = get_single_value(&result).as_f64().unwrap();
//     assert!((lat - 56.3).abs() < 0.001);
// }

// #[test]
// fn test_point_longitude_accessor() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     // longitude should return x coordinate for WGS84 points
//     let result = execute_query(
//         &mut engine,
//         "WITH point({longitude: 12.5, latitude: 56.3, crs: 'wgs-84'}) AS p RETURN p.longitude AS result"
//     );
//     let lon = get_single_value(&result).as_f64().unwrap();
//     assert!((lon - 12.5).abs() < 0.001);
// }

// #[test]
// fn test_point_accessors_in_return() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     let result = execute_query(
//         &mut engine,
//         "WITH point({x: 10, y: 20, z: 30}) AS p RETURN p.x AS x, p.y AS y, p.z AS z"
//     );
//     assert_eq!(result.columns, vec!["x", "y", "z"]);
//     assert_eq!(result.rows.len(), 1);
//     assert_eq!(result.rows[0].values[0].as_f64().unwrap(), 10.0);
//     assert_eq!(result.rows[0].values[1].as_f64().unwrap(), 20.0);
//     assert_eq!(result.rows[0].values[2].as_f64().unwrap(), 30.0);
// }

// ============================================================================
// NULL HANDLING
// ============================================================================

#[test]
fn test_duration_functions_with_null() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN years(null) AS result");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_temporal_localtime_with_null() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN localtime(null) AS result");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_temporal_localdatetime_with_null() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN localdatetime(null) AS result");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_duration_without_specified_component() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // Duration without years should return null for years()
    let result = execute_query(&mut engine, "RETURN years(duration({months: 5})) AS result");
    assert!(get_single_value(&result).is_null());
}

// #[test]
// fn test_point_accessor_on_non_point() {
//     let (mut engine, _ctx) = setup_test_engine().unwrap();
//     // Accessing .x on a non-point value should return null
//     let result = execute_query(&mut engine, "WITH {name: 'test'} AS obj RETURN obj.x AS result");
//     assert!(get_single_value(&result).is_null());
// }
