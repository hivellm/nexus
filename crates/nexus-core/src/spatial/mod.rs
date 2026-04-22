//! `spatial.*` procedure dispatcher
//! (phase6_opencypher-geospatial-predicates §7).
//!
//! Mirrors the [`crate::apoc`] dispatch shape: every pure-value
//! `spatial.*` procedure consumes `Vec<serde_json::Value>`
//! arguments and returns `Result<SpatialResult>`. The executor's
//! `execute_call_procedure` recognises the `spatial.` prefix and
//! routes here before falling back to the legacy
//! `GraphProcedure` registry.
//!
//! Engine-aware procedures (`spatial.nearest`, which consults the
//! shared R-tree index registry) live on the executor itself —
//! not in this module — because they need access to
//! `ExecutorShared::spatial_indexes`. The dispatcher returns
//! `Ok(None)` for those names so the executor keeps ownership.
//!
//! ## Error codes
//!
//! | Code                  | Raised when                                         |
//! |-----------------------|-----------------------------------------------------|
//! | `ERR_CRS_MISMATCH`    | Two points with different CRS or dimensionality     |
//! | `ERR_BBOX_MALFORMED`  | bbox map missing `bottomLeft` / `topRight`          |
//! | `ERR_INVALID_ARG_TYPE`| Argument is not the expected POINT / LIST / number  |
//! | `ERR_MISSING_ARG`     | A required positional argument was not supplied     |
//! | `ERR_INVALID_ARG_VALUE` | e.g. `spatial.interpolate` with `frac ∉ [0,1]`    |

use crate::geospatial::Point;
use crate::{Error, Result};
use serde_json::{Map, Value};

/// Shape returned by every `spatial.*` procedure — column names
/// plus the row cells. Identical layout to the APOC dispatcher's
/// `ApocResult` so the executor treats both the same.
#[derive(Debug)]
pub struct SpatialResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl SpatialResult {
    fn scalar(col: &str, value: Value) -> Self {
        Self {
            columns: vec![col.to_string()],
            rows: vec![vec![value]],
        }
    }
}

/// List every pure-value `spatial.*` procedure name. Used by the
/// `dbms.procedures()` introspection surface and by the
/// not-found error message.
pub fn list_procedures() -> Vec<&'static str> {
    vec![
        "spatial.azimuth",
        "spatial.bbox",
        "spatial.distance",
        "spatial.interpolate",
        "spatial.withinBBox",
        "spatial.withinDistance",
    ]
}

/// Pure-value dispatch entry point. Returns `Ok(None)` when the
/// name is not a pure-value `spatial.*` procedure — the caller
/// then tries the engine-aware path (`spatial.nearest`) before
/// surfacing an `ERR_PROC_NOT_FOUND`.
pub fn dispatch(name: &str, args: Vec<Value>) -> Result<Option<SpatialResult>> {
    if !name.starts_with("spatial.") {
        return Ok(None);
    }
    let out = match name {
        "spatial.azimuth" => Some(azimuth(args)?),
        "spatial.bbox" => Some(bbox(args)?),
        "spatial.distance" => Some(distance(args)?),
        "spatial.interpolate" => Some(interpolate(args)?),
        "spatial.withinBBox" => Some(within_bbox(args)?),
        "spatial.withinDistance" => Some(within_distance(args)?),
        _ => None,
    };
    Ok(out)
}

fn require_arg<'a>(args: &'a [Value], idx: usize, name: &str, proc: &str) -> Result<&'a Value> {
    args.get(idx).ok_or_else(|| {
        Error::CypherExecution(format!(
            "ERR_MISSING_ARG: {proc} requires `{name}` at position {idx}"
        ))
    })
}

fn require_point(value: &Value, arg: &str, proc: &str) -> Result<Point> {
    if !matches!(value, Value::Object(_)) {
        return Err(Error::CypherExecution(format!(
            "ERR_INVALID_ARG_TYPE: {proc} argument `{arg}` must be a POINT (got {value})"
        )));
    }
    Point::from_json_value(value).map_err(|e| {
        Error::CypherExecution(format!(
            "ERR_INVALID_ARG_TYPE: {proc} argument `{arg}` is not a valid POINT: {e}"
        ))
    })
}

fn require_f64(value: &Value, arg: &str, proc: &str) -> Result<f64> {
    value.as_f64().ok_or_else(|| {
        Error::CypherExecution(format!(
            "ERR_INVALID_ARG_TYPE: {proc} argument `{arg}` must be a number (got {value})"
        ))
    })
}

