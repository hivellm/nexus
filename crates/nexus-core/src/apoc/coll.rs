//! `apoc.coll.*` — list / set operations (phase6 apoc §2).

use super::{ApocResult, as_list, bad_arg, cmp_values, not_found};
use crate::{Error, Result};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.coll.union",
        "apoc.coll.intersection",
        "apoc.coll.disjunction",
        "apoc.coll.subtract",
        "apoc.coll.sort",
        "apoc.coll.sortNodes",
        "apoc.coll.sortMaps",
        "apoc.coll.shuffle",
        "apoc.coll.reverse",
        "apoc.coll.zip",
        "apoc.coll.pairs",
        "apoc.coll.pairsMin",
        "apoc.coll.combinations",
        "apoc.coll.partitions",
        "apoc.coll.flatten",
        "apoc.coll.frequencies",
        "apoc.coll.frequenciesAsMap",
        "apoc.coll.duplicates",
        "apoc.coll.toSet",
        "apoc.coll.indexOf",
        "apoc.coll.contains",
        "apoc.coll.containsAll",
        "apoc.coll.max",
        "apoc.coll.min",
        "apoc.coll.sum",
        "apoc.coll.avg",
        "apoc.coll.stdev",
        "apoc.coll.remove",
        "apoc.coll.fill",
        "apoc.coll.runningTotal",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "union" => union(args),
        "intersection" => intersection(args),
        "disjunction" => disjunction(args),
        "subtract" => subtract(args),
        "sort" => sort(args),
        "sortNodes" => sort(args), // shares sort semantics — entity id tiebreak in future
        "sortMaps" => sort_maps(args),
        "shuffle" => shuffle(args),
        "reverse" => reverse(args),
        "zip" => zip(args),
        "pairs" => pairs(args),
        "pairsMin" => pairs_min(args),
        "combinations" => combinations(args),
        "partitions" => partitions(args),
        "flatten" => flatten(args),
        "frequencies" => frequencies(args),
        "frequenciesAsMap" => frequencies_as_map(args),
        "duplicates" => duplicates(args),
        "toSet" => to_set(args),
        "indexOf" => index_of(args),
        "contains" => contains(args),
        "containsAll" => contains_all(args),
        "max" => reduce_max(args),
        "min" => reduce_min(args),
        "sum" => reduce_sum(args),
        "avg" => reduce_avg(args),
        "stdev" => reduce_stdev(args),
        "remove" => remove(args),
        "fill" => fill(args),
        "runningTotal" => running_total(args),
        _ => Err(not_found(&format!("apoc.coll.{proc}"))),
    }
}

// ─────────────────────────── set operations ───────────────────────────

