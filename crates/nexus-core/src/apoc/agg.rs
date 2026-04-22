//! `apoc.agg.*` — aggregation / statistical procedures that operate on
//! a materialised list rather than a stream. Procedures that need
//! streaming (`first`, `last`, `nth`, `minItems`, `maxItems`) also
//! land here — taking the pre-materialised list keeps the shape
//! symmetric with the other pure-value APOC namespaces.

use super::{ApocResult, as_list, bad_arg, cmp_values, not_found};
use crate::Result;
use serde_json::{Map, Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.agg.statistics",
        "apoc.agg.percentiles",
        "apoc.agg.median",
        "apoc.agg.mode",
        "apoc.agg.nth",
        "apoc.agg.first",
        "apoc.agg.last",
        "apoc.agg.maxItems",
        "apoc.agg.minItems",
        "apoc.agg.product",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "statistics" => statistics(args),
        "percentiles" => percentiles(args),
        "median" => median(args),
        "mode" => mode(args),
        "nth" => nth(args),
        "first" => first(args),
        "last" => last(args),
        "maxItems" => max_items(args),
        "minItems" => min_items(args),
        "product" => product(args),
        _ => Err(not_found(&format!("apoc.agg.{proc}"))),
    }
}

fn numeric_list(v: &Value) -> Vec<f64> {
    as_list(v).into_iter().filter_map(|x| x.as_f64()).collect()
}

fn statistics(args: Vec<Value>) -> Result<ApocResult> {
    let xs = numeric_list(&args.first().cloned().unwrap_or(Value::Null));
    if xs.is_empty() {
        return Ok(ApocResult::scalar(Value::Null));
    }
    let n = xs.len() as f64;
    let sum: f64 = xs.iter().sum();
    let mean = sum / n;
    let min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let stdev = var.sqrt();
    let mut sorted = xs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_v = percentile_of(&sorted, 0.5);
    let p75 = percentile_of(&sorted, 0.75);
    let p90 = percentile_of(&sorted, 0.90);
    let p95 = percentile_of(&sorted, 0.95);
    let p99 = percentile_of(&sorted, 0.99);
    let mut m = Map::new();
    m.insert("total".to_string(), json!(n as i64));
    m.insert("min".to_string(), num_or_null(min));
    m.insert("max".to_string(), num_or_null(max));
    m.insert("mean".to_string(), num_or_null(mean));
    m.insert("stdev".to_string(), num_or_null(stdev));
    m.insert("median".to_string(), num_or_null(median_v));
    m.insert("percentile_75".to_string(), num_or_null(p75));
    m.insert("percentile_90".to_string(), num_or_null(p90));
    m.insert("percentile_95".to_string(), num_or_null(p95));
    m.insert("percentile_99".to_string(), num_or_null(p99));
    Ok(ApocResult::scalar(Value::Object(m)))
}