fn require_point_list(value: &Value, arg: &str, proc: &str) -> Result<Vec<Point>> {
    let arr = match value {
        Value::Array(arr) => arr,
        other => {
            return Err(Error::CypherExecution(format!(
                "ERR_INVALID_ARG_TYPE: {proc} argument `{arg}` must be LIST<POINT> (got {other})"
            )));
        }
    };
    let mut points = Vec::with_capacity(arr.len());
    for (i, v) in arr.iter().enumerate() {
        points.push(require_point(v, &format!("{arg}[{i}]"), proc)?);
    }
    Ok(points)
}

fn bbox_pair(value: &Value, proc: &str) -> Result<(Point, Point)> {
    let obj = value.as_object().ok_or_else(|| {
        Error::CypherExecution(format!(
            "ERR_BBOX_MALFORMED: {proc} bbox must be a map (got {value})"
        ))
    })?;
    let bl = obj.get("bottomLeft").ok_or_else(|| {
        Error::CypherExecution(format!(
            "ERR_BBOX_MALFORMED: {proc} bbox missing `bottomLeft`"
        ))
    })?;
    let tr = obj.get("topRight").ok_or_else(|| {
        Error::CypherExecution(format!(
            "ERR_BBOX_MALFORMED: {proc} bbox missing `topRight`"
        ))
    })?;
    let bl_point = require_point(bl, "bbox.bottomLeft", proc)?;
    let tr_point = require_point(tr, "bbox.topRight", proc)?;
    Ok((bl_point, tr_point))
}

fn crs_mismatch_err(proc: &str, a: &Point, b: &Point) -> Error {
    Error::CypherExecution(format!(
        "ERR_CRS_MISMATCH: {proc} expects matching CRS (a={}, b={})",
        a.crs_name(),
        b.crs_name()
    ))
}

fn azimuth(args: Vec<Value>) -> Result<SpatialResult> {
    let proc = "spatial.azimuth";
    let a = require_point(require_arg(&args, 0, "a", proc)?, "a", proc)?;
    let b = require_point(require_arg(&args, 1, "b", proc)?, "b", proc)?;
    if !a.same_crs(&b) {
        return Err(crs_mismatch_err(proc, &a, &b));
    }
    let cell = match a.azimuth_to(&b) {
        Some(deg) => serde_json::Number::from_f64(deg)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        None => Value::Null,
    };
    Ok(SpatialResult::scalar("degrees", cell))
}

fn bbox(args: Vec<Value>) -> Result<SpatialResult> {
    let proc = "spatial.bbox";
    let points = require_point_list(require_arg(&args, 0, "points", proc)?, "points", proc)?;
    if points.is_empty() {
        return Ok(SpatialResult::scalar("bbox", Value::Null));
    }
    // Enforce a single CRS across the whole list. Allowing a
    // mix silently "works" today (we'd still return a map) but
    // the result would be geometrically meaningless.
    let first = &points[0];
    for (i, p) in points.iter().enumerate().skip(1) {
        if !first.same_crs(p) {
            return Err(Error::CypherExecution(format!(
                "ERR_CRS_MISMATCH: {proc} points[0]={}, points[{i}]={}",
                first.crs_name(),
                p.crs_name()
            )));
        }
    }
    let is_3d = first.is_3d();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
    let (mut min_z, mut max_z) = (first.z(), first.z());
    for p in &points[1..] {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
        if is_3d {
            min_z = min_z.min(p.z());
            max_z = max_z.max(p.z());
        }
    }
    let bl = if is_3d {
        Point::new_3d(min_x, min_y, min_z, first.coordinate_system)
    } else {
        Point::new_2d(min_x, min_y, first.coordinate_system)
    };
    let tr = if is_3d {
        Point::new_3d(max_x, max_y, max_z, first.coordinate_system)
    } else {
        Point::new_2d(max_x, max_y, first.coordinate_system)
    };
    let mut bbox_map = Map::new();
    bbox_map.insert("bottomLeft".to_string(), bl.to_json_value());
    bbox_map.insert("topRight".to_string(), tr.to_json_value());
    Ok(SpatialResult::scalar("bbox", Value::Object(bbox_map)))
}

fn distance(args: Vec<Value>) -> Result<SpatialResult> {
    let proc = "spatial.distance";
    let a = require_point(require_arg(&args, 0, "a", proc)?, "a", proc)?;
    let b = require_point(require_arg(&args, 1, "b", proc)?, "b", proc)?;
    if !a.same_crs(&b) {
        return Err(crs_mismatch_err(proc, &a, &b));
    }
    let meters = a.distance_to(&b);
    let cell = serde_json::Number::from_f64(meters)
        .map(Value::Number)
        .unwrap_or(Value::Null);
    Ok(SpatialResult::scalar("meters", cell))
}