fn union(args: Vec<Value>) -> Result<ApocResult> {
    let a = args.first().map(as_list).unwrap_or_default();
    let b = args.get(1).map(as_list).unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for v in a.into_iter().chain(b.into_iter()) {
        if !out.iter().any(|x| x == &v) {
            out.push(v);
        }
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn intersection(args: Vec<Value>) -> Result<ApocResult> {
    let a = args.first().map(as_list).unwrap_or_default();
    let b = args.get(1).map(as_list).unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for v in a {
        if b.contains(&v) && !out.contains(&v) {
            out.push(v);
        }
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn disjunction(args: Vec<Value>) -> Result<ApocResult> {
    // Symmetric difference: (A ∪ B) \ (A ∩ B).
    let a = args.first().map(as_list).unwrap_or_default();
    let b = args.get(1).map(as_list).unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for v in &a {
        if !b.contains(v) && !out.contains(v) {
            out.push(v.clone());
        }
    }
    for v in &b {
        if !a.contains(v) && !out.contains(v) {
            out.push(v.clone());
        }
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn subtract(args: Vec<Value>) -> Result<ApocResult> {
    let a = args.first().map(as_list).unwrap_or_default();
    let b = args.get(1).map(as_list).unwrap_or_default();
    let out: Vec<Value> = a.into_iter().filter(|v| !b.contains(v)).collect();
    let mut dedup: Vec<Value> = Vec::new();
    for v in out {
        if !dedup.contains(&v) {
            dedup.push(v);
        }
    }
    Ok(ApocResult::scalar(Value::Array(dedup)))
}

// ───────────────────────────── ordering ────────────────────────────────

fn sort(args: Vec<Value>) -> Result<ApocResult> {
    let mut xs = args.first().map(as_list).unwrap_or_default();
    xs.sort_by(cmp_values);
    Ok(ApocResult::scalar(Value::Array(xs)))
}

fn sort_maps(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let key = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg("apoc.coll.sortMaps", "second arg must be a STRING key"))?
        .to_string();
    let mut items: Vec<Value> = xs;
    items.sort_by(|a, b| {
        let (ax, bx) = match (a, b) {
            (Value::Object(a), Value::Object(b)) => (
                a.get(&key).unwrap_or(&Value::Null).clone(),
                b.get(&key).unwrap_or(&Value::Null).clone(),
            ),
            _ => (Value::Null, Value::Null),
        };
        cmp_values(&ax, &bx)
    });
    Ok(ApocResult::scalar(Value::Array(items)))
}

fn shuffle(args: Vec<Value>) -> Result<ApocResult> {
    use rand::seq::SliceRandom;
    let mut xs = args.first().map(as_list).unwrap_or_default();
    let mut rng = rand::thread_rng();
    xs.shuffle(&mut rng);
    Ok(ApocResult::scalar(Value::Array(xs)))
}

fn reverse(args: Vec<Value>) -> Result<ApocResult> {
    let mut xs = args.first().map(as_list).unwrap_or_default();
    xs.reverse();
    Ok(ApocResult::scalar(Value::Array(xs)))
}

// ─────────────────────────── pair / combine ───────────────────────────

fn zip(args: Vec<Value>) -> Result<ApocResult> {
    let a = args.first().map(as_list).unwrap_or_default();
    let b = args.get(1).map(as_list).unwrap_or_default();
    let out: Vec<Value> = a
        .into_iter()
        .zip(b)
        .map(|(x, y)| Value::Array(vec![x, y]))
        .collect();
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn pairs(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut out: Vec<Value> = Vec::with_capacity(xs.len());
    for i in 0..xs.len() {
        let a = xs[i].clone();
        let b = xs.get(i + 1).cloned().unwrap_or(Value::Null);
        out.push(Value::Array(vec![a, b]));
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn pairs_min(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    if xs.len() < 2 {
        return Ok(ApocResult::scalar(Value::Array(Vec::new())));
    }
    let out: Vec<Value> = xs
        .windows(2)
        .map(|w| Value::Array(vec![w[0].clone(), w[1].clone()]))
        .collect();
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn combinations(args: Vec<Value>) -> Result<ApocResult> {
    // combinations(list, min, max) — every contiguous sub-list
    // whose length is in [min, max]. Matches APOC's semantics.
    let xs = args.first().map(as_list).unwrap_or_default();
    let min = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(1) as usize;
    let max = args
        .get(2)
        .and_then(|v| v.as_i64())
        .unwrap_or(xs.len() as i64)
        .max(0) as usize;
    let mut out: Vec<Value> = Vec::new();
    for len in min..=max.min(xs.len()) {
        for start in 0..=xs.len().saturating_sub(len) {
            out.push(Value::Array(xs[start..start + len].to_vec()));
        }
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn partitions(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let size = args
        .get(1)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| bad_arg("apoc.coll.partitions", "second arg must be an INTEGER size"))?;
    let size = size.max(1) as usize;
    let out: Vec<Value> = xs.chunks(size).map(|c| Value::Array(c.to_vec())).collect();
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn flatten(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let deep = args.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
    let mut out: Vec<Value> = Vec::new();
    for v in xs {
        flatten_into(v, deep, &mut out);
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn flatten_into(v: Value, deep: bool, out: &mut Vec<Value>) {
    match v {
        Value::Array(inner) => {
            if deep {
                for x in inner {
                    flatten_into(x, true, out);
                }
            } else {
                out.extend(inner);
            }
        }
        other => out.push(other),
    }
}

// ─────────────────────────── statistics ───────────────────────────

fn frequencies(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut counts: Vec<(Value, i64)> = Vec::new();
    for v in xs {
        if let Some(slot) = counts.iter_mut().find(|(k, _)| k == &v) {
            slot.1 += 1;
        } else {
            counts.push((v, 1));
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    let rows: Vec<Value> = counts
        .into_iter()
        .map(|(item, count)| {
            let mut m = Map::new();
            m.insert("item".to_string(), item);
            m.insert("count".to_string(), Value::Number(count.into()));
            Value::Object(m)
        })
        .collect();
    Ok(ApocResult::scalar(Value::Array(rows)))
}

fn frequencies_as_map(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut counts: HashMap<String, i64> = HashMap::new();
    for v in xs {
        *counts.entry(value_key(&v)).or_insert(0) += 1;
    }
    let mut m = Map::new();
    for (k, v) in counts {
        m.insert(k, Value::Number(v.into()));
    }
    Ok(ApocResult::scalar(Value::Object(m)))
}

fn value_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn duplicates(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut seen: Vec<Value> = Vec::new();
    let mut dup: Vec<Value> = Vec::new();
    for v in xs {
        if seen.contains(&v) {
            if !dup.contains(&v) {
                dup.push(v);
            }
        } else {
            seen.push(v);
        }
    }
    Ok(ApocResult::scalar(Value::Array(dup)))
}

fn to_set(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut out: Vec<Value> = Vec::new();
    for v in xs {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn index_of(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let needle = args.get(1).cloned().unwrap_or(Value::Null);
    let idx = xs
        .iter()
        .position(|v| v == &needle)
        .map(|i| Value::Number((i as i64).into()))
        .unwrap_or(Value::Number((-1i64).into()));
    Ok(ApocResult::scalar(idx))
}

fn contains(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let needle = args.get(1).cloned().unwrap_or(Value::Null);
    Ok(ApocResult::scalar(Value::Bool(xs.contains(&needle))))
}

fn contains_all(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let needles = args.get(1).map(as_list).unwrap_or_default();
    let ok = needles.iter().all(|n| xs.contains(n));
    Ok(ApocResult::scalar(Value::Bool(ok)))
}

// ─────────────────────────── reductions ───────────────────────────

fn reduce_max(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let m = xs.into_iter().max_by(cmp_values).unwrap_or(Value::Null);
    Ok(ApocResult::scalar(m))
}

fn reduce_min(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let m = xs.into_iter().min_by(cmp_values).unwrap_or(Value::Null);
    Ok(ApocResult::scalar(m))
}

fn reduce_sum(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut all_int = true;
    let mut s_int: i64 = 0;
    let mut s_float: f64 = 0.0;
    for v in &xs {
        match v {
            Value::Number(n) if n.is_i64() && all_int => s_int += n.as_i64().unwrap_or(0),
            Value::Number(n) => {
                all_int = false;
                s_float += n.as_f64().unwrap_or(0.0);
            }
            Value::Null => {}
            _ => {
                return Err(Error::CypherExecution(format!(
                    "apoc.coll.sum: non-numeric element {v}"
                )));
            }
        }
    }
    let out = if all_int {
        Value::Number(s_int.into())
    } else {
        // Fold in the integer partial if we switched mid-stream.
        let total = s_int as f64 + s_float;
        serde_json::Number::from_f64(total)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    };
    Ok(ApocResult::scalar(out))
}

fn reduce_avg(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let nums: Vec<f64> = xs.into_iter().filter_map(|v| v.as_f64()).collect();
    if nums.is_empty() {
        return Ok(ApocResult::scalar(Value::Null));
    }
    let sum: f64 = nums.iter().sum();
    let avg = sum / nums.len() as f64;
    Ok(ApocResult::scalar(
        serde_json::Number::from_f64(avg)
            .map(Value::Number)
            .unwrap_or(Value::Null),
    ))
}

fn reduce_stdev(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let nums: Vec<f64> = xs.into_iter().filter_map(|v| v.as_f64()).collect();
    if nums.len() < 2 {
        return Ok(ApocResult::scalar(Value::Null));
    }
    let mean = nums.iter().sum::<f64>() / nums.len() as f64;
    let var = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nums.len() - 1) as f64;
    let sd = var.sqrt();
    Ok(ApocResult::scalar(
        serde_json::Number::from_f64(sd)
            .map(Value::Number)
            .unwrap_or(Value::Null),
    ))
}

fn remove(args: Vec<Value>) -> Result<ApocResult> {
    // apoc.coll.remove(list, index, count=1)
    let mut xs = args.first().map(as_list).unwrap_or_default();
    let idx = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0).max(0) as usize;
    let count = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize;
    let end = (idx + count).min(xs.len());
    if idx < xs.len() {
        xs.drain(idx..end);
    }
    Ok(ApocResult::scalar(Value::Array(xs)))
}

fn fill(args: Vec<Value>) -> Result<ApocResult> {
    let value = args.first().cloned().unwrap_or(Value::Null);
    let count = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0).max(0) as usize;
    let out = vec![value; count];
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn running_total(args: Vec<Value>) -> Result<ApocResult> {
    let xs = args.first().map(as_list).unwrap_or_default();
    let mut running: f64 = 0.0;
    let mut all_int = true;
    let mut out: Vec<Value> = Vec::with_capacity(xs.len());
    for v in xs {
        match &v {
            Value::Number(n) => {
                if n.is_f64() {
                    all_int = false;
                }
                running += n.as_f64().unwrap_or(0.0);
                out.push(running_value(all_int, running));
            }
            _ => {
                return Err(Error::CypherExecution(format!(
                    "apoc.coll.runningTotal: non-numeric element {v}"
                )));
            }
        }
    }
    Ok(ApocResult::scalar(Value::Array(out)))
}

fn running_value(all_int: bool, running: f64) -> Value {
    if all_int && running.fract() == 0.0 && running.is_finite() {
        Value::Number((running as i64).into())
    } else {
        serde_json::Number::from_f64(running)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn union_deduplicates_preserving_order() {
        assert_eq!(
            call("union", vec![json!([1, 2, 3]), json!([3, 4, 5])]),
            json!([1, 2, 3, 4, 5])
        );
    }

    #[test]
    fn union_null_is_empty() {
        assert_eq!(
            call("union", vec![json!([1, 2]), Value::Null]),
            json!([1, 2])
        );
    }

    #[test]
    fn intersection_preserves_left_order() {
        assert_eq!(
            call("intersection", vec![json!([1, 2, 3, 4]), json!([3, 4, 5])]),
            json!([3, 4])
        );
    }

    #[test]
    fn disjunction_is_symmetric_difference() {
        assert_eq!(
            call("disjunction", vec![json!([1, 2, 3]), json!([2, 3, 4])]),
            json!([1, 4])
        );
    }

    #[test]
    fn subtract_removes_right_elements() {
        assert_eq!(
            call("subtract", vec![json!([1, 2, 3, 4]), json!([2, 4])]),
            json!([1, 3])
        );
    }

    #[test]
    fn sort_integers_asc() {
        assert_eq!(call("sort", vec![json!([3, 1, 2])]), json!([1, 2, 3]));
    }

    #[test]
    fn sort_mixed_types_by_type_ordinal() {
        // null < bool < int < float < string per Neo4j rule.
        assert_eq!(
            call("sort", vec![json!([1.5, 1, "a", true])]),
            json!([true, 1, 1.5, "a"])
        );
    }

    #[test]
    fn sort_maps_by_key() {
        let out = call(
            "sortMaps",
            vec![json!([{"g": "b"}, {"g": "a"}, {"g": "c"}]), json!("g")],
        );
        assert_eq!(out, json!([{"g": "a"}, {"g": "b"}, {"g": "c"}]));
    }

    #[test]
    fn reverse_reverses() {
        assert_eq!(call("reverse", vec![json!([1, 2, 3])]), json!([3, 2, 1]));
    }

    #[test]
    fn zip_pairs_elements() {
        assert_eq!(
            call("zip", vec![json!([1, 2, 3]), json!(["a", "b", "c"])]),
            json!([[1, "a"], [2, "b"], [3, "c"]])
        );
    }

    #[test]
    fn pairs_includes_trailing_null() {
        assert_eq!(
            call("pairs", vec![json!([1, 2, 3])]),
            json!([[1, 2], [2, 3], [3, null]])
        );
    }

    #[test]
    fn pairs_min_omits_trailing_null() {
        assert_eq!(
            call("pairsMin", vec![json!([1, 2, 3])]),
            json!([[1, 2], [2, 3]])
        );
    }

    #[test]
    fn partitions_chunks_list() {
        assert_eq!(
            call("partitions", vec![json!([1, 2, 3, 4, 5]), json!(2)]),
            json!([[1, 2], [3, 4], [5]])
        );
    }

    #[test]
    fn flatten_one_level_by_default() {
        assert_eq!(
            call("flatten", vec![json!([[1, 2], [3, [4, 5]]])]),
            json!([1, 2, 3, [4, 5]])
        );
    }

    #[test]
    fn flatten_deep_mode_recurses() {
        assert_eq!(
            call("flatten", vec![json!([[1, 2], [3, [4, 5]]]), json!(true)]),
            json!([1, 2, 3, 4, 5])
        );
    }

    #[test]
    fn frequencies_sorts_desc_by_count() {
        let out = call("frequencies", vec![json!(["a", "b", "a", "c", "a", "b"])]);
        assert_eq!(
            out,
            json!([
                {"item": "a", "count": 3},
                {"item": "b", "count": 2},
                {"item": "c", "count": 1}
            ])
        );
    }

    #[test]
    fn duplicates_returns_repeat_elements_only() {
        assert_eq!(
            call("duplicates", vec![json!([1, 2, 2, 3, 3, 3])]),
            json!([2, 3])
        );
    }

    #[test]
    fn to_set_preserves_first_occurrence_order() {
        assert_eq!(
            call("toSet", vec![json!([1, 2, 2, 3, 1])]),
            json!([1, 2, 3])
        );
    }

    #[test]
    fn index_of_returns_minus_one_when_missing() {
        assert_eq!(call("indexOf", vec![json!([1, 2, 3]), json!(9)]), json!(-1));
    }

    #[test]
    fn contains_true_when_present() {
        assert_eq!(
            call("contains", vec![json!([1, 2, 3]), json!(2)]),
            json!(true)
        );
    }

    #[test]
    fn contains_all_requires_every_element() {
        assert_eq!(
            call("containsAll", vec![json!([1, 2, 3, 4]), json!([2, 3])]),
            json!(true)
        );
        assert_eq!(
            call("containsAll", vec![json!([1, 2, 3, 4]), json!([2, 5])]),
            json!(false)
        );
    }

    #[test]
    fn reductions() {
        assert_eq!(call("max", vec![json!([3, 1, 2])]), json!(3));
        assert_eq!(call("min", vec![json!([3, 1, 2])]), json!(1));
        assert_eq!(call("sum", vec![json!([1, 2, 3])]), json!(6));
        assert_eq!(call("avg", vec![json!([1, 2, 3])]), json!(2.0));
    }

    #[test]
    fn sum_rejects_non_numeric() {
        let err = dispatch("sum", vec![json!([1, "two"])]).unwrap_err();
        assert!(err.to_string().contains("non-numeric"));
    }

    #[test]
    fn remove_drops_range() {
        assert_eq!(
            call("remove", vec![json!([1, 2, 3, 4, 5]), json!(1), json!(2)]),
            json!([1, 4, 5])
        );
    }

    #[test]
    fn fill_produces_repeated_value() {
        assert_eq!(
            call("fill", vec![json!("x"), json!(3)]),
            json!(["x", "x", "x"])
        );
    }

    #[test]
    fn running_total_accumulates() {
        assert_eq!(
            call("runningTotal", vec![json!([1, 2, 3, 4])]),
            json!([1, 3, 6, 10])
        );
    }

    #[test]
    fn shuffle_preserves_length_and_elements() {
        let input = json!([1, 2, 3, 4, 5]);
        let out = call("shuffle", vec![input.clone()]);
        let mut a = input.as_array().unwrap().clone();
        let mut b = out.as_array().unwrap().clone();
        a.sort_by(cmp_values);
        b.sort_by(cmp_values);
        assert_eq!(a, b);
    }
}