fn percentiles(args: Vec<Value>) -> Result<ApocResult> {
    let mut xs = numeric_list(&args.first().cloned().unwrap_or(Value::Null));
    if xs.is_empty() {
        return Ok(ApocResult::scalar(Value::Array(Vec::new())));
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ps: Vec<f64> = args
        .get(1)
        .map(|v| {
            as_list(v)
                .iter()
                .filter_map(|x| x.as_f64())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![0.5, 0.75, 0.9, 0.95, 0.99]);
    let out: Vec<Value> = ps
        .iter()
        .map(|p| num_or_null(percentile_of(&xs, *p)))
        .collect();
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn percentile_of(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let p = p.clamp(0.0, 1.0);
    let idx = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn num_or_null(f: f64) -> Value {
    if f.is_finite() {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

fn median(args: Vec<Value>) -> Result<ApocResult> {
    let mut xs = numeric_list(&args.first().cloned().unwrap_or(Value::Null));
    if xs.is_empty() {
        return Ok(ApocResult::scalar(Value::Null));
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = xs.len();
    let v = if n.is_multiple_of(2) {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    } else {
        xs[n / 2]
    };
    Ok(ApocResult::scalar(num_or_null(v)))
}

fn mode(args: Vec<Value>) -> Result<ApocResult> {
    let xs = as_list(&args.first().cloned().unwrap_or(Value::Null));
    if xs.is_empty() {
        return Ok(ApocResult::scalar(Value::Null));
    }
    let mut counts: Vec<(Value, i64)> = Vec::new();
    for v in xs {
        if let Some(slot) = counts.iter_mut().find(|(k, _)| k == &v) {
            slot.1 += 1;
        } else {
            counts.push((v, 1));
        }
    }
    let max = counts.iter().max_by_key(|(_, c)| *c).cloned();
    Ok(ApocResult::scalar(
        max.map(|(v, _)| v).unwrap_or(Value::Null),
    ))
}

fn nth(args: Vec<Value>) -> Result<ApocResult> {
    let xs = as_list(&args.first().cloned().unwrap_or(Value::Null));
    let n = args
        .get(1)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| bad_arg("apoc.agg.nth", "arg 1 must be INTEGER"))?;
    let idx = if n < 0 {
        (xs.len() as i64 + n).max(0) as usize
    } else {
        n as usize
    };
    Ok(ApocResult::scalar(
        xs.get(idx).cloned().unwrap_or(Value::Null),
    ))
}

fn first(args: Vec<Value>) -> Result<ApocResult> {
    let xs = as_list(&args.first().cloned().unwrap_or(Value::Null));
    Ok(ApocResult::scalar(
        xs.first().cloned().unwrap_or(Value::Null),
    ))
}

fn last(args: Vec<Value>) -> Result<ApocResult> {
    let xs = as_list(&args.first().cloned().unwrap_or(Value::Null));
    Ok(ApocResult::scalar(
        xs.last().cloned().unwrap_or(Value::Null),
    ))
}

fn max_items(args: Vec<Value>) -> Result<ApocResult> {
    // maxItems(list, key, limit=1) — return up to `limit` items with
    // the largest `key` value (map lookup) or the largest scalar
    // when no key is supplied.
    let mut xs = as_list(&args.first().cloned().unwrap_or(Value::Null));
    let key = args.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());
    let limit = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1).max(1) as usize;
    xs.sort_by(|a, b| cmp_by_key(b, a, key.as_deref()));
    xs.truncate(limit);
    Ok(ApocResult::scalar(Value::Array(xs)))
}

fn min_items(args: Vec<Value>) -> Result<ApocResult> {
    let mut xs = as_list(&args.first().cloned().unwrap_or(Value::Null));
    let key = args.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());
    let limit = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1).max(1) as usize;
    xs.sort_by(|a, b| cmp_by_key(a, b, key.as_deref()));
    xs.truncate(limit);
    Ok(ApocResult::scalar(Value::Array(xs)))
}

fn cmp_by_key(a: &Value, b: &Value, key: Option<&str>) -> std::cmp::Ordering {
    match key {
        Some(k) => {
            let ax = match a {
                Value::Object(m) => m.get(k).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            let bx = match b {
                Value::Object(m) => m.get(k).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            cmp_values(&ax, &bx)
        }
        None => cmp_values(a, b),
    }
}

fn product(args: Vec<Value>) -> Result<ApocResult> {
    let xs = numeric_list(&args.first().cloned().unwrap_or(Value::Null));
    if xs.is_empty() {
        return Ok(ApocResult::scalar(Value::Null));
    }
    let p: f64 = xs.iter().product();
    Ok(ApocResult::scalar(num_or_null(p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn statistics_on_integer_list() {
        let out = call("statistics", vec![json!([1, 2, 3, 4, 5])]);
        let m = out.as_object().unwrap();
        assert_eq!(m.get("total"), Some(&json!(5)));
        assert_eq!(m.get("min"), Some(&json!(1.0)));
        assert_eq!(m.get("max"), Some(&json!(5.0)));
        assert_eq!(m.get("mean"), Some(&json!(3.0)));
        assert_eq!(m.get("median"), Some(&json!(3.0)));
    }

    #[test]
    fn percentiles_default_set() {
        let out = call("percentiles", vec![json!([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])]);
        // p * (n-1) with n=10: p=0.5→4.5→round=5 (xs[5]=6),
        // 0.75→6.75→7 (xs[7]=8), 0.9→8.1→8 (xs[8]=9), 0.95→8.55→9
        // (xs[9]=10), 0.99→8.91→9 (xs[9]=10).
        assert_eq!(out, json!([6.0, 8.0, 9.0, 10.0, 10.0]));
    }

    #[test]
    fn median_even_count_averages_middle_two() {
        assert_eq!(call("median", vec![json!([1, 2, 3, 4])]), json!(2.5));
    }

    #[test]
    fn median_odd_count_picks_middle() {
        assert_eq!(call("median", vec![json!([1, 2, 3, 4, 5])]), json!(3.0));
    }

    #[test]
    fn mode_returns_most_frequent() {
        assert_eq!(call("mode", vec![json!([1, 2, 2, 3, 2, 4])]), json!(2));
    }

    #[test]
    fn nth_supports_negative_index() {
        assert_eq!(
            call("nth", vec![json!(["a", "b", "c"]), json!(-1)]),
            json!("c")
        );
    }

    #[test]
    fn first_and_last() {
        assert_eq!(call("first", vec![json!([1, 2, 3])]), json!(1));
        assert_eq!(call("last", vec![json!([1, 2, 3])]), json!(3));
    }

    #[test]
    fn max_items_picks_top_n_by_key() {
        let out = call(
            "maxItems",
            vec![
                json!([{"score": 3}, {"score": 1}, {"score": 2}]),
                json!("score"),
                json!(2),
            ],
        );
        assert_eq!(out, json!([{"score": 3}, {"score": 2}]));
    }

    #[test]
    fn min_items_picks_bottom_n() {
        let out = call(
            "minItems",
            vec![json!([{"s": 3}, {"s": 1}, {"s": 2}]), json!("s"), json!(2)],
        );
        assert_eq!(out, json!([{"s": 1}, {"s": 2}]));
    }

    #[test]
    fn product_of_ints() {
        assert_eq!(call("product", vec![json!([2, 3, 4])]), json!(24.0));
    }

    #[test]
    fn empty_list_returns_null() {
        assert_eq!(call("median", vec![json!([])]), Value::Null);
        assert_eq!(call("mode", vec![json!([])]), Value::Null);
        assert_eq!(call("product", vec![json!([])]), Value::Null);
    }
}