fn interpolate(args: Vec<Value>) -> Result<SpatialResult> {
    let proc = "spatial.interpolate";
    let line = require_point_list(require_arg(&args, 0, "line", proc)?, "line", proc)?;
    let frac = require_f64(require_arg(&args, 1, "frac", proc)?, "frac", proc)?;
    if !(0.0..=1.0).contains(&frac) {
        return Err(Error::CypherExecution(format!(
            "ERR_INVALID_ARG_VALUE: {proc} frac must be in [0, 1] (got {frac})"
        )));
    }
    if line.len() < 2 {
        return Err(Error::CypherExecution(format!(
            "ERR_INVALID_ARG_VALUE: {proc} line must contain at least two points"
        )));
    }
    let first = &line[0];
    for (i, p) in line.iter().enumerate().skip(1) {
        if !first.same_crs(p) {
            return Err(Error::CypherExecution(format!(
                "ERR_CRS_MISMATCH: {proc} line[0]={}, line[{i}]={}",
                first.crs_name(),
                p.crs_name()
            )));
        }
    }
    // Piecewise-linear interpolation by cumulative arc length —
    // distance metric comes from `Point::distance_to` so WGS84
    // lines interpolate along great circles' secant approximation
    // and Cartesian lines are exact.
    let mut seg_lens = Vec::with_capacity(line.len() - 1);
    let mut total = 0.0f64;
    for pair in line.windows(2) {
        let d = pair[0].distance_to(&pair[1]);
        total += d;
        seg_lens.push(d);
    }
    if total == 0.0 {
        return Ok(SpatialResult::scalar("point", line[0].to_json_value()));
    }
    let target = total * frac;
    let mut walked = 0.0f64;
    for (i, seg) in seg_lens.iter().enumerate() {
        if walked + seg >= target - f64::EPSILON {
            let local = if *seg > 0.0 {
                (target - walked) / seg
            } else {
                0.0
            };
            let a = &line[i];
            let b = &line[i + 1];
            let x = a.x + (b.x - a.x) * local;
            let y = a.y + (b.y - a.y) * local;
            let point = if a.is_3d() {
                Point::new_3d(x, y, a.z() + (b.z() - a.z()) * local, a.coordinate_system)
            } else {
                Point::new_2d(x, y, a.coordinate_system)
            };
            return Ok(SpatialResult::scalar("point", point.to_json_value()));
        }
        walked += seg;
    }
    // Floating-point slack at frac=1.0 can leave the loop
    // without emitting — fall back to the terminal vertex.
    Ok(SpatialResult::scalar(
        "point",
        line.last().unwrap().to_json_value(),
    ))
}

fn within_bbox(args: Vec<Value>) -> Result<SpatialResult> {
    let proc = "spatial.withinBBox";
    let p = require_point(require_arg(&args, 0, "point", proc)?, "point", proc)?;
    let (bl, tr) = bbox_pair(require_arg(&args, 1, "bbox", proc)?, proc)?;
    if !p.same_crs(&bl) || !p.same_crs(&tr) {
        return Err(Error::CypherExecution(format!(
            "ERR_CRS_MISMATCH: {proc} point={}, bbox=({}, {})",
            p.crs_name(),
            bl.crs_name(),
            tr.crs_name()
        )));
    }
    Ok(SpatialResult::scalar(
        "within",
        Value::Bool(p.within_bbox(&bl, &tr)),
    ))
}

fn within_distance(args: Vec<Value>) -> Result<SpatialResult> {
    let proc = "spatial.withinDistance";
    let a = require_point(require_arg(&args, 0, "a", proc)?, "a", proc)?;
    let b = require_point(require_arg(&args, 1, "b", proc)?, "b", proc)?;
    let d = require_f64(require_arg(&args, 2, "distance", proc)?, "distance", proc)?;
    if !a.same_crs(&b) {
        return Err(crs_mismatch_err(proc, &a, &b));
    }
    Ok(SpatialResult::scalar(
        "within",
        Value::Bool(a.distance_to(&b) <= d),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geospatial::CoordinateSystem;
    use serde_json::json;

    fn cart(x: f64, y: f64) -> Value {
        Point::new_2d(x, y, CoordinateSystem::Cartesian).to_json_value()
    }

    fn wgs(lon: f64, lat: f64) -> Value {
        Point::new_2d(lon, lat, CoordinateSystem::WGS84).to_json_value()
    }

    #[test]
    fn bbox_collapses_points_to_axis_aligned_rect() {
        let r = bbox(vec![Value::Array(vec![
            cart(1.0, 1.0),
            cart(5.0, 2.0),
            cart(3.0, 7.0),
        ])])
        .unwrap();
        let m = r.rows[0][0].as_object().unwrap();
        let bl = Point::from_json_value(&m["bottomLeft"]).unwrap();
        let tr = Point::from_json_value(&m["topRight"]).unwrap();
        assert_eq!((bl.x, bl.y), (1.0, 1.0));
        assert_eq!((tr.x, tr.y), (5.0, 7.0));
    }

    #[test]
    fn bbox_empty_returns_null() {
        let r = bbox(vec![Value::Array(vec![])]).unwrap();
        assert!(r.rows[0][0].is_null());
    }

    #[test]
    fn bbox_rejects_mixed_crs() {
        let err = bbox(vec![Value::Array(vec![cart(0.0, 0.0), wgs(1.0, 1.0)])]).unwrap_err();
        assert!(err.to_string().contains("ERR_CRS_MISMATCH"));
    }

    #[test]
    fn distance_paris_to_berlin_is_close_to_878km() {
        let paris = wgs(2.3522, 48.8566);
        let berlin = wgs(13.4050, 52.5200);
        let r = distance(vec![paris, berlin]).unwrap();
        let meters = r.rows[0][0].as_f64().unwrap();
        assert!((meters - 878_000.0).abs() < 10_000.0, "meters={meters}");
    }

    #[test]
    fn distance_rejects_mixed_crs() {
        let err = distance(vec![cart(0.0, 0.0), wgs(0.0, 0.0)]).unwrap_err();
        assert!(err.to_string().contains("ERR_CRS_MISMATCH"));
    }

    #[test]
    fn within_distance_close_points_match() {
        let p = cart(0.0, 0.0);
        let q = cart(3.0, 4.0);
        let r = within_distance(vec![p, q, json!(10.0)]).unwrap();
        assert_eq!(r.rows[0][0], Value::Bool(true));
    }

    #[test]
    fn within_distance_far_points_do_not_match() {
        let p = cart(0.0, 0.0);
        let q = cart(100.0, 100.0);
        let r = within_distance(vec![p, q, json!(10.0)]).unwrap();
        assert_eq!(r.rows[0][0], Value::Bool(false));
    }

    #[test]
    fn within_bbox_inside_and_outside() {
        let bbox_map = json!({
            "bottomLeft": cart(0.0, 0.0),
            "topRight": cart(10.0, 10.0)
        });
        let inside = within_bbox(vec![cart(5.0, 5.0), bbox_map.clone()]).unwrap();
        assert_eq!(inside.rows[0][0], Value::Bool(true));
        let outside = within_bbox(vec![cart(11.0, 5.0), bbox_map]).unwrap();
        assert_eq!(outside.rows[0][0], Value::Bool(false));
    }

    #[test]
    fn within_bbox_rejects_malformed_map() {
        let err = within_bbox(vec![cart(0.0, 0.0), json!({"a": 1})]).unwrap_err();
        assert!(err.to_string().contains("ERR_BBOX_MALFORMED"));
    }

    #[test]
    fn interpolate_midpoint_of_two_point_line() {
        let line = Value::Array(vec![cart(0.0, 0.0), cart(10.0, 0.0)]);
        let r = interpolate(vec![line, json!(0.5)]).unwrap();
        let p = Point::from_json_value(&r.rows[0][0]).unwrap();
        assert!((p.x - 5.0).abs() < 1e-9);
        assert!((p.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn interpolate_rejects_out_of_range_frac() {
        let line = Value::Array(vec![cart(0.0, 0.0), cart(10.0, 0.0)]);
        let err = interpolate(vec![line, json!(1.5)]).unwrap_err();
        assert!(err.to_string().contains("ERR_INVALID_ARG_VALUE"));
    }

    #[test]
    fn azimuth_due_east_wgs84() {
        let r = azimuth(vec![wgs(0.0, 0.0), wgs(1.0, 0.0)]).unwrap();
        let deg = r.rows[0][0].as_f64().unwrap();
        assert!((deg - 90.0).abs() < 0.1, "deg={deg}");
    }

    #[test]
    fn dispatch_unknown_name_returns_none() {
        assert!(dispatch("spatial.unknownProc", vec![]).unwrap().is_none());
    }

    #[test]
    fn dispatch_non_spatial_prefix_returns_none() {
        assert!(dispatch("apoc.coll.sum", vec![]).unwrap().is_none());
    }
}
